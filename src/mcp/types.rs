use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcMessage {
    #[serde(default)]
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub params: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl JsonRpcMessage {
    pub fn result(id: Value, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id: Some(id),
            method: None,
            params: None,
            result: Some(result),
            error: None,
        }
    }

    pub fn error(id: Value, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id: Some(id),
            method: None,
            params: None,
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
                data: None,
            }),
        }
    }

    pub fn notification(method: impl Into<String>, params: Value) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id: None,
            method: Some(method.into()),
            params: Some(params),
            result: None,
            error: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpCapabilities {
    pub tools: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitializeResult {
    pub protocol_version: String,
    pub capabilities: McpCapabilities,
    pub server_info: ServerInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerInfo {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub content: Vec<ToolContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ToolContent {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "resource")]
    Resource {
        resource: ResourceContent,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceContent {
    pub uri: String,
    pub mime_type: String,
    pub text: String,
}
