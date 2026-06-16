use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl ChatMessage {
    pub fn user(content: impl Into<String>) -> Self {
        Self { role: "user".into(), content: Some(content.into()), tool_calls: None, tool_call_id: None, name: None }
    }
    pub fn system(content: impl Into<String>) -> Self {
        Self { role: "system".into(), content: Some(content.into()), tool_calls: None, tool_call_id: None, name: None }
    }
    pub fn assistant(content: Option<String>, tool_calls: Option<Vec<ToolCall>>) -> Self {
        Self { role: "assistant".into(), content, tool_calls, tool_call_id: None, name: None }
    }
    pub fn tool(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self { role: "tool".into(), content: Some(content.into()), tool_calls: None, tool_call_id: Some(tool_call_id.into()), name: None }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub type_: String,
    pub function: ToolFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolFunction {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolDefinition {
    #[serde(rename = "type")]
    pub type_: String,
    pub function: ToolInfo,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ToolDefinition>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChatResponse {
    pub id: Option<String>,
    pub object: Option<String>,
    pub created: Option<u64>,
    pub model: Option<String>,
    pub choices: Vec<Choice>,
    pub usage: Option<Usage>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Choice {
    pub index: u32,
    pub message: ChatMessage,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StreamChunk {
    pub id: Option<String>,
    pub object: Option<String>,
    pub choices: Vec<StreamChoice>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StreamChoice {
    pub index: u32,
    pub delta: Delta,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Delta {
    pub content: Option<String>,
    pub role: Option<String>,
    #[serde(default)]
    pub tool_calls: Option<Vec<ToolCall>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiError {
    pub error: ApiErrorDetail,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiErrorDetail {
    pub message: String,
    #[serde(rename = "type")]
    pub error_type: Option<String>,
    pub code: Option<Value>,
}
