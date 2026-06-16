use std::sync::Arc;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tracing_subscriber;

mod config;
mod mcp;
mod tools;
mod security;
mod terminal;
mod llm;

use mcp::types::*;
use tools::ToolHandler;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .with_target(false)
        .init();

    // Config is auto-initialized via LazyLock

    // Register tools
    let tools_list = Arc::new(tools::get_all_tool_definitions());
    let mut handlers: Vec<(String, ToolHandler)> = Vec::new();
    for tool in tools_list.iter() {
        if let Some(handler) = tools::get_tool_handler(&tool.name) {
            handlers.push((tool.name.clone(), handler));
        }
    }
    let handlers = Arc::new(handlers);

    let stdin = tokio::io::stdin();
    let reader = BufReader::new(stdin);
    let mut lines = reader.lines();
    let mut stdout = tokio::io::stdout();

    // MCP init state
    let mut initialized = false;

    while let Ok(Some(line)) = lines.next_line().await {
        if line.trim().is_empty() { continue; }

        let msg: JsonRpcMessage = match serde_json::from_str(&line) {
            Ok(m) => m,
            Err(e) => {
                let err = JsonRpcMessage {
                    jsonrpc: "2.0".into(),
                    id: Some(Value::Null),
                    method: None,
                    params: None,
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32700,
                        message: format!("Parse error: {}", e),
                        data: None,
                    }),
                };
                send(&mut stdout, &err).await;
                continue;
            }
        };

        let method = match msg.method.as_deref() {
            Some(m) => m,
            None => continue,
        };

        match method {
            "initialize" => {
                let result = serde_json::json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": {
                        "tools": {}
                    },
                    "serverInfo": {
                        "name": "agents-rust",
                        "version": "0.1.0"
                    }
                });
                if let Some(id) = msg.id {
                    send(&mut stdout, &JsonRpcMessage::result(id, result)).await;
                }
            }
            "notifications/initialized" => {
                initialized = true;
            }
            "notifications/exit" => {
                break;
            }
            "tools/list" => {
                let result = serde_json::json!({ "tools": *tools_list });
                if let Some(id) = msg.id {
                    send(&mut stdout, &JsonRpcMessage::result(id, result)).await;
                }
            }
            "tools/call" => {
                if let Some(id) = msg.id {
                    let params = msg.params.unwrap_or(Value::Null);
                    let tool_name = params.get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let arguments = params.get("arguments")
                        .cloned()
                        .unwrap_or(Value::Null);

                    let resp = if let Some((_, handler)) = handlers.iter().find(|(n, _)| n == tool_name) {
                        match handler(arguments) {
                            Ok(result) => {
                                JsonRpcMessage::result(id, serde_json::to_value(result).unwrap())
                            }
                            Err(e) => {
                                let err_result = ToolResult {
                                    content: vec![ToolContent::Text { text: e }],
                                    is_error: Some(true),
                                };
                                JsonRpcMessage::result(id, serde_json::to_value(err_result).unwrap())
                            }
                        }
                    } else {
                        let err_result = ToolResult {
                            content: vec![ToolContent::Text {
                                text: format!("Tool not found: {}", tool_name)
                            }],
                            is_error: Some(true),
                        };
                        JsonRpcMessage::result(id, serde_json::to_value(err_result).unwrap())
                    };
                    send(&mut stdout, &resp).await;
                }
            }
            _ => {
                if let Some(id) = msg.id {
                    send(&mut stdout, &JsonRpcMessage::error(id, -32601, format!("Method not found: {}", method))).await;
                }
            }
        }
    }
}

async fn send(stdout: &mut tokio::io::Stdout, msg: &JsonRpcMessage) {
    let json = serde_json::to_string(msg).unwrap_or_default();
    let mut line = json;
    line.push('\n');
    let _ = stdout.write_all(line.as_bytes()).await;
    let _ = stdout.flush().await;
}
