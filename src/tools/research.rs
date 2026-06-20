use serde_json::{json, Value};
use crate::mcp::types::{ToolDefinition, ToolResult, ToolContent};

pub fn deep_research_definition() -> ToolDefinition {
    ToolDefinition {
        name: "deep_research".into(),
        description:
            "Perform multi-source research on a topic. Calls grep.app for code, \
            fetches web pages for context, then synthesizes via LLM.\n\n\
            How it works:\n\
            1. Searches GitHub code (via grep.app)\n\
            2. Fetches web pages for context\n\
            3. Generates a synthesized report using LLM"
        .into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Research query" },
                "depth": {
                    "type": "integer", "description": "1=quick (code+LLM), 2=standard (+web fetch)",
                    "default": 1, "minimum": 1, "maximum": 2
                }
            },
            "required": ["query"]
        }),
    }
}

pub fn handle_deep_research(args: Value) -> Result<ToolResult, String> {
    let query = args.get("query")
        .and_then(|v| v.as_str())
        .ok_or("Missing: query")?;
    let depth = args.get("depth").and_then(|v| v.as_u64()).unwrap_or(1).min(2) as u8;

    let handle = tokio::runtime::Handle::current();

    let mut sections: Vec<String> = Vec::new();
    sections.push(format!("# Research: {}\n", query));

    // Phase 1: Code search (fast - single HTTP call)
    sections.push("\n## 🔎 Code\n".to_string());
    let code = tokio::task::block_in_place(|| {
        handle.block_on(grep_search(query))
    }).unwrap_or_else(|e| format!("*Search error: {}*", e));
    sections.push(code);

    // Phase 2: Web pages (depth >= 2)
    if depth >= 2 {
        sections.push("\n## 🌐 Web\n".to_string());
        let web = tokio::task::block_in_place(|| {
            handle.block_on(fetch_web(query))
        }).unwrap_or_else(|e| format!("*Fetch error: {}*", e));
        sections.push(web);
    }

    // Phase 3: LLM synthesis
    sections.push("\n---\n## 🧠 Synthesis\n".to_string());
    let raw = sections.join("\n");
    let synth = tokio::task::block_in_place(|| {
        handle.block_on(llm_summarize(query, &raw))
    }).unwrap_or_else(|_| "*Synthesis unavailable*".to_string());
    sections.push(synth);

    Ok(ToolResult {
        content: vec![ToolContent::Text { text: sections.join("\n") }],
        is_error: Some(false),
    })
}

async fn grep_search(query: &str) -> Result<String, String> {
    let client = reqwest::Client::new();
    let init = json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"agents-rust","version":"0.1.0"}}});
    let _ = send_sse(&client, "https://mcp.grep.app", init).await?;
    let call = json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"searchGitHub","arguments":{"query":query,"matchCase":false}}});
    let resp = send_sse(&client, "https://mcp.grep.app", call).await?;
    extract_text(&resp)
}

async fn fetch_web(query: &str) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .user_agent("Mozilla/5.0")
        .build()
        .map_err(|_| "HTTP error".to_string())?;

    let encoded: String = query.chars().map(|c| match c {
        'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
        ' ' => "+".to_string(),
        _ => format!("%{:02X}", c as u8),
    }).collect();

    let body = client.get(format!("https://html.duckduckgo.com/html/?q={}", encoded))
        .send().await.map_err(|e| format!("DDG: {}", e))?
        .text().await.map_err(|e| format!("Read: {}", e))?;

    let urls: Vec<String> = body.split("href=\"")
        .filter_map(|p| { let u = p.split('"').next()?; if u.starts_with("http") && !u.contains("duckduckgo") { Some(u.to_string()) } else { None } })
        .take(2).collect();

    if urls.is_empty() { return Ok("*No pages*".to_string()); }
    let mut results = Vec::new();
    for url in &urls {
        if let Ok(resp) = client.get(url).send().await {
            if let Ok(text) = resp.text().await {
                let doc = scraper::Html::parse_document(&text);
                if let Some(body) = doc.select(&scraper::Selector::parse("body").unwrap()).next() {
                    let txt: Vec<String> = body.text().collect::<Vec<_>>().join(" ").lines()
                        .filter_map(|l| { let t = l.trim(); if t.is_empty() { None } else { Some(t.to_string()) } }).collect();
                    let preview: String = txt.join("\n").chars().take(1000).collect();
                    results.push(format!("**{}**\n{}", url, preview));
                }
            }
        }
    }
    Ok(if results.is_empty() { "*No content*".to_string() } else { results.join("\n\n---\n\n") })
}

async fn llm_summarize(query: &str, raw: &str) -> Result<String, String> {
    let cfg = crate::config::get();
    let client = reqwest::Client::new();
    let body = json!({
        "model": "deepseek-v4-flash-free",
        "messages": [
            {"role": "system", "content": "You synthesize research concisely. Use markdown."},
            {"role": "user", "content": format!("Query: {}\n---\n{}\n---\nSynthesize:", query, raw)}
        ],
        "stream": false
    });

    let resp = client.post(&cfg.llm.base_url)
        .header("Authorization", format!("Bearer {}", cfg.llm.api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("LLM: {}", e))?
        .json::<Value>()
        .await
        .map_err(|e| format!("Parse: {}", e))?;

    Ok(resp.pointer("/choices/0/message/content")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string())
}

async fn send_sse(client: &reqwest::Client, url: &str, body: Value) -> Result<Value, String> {
    let resp = client.post(url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("HTTP: {}", e))?;
    if !resp.status().is_success() { return Err(format!("HTTP {}", resp.status())); }
    let text = resp.text().await.map_err(|e| format!("Read: {}", e))?;
    text.lines().filter_map(|l| l.strip_prefix("data: ")).filter_map(|j| serde_json::from_str::<Value>(j).ok()).last().ok_or("No SSE data".to_string())
}

fn extract_text(resp: &Value) -> Result<String, String> {
    let texts: Vec<String> = resp.pointer("/result/content")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|i| i.get("text").and_then(|v| v.as_str()).map(String::from)).collect())
        .unwrap_or_default();
    Ok(if texts.is_empty() { "*Empty*".to_string() } else { texts.join("\n---\n") })
}
