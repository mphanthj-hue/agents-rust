use crate::mcp::types::{ToolDefinition, ToolResult, ToolContent};
use crate::llm::LlmClient;
use crate::llm::types::ChatMessage;
use serde_json::{json, Value};

pub fn ask_llm_definition() -> ToolDefinition {
    ToolDefinition {
        name: "ask_llm".into(),
        description: "Send a prompt to the LLM (OpenCode Zen) and get a response. Use for code generation, analysis, explanations, and general knowledge.".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "prompt": { "type": "string", "description": "The prompt/question to send to the LLM" },
                "system": { "type": "string", "description": "Optional system prompt to set context/behavior", "default": "" },
                "model": { "type": "string", "description": "Model override: deepseek-v4-flash-free, nemotron-3-super-free, big-pickle", "default": "" }
            },
            "required": ["prompt"]
        }),
    }
}

pub fn handle_ask_llm(args: Value) -> Result<ToolResult, String> {
    let prompt = args.get("prompt")
        .and_then(|v| v.as_str())
        .ok_or("Missing: prompt")?;
    let system = args.get("system").and_then(|v| v.as_str()).unwrap_or("");
    let model = args.get("model").and_then(|v| v.as_str()).unwrap_or("");

    let mut messages = Vec::new();
    if !system.is_empty() {
        messages.push(ChatMessage::system(system));
    }
    messages.push(ChatMessage::user(prompt));

    let client = if model.is_empty() {
        LlmClient::new()
    } else {
        let cfg = crate::config::get();
        LlmClient::with_config(&cfg.llm.base_url, &cfg.llm.api_key, model)
    };

    let response = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current()
            .block_on(client.chat(messages))
    }).map_err(|e| format!("LLM request failed: {}", e))?;

    let text = response.choices
        .into_iter()
        .next()
        .and_then(|c| c.message.content)
        .unwrap_or_default();

    Ok(ToolResult {
        content: vec![ToolContent::Text { text }],
        is_error: None,
    })
}
