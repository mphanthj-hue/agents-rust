use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::sync::LazyLock;
use std::sync::atomic::{AtomicU64, Ordering};
use crate::mcp::types::{ToolDefinition, ToolResult, ToolContent};

const REMOTE_SERVERS: &[(&str, &str)] = &[
    ("grep_app", "https://mcp.grep.app"),
    ("websearch", "https://mcp.exa.ai/mcp?tools=web_search_exa"),
    ("ms-learn", "https://learn.microsoft.com/api/mcp"),
];

const LOCAL_SERVERS: &[(&str, &[&str])] = &[
    ("context7", &["npx", "@upstash/context7-mcp@latest"]),
    ("obscura", &["npx", "@obscuraai/mcp-server"]),
];

static NEXT_ID: LazyLock<AtomicU64> = LazyLock::new(|| AtomicU64::new(100));

fn next_id() -> u64 {
    NEXT_ID.fetch_add(1, Ordering::SeqCst)
}

pub fn call_mcp_server_definition() -> ToolDefinition {
    let servers: Vec<String> = REMOTE_SERVERS.iter()
        .map(|(name, _)| name.to_string())
        .chain(LOCAL_SERVERS.iter().map(|(name, _)| name.to_string()))
        .collect();
    let servers_str = servers.join(", ");

    ToolDefinition {
        name: "call_mcp_server".into(),
        description: format!(
            "Call a tool on another MCP server. Supports: {}. \
            Use to search web, look up docs, search code, browse with CDP, etc.\n\n\
            Examples:\n\
            - grep_app searchGitHub: {{\"query\": \"useState(\", \"language\": [\"TypeScript\"]}}\n\
            - websearch web_search_exa: {{\"query\": \"rust async tutorial\"}}\n\
            - context7 resolve-library-id: {{\"libraryName\": \"Next.js\", \"query\": \"app router\"}}\n\
            - obscura browser_action: {{\"action\": \"navigate\", \"url\": \"https://...\"}}",
            servers_str
        ),
        input_schema: json!({
            "type": "object",
            "properties": {
                "server": {
                    "type": "string",
                    "description": "MCP server name",
                    "enum": servers
                },
                "tool": {
                    "type": "string",
                    "description": "Tool name to call on the server"
                },
                "arguments": {
                    "type": "object",
                    "description": "Arguments to pass to the tool",
                    "default": {}
                }
            },
            "required": ["server", "tool"]
        }),
    }
}

pub fn handle_call_mcp_server(args: Value) -> Result<ToolResult, String> {
    let server_name = args.get("server")
        .and_then(|v| v.as_str())
        .ok_or("Missing: server")?;
    let tool_name = args.get("tool")
        .and_then(|v| v.as_str())
        .ok_or("Missing: tool")?;
    let tool_args = args.get("arguments")
        .filter(|v| v.is_object())
        .cloned()
        .unwrap_or(json!({}));

    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async {
            call_server(server_name, tool_name, tool_args).await
        })
    })
}

async fn call_server(server: &str, tool: &str, args: Value) -> Result<ToolResult, String> {
    if let Some(&(_, url)) = REMOTE_SERVERS.iter().find(|(n, _)| *n == server) {
        call_remote(url, tool, args).await
    } else if let Some(&(_, cmd)) = LOCAL_SERVERS.iter().find(|(n, _)| *n == server) {
        call_local(cmd, tool, args).await
    } else {
        Err(format!("Unknown server: {}. Available: grep_app, websearch, context7, obscura, ms-learn", server))
    }
}

async fn call_remote(url: &str, tool: &str, args: Value) -> Result<ToolResult, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .user_agent("agents-rust/0.1.0")
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    let init_req = json!({
        "jsonrpc": "2.0",
        "id": next_id(),
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": { "name": "agents-rust", "version": "0.1.0" }
        }
    });

    let init_resp = send_sse_request(&client, url, init_req).await?;
    if init_resp.get("error").is_some() {
        return Err(format!("Initialize error: {}", init_resp));
    }

    let call_req = json!({
        "jsonrpc": "2.0",
        "id": next_id(),
        "method": "tools/call",
        "params": {
            "name": tool,
            "arguments": args
        }
    });

    let call_resp = send_sse_request(&client, url, call_req).await?;

    if let Some(error) = call_resp.get("error") {
        let msg = error.get("message").and_then(|v| v.as_str()).unwrap_or("");
        return Err(format!("{} error: {}", tool, msg));
    }

    let result = call_resp.get("result")
        .ok_or("No result in response")?;
    let content = result.get("content").and_then(|v| v.as_array())
        .ok_or("No content in result")?;

    let mut texts = Vec::new();
    for item in content {
        if let Some(text) = item.get("text").and_then(|v| v.as_str()) {
            texts.push(text.to_string());
        }
    }

    let combined = if texts.len() == 1 {
        texts.into_iter().next().unwrap()
    } else {
        texts.join("\n\n---\n\n")
    };

    Ok(ToolResult {
        content: vec![ToolContent::Text { text: combined }],
        is_error: Some(false),
    })
}

async fn send_sse_request(client: &reqwest::Client, url: &str, body: Value) -> Result<Value, String> {
    let response = client.post(url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("HTTP request failed: {}", e))?;

    let status = response.status();
    if !status.is_success() {
        let text = response.text().await.unwrap_or_default();
        return Err(format!("HTTP {}: {}", status, text));
    }

    let content_type = response.headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if content_type.contains("text/event-stream") {
        let bytes = response.bytes().await
            .map_err(|e| format!("Failed to read SSE body: {}", e))?;
        let text = String::from_utf8_lossy(&bytes);
        parse_sse_event(&text)
    } else {
        response.json::<Value>().await
            .map_err(|e| format!("Failed to parse JSON: {}", e))
    }
}

fn parse_sse_event(data: &str) -> Result<Value, String> {
    let mut last_json = None;
    for line in data.lines() {
        if let Some(json_str) = line.strip_prefix("data: ") {
            if let Ok(val) = serde_json::from_str::<Value>(json_str) {
                last_json = Some(val);
            }
        }
    }
    last_json.ok_or_else(|| {
        let preview: String = data.chars().take(200).collect();
        format!("No valid SSE data found: {}", preview)
    })
}

async fn call_local(cmd: &[&str], tool: &str, args: Value) -> Result<ToolResult, String> {
    let mut child = Command::new(cmd[0])
        .args(&cmd[1..])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn {}: {}", cmd[0], e))?;

    let stdin = child.stdin.as_mut().ok_or("Failed to get stdin")?;
    let stdout = child.stdout.take().ok_or("Failed to get stdout")?;

    let init_req = json!({
        "jsonrpc": "2.0",
        "id": next_id(),
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": { "name": "agents-rust", "version": "0.1.0" }
        }
    });

    writeln!(stdin, "{}", serde_json::to_string(&init_req).unwrap())
        .map_err(|e| format!("Failed to write stdin: {}", e))?;

    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    reader.read_line(&mut line)
        .map_err(|e| format!("Failed to read stdout: {}", e))?;

    let init_resp: Value = serde_json::from_str(&line)
        .map_err(|e| format!("Failed to parse init: {}", e))?;

    if init_resp.get("error").is_some() {
        return Err(format!("Init error: {}", init_resp));
    }

    let call_req = json!({
        "jsonrpc": "2.0",
        "id": next_id(),
        "method": "tools/call",
        "params": { "name": tool, "arguments": args }
    });

    writeln!(stdin, "{}", serde_json::to_string(&call_req).unwrap())
        .map_err(|e| format!("Failed to write stdin: {}", e))?;

    line.clear();
    reader.read_line(&mut line)
        .map_err(|e| format!("Failed to read stdout: {}", e))?;

    drop(stdin);
    let _ = child.wait();

    let call_resp: Value = serde_json::from_str(&line)
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    if let Some(error) = call_resp.get("error") {
        let msg = error.get("message").and_then(|v| v.as_str()).unwrap_or("");
        return Err(format!("{} error: {}", tool, msg));
    }

    let result = call_resp.get("result")
        .ok_or("No result in response")?;
    let content = result.get("content").and_then(|v| v.as_array())
        .ok_or("No content in result")?;

    let mut texts = Vec::new();
    for item in content {
        if let Some(text) = item.get("text").and_then(|v| v.as_str()) {
            texts.push(text.to_string());
        }
    }

    let combined = if texts.len() == 1 {
        texts.into_iter().next().unwrap()
    } else {
        texts.join("\n\n---\n\n")
    };

    Ok(ToolResult {
        content: vec![ToolContent::Text { text: combined }],
        is_error: Some(false),
    })
}
