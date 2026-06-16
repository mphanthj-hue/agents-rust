use std::collections::HashMap;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::Mutex;
use serde_json::Value;
use crate::mcp::types::*;
use crate::tools;

pub type ToolHandler = fn(Value) -> Result<ToolResult, String>;

pub struct McpServer {
    handlers: Mutex<HashMap<String, ToolHandler>>,
}

impl McpServer {
    pub fn new() -> Self {
        Self {
            handlers: Mutex::new(HashMap::new()),
        }
    }

    pub fn register_tool(&self, name: &str, handler: ToolHandler) {
        let mut handlers = self.handlers.blocking_lock();
        handlers.insert(name.to_string(), handler);
    }

    fn get_tools_list() -> Vec<ToolDefinition> {
        tools::get_all_tool_definitions()
    }

    pub async fn run(&self) {
        let stdin = tokio::io::stdin();
        let reader = BufReader::new(stdin);
        let mut lines = reader.lines();
        let mut stdout = tokio::io::stdout();
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
                    self.send_response(&mut stdout, &err).await;
                    continue;
                }
            };

            let method = match msg.method.as_deref() {
                Some(m) => m,
                None => continue,
            };

            match method {
                "initialize" => {
                    let result = InitializeResult {
                        protocol_version: "2024-11-05".into(),
                        capabilities: McpCapabilities {
                            tools: Value::Object(serde_json::Map::new()),
                        },
                        server_info: ServerInfo {
                            name: "agents-rust".into(),
                            version: "0.1.0".into(),
                        },
                    };
                    if let Some(id) = msg.id {
                        let resp = JsonRpcMessage::result(id, serde_json::to_value(result).unwrap());
                        self.send_response(&mut stdout, &resp).await;
                    }
                }
                "notifications/initialized" => {
                    initialized = true;
                }
                "notifications/exit" => {
                    break;
                }
                "tools/list" => {
                    if !initialized && msg.method.as_deref() != Some("initialize") {
                        // Allow list before initialized per spec
                    }
                    let tools = Self::get_tools_list();
                    let result = serde_json::json!({ "tools": tools });
                    if let Some(id) = msg.id {
                        let resp = JsonRpcMessage::result(id, result);
                        self.send_response(&mut stdout, &resp).await;
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

                        let handlers = self.handlers.lock().await;
                        let resp = if let Some(handler) = handlers.get(tool_name) {
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
                            JsonRpcMessage::error(id, -32601, format!("Tool not found: {}", tool_name))
                        };
                        self.send_response(&mut stdout, &resp).await;
                    }
                }
                _ => {
                    if let Some(id) = msg.id {
                        let resp = JsonRpcMessage::error(id, -32601, format!("Method not found: {}", method));
                        self.send_response(&mut stdout, &resp).await;
                    }
                }
            }
        }
    }

    async fn send_response(&self, stdout: &mut tokio::io::Stdout, msg: &JsonRpcMessage) {
        let json = serde_json::to_string(msg).unwrap_or_default();
        let mut line = json;
        line.push('\n');
        let _ = stdout.write_all(line.as_bytes()).await;
        let _ = stdout.flush().await;
    }
}
