use crate::llm::LlmClient;
use crate::llm::types::*;
use crate::tools;
use crate::config;
use serde_json::Value;

#[allow(dead_code)]
pub struct Agent {
    client: LlmClient,
    tools: Vec<ToolDefinition>,
    messages: Vec<ChatMessage>,
    used_fallback: bool,
    active_model: String,
    max_messages: usize,
}

impl Default for Agent {
    fn default() -> Self {
        Self::new()
    }
}

impl Agent {
    pub fn new() -> Self {
        let cfg = config::get();
        let tool_defs = tools::get_all_tool_definitions();

        let openai_tools: Vec<ToolDefinition> = tool_defs.iter().map(|t| {
            ToolDefinition {
                type_: "function".into(),
                function: ToolInfo {
                    name: t.name.clone(),
                    description: t.description.clone(),
                    parameters: t.input_schema.clone(),
                },
            }
        }).collect();

        let initial_model = cfg.llm.model.clone();

        Self {
            client: if cfg.llm.api_key.is_empty() {
                LlmClient::new()
            } else {
                LlmClient::with_config(&cfg.llm.base_url, &cfg.llm.api_key, &initial_model)
            },
            tools: openai_tools,
            messages: Vec::new(),
            used_fallback: false,
            active_model: initial_model,
            max_messages: 20,
        }
    }

    pub fn add_system_prompt(&mut self, prompt: &str) {
        self.messages.push(ChatMessage::system(prompt));
        self.trim_messages();
    }

    pub fn add_user_message(&mut self, text: &str) {
        // Auto-detect vision: if prompt mentions images, switch to vision model
        if LlmClient::has_vision(text) && !self.active_model.contains("mimo") {
            let vision = self.client.vision_model().to_string();
            self.client.set_model(&vision);
            self.active_model = vision;
        }
        self.messages.push(ChatMessage::user(text));
        self.trim_messages();
    }

    fn trim_messages(&mut self) {
        if self.messages.len() > self.max_messages {
            let excess = self.messages.len() - self.max_messages;
            self.messages.drain(0..excess);
        }
    }

    #[allow(dead_code)]
    pub fn active_model(&self) -> &str { &self.active_model }
    #[allow(dead_code)]
    pub fn used_fallback(&self) -> bool { self.used_fallback }

    pub async fn run(&mut self) -> Result<String, String> {
        let max_iterations = 20;
        let mut iteration = 0;
        let primary_model = config::get().llm.model;

        loop {
            if iteration >= max_iterations {
                return Err("Agent reached maximum iterations".into());
            }
            iteration += 1;

            let response = self.client
                .chat_with_intelligent_fallback(self.messages.clone(), self.tools.clone(), None)
                .await?;

            // Check if fallback was used (model in response differs from primary)
            if let Some(ref resp_model) = response.model {
                if resp_model != &primary_model {
                    self.used_fallback = true;
                    self.active_model = resp_model.clone();
                    self.client.set_model(resp_model);
                }
            }

            let choice = response.choices.into_iter().next()
                .ok_or("Empty response from LLM")?;

            let has_tool_calls = choice.message.tool_calls
                .as_ref()
                .map(|c| !c.is_empty())
                .unwrap_or(false);

            if has_tool_calls {
                let tool_calls = choice.message.tool_calls.unwrap();
                self.messages.push(ChatMessage::assistant(None, Some(tool_calls.clone())));
                self.trim_messages();

                for tc in &tool_calls {
                    let result = self.execute_tool(&tc.function.name, &tc.function.arguments).await;
                    match result {
                        Ok(output) => {
                            self.messages.push(ChatMessage::tool(&tc.id, &output));
                            self.trim_messages();
                        }
                        Err(e) => {
                            self.messages.push(ChatMessage::tool(&tc.id, format!("Error: {}", e)));
                            self.trim_messages();
                        }
                    }
                }
            } else if let Some(content) = choice.message.content {
                return Ok(content);
            } else {
                return Err("LLM returned empty response with no tool calls".into());
            }
        }
    }

    async fn execute_tool(&self, name: &str, arguments: &str) -> Result<String, String> {
        let args: Value = serde_json::from_str(arguments)
            .map_err(|e| format!("Failed to parse tool arguments: {}", e))?;

        let result = if let Some(handler) = tools::get_tool_handler(name) {
            // Built-in tool: fn pointer là 'static + Send, spawn_blocking an toàn
            tokio::task::spawn_blocking(move || handler(args))
                .await
                .map_err(|e| format!("Tool execution panicked: {}", e))??
        } else if let Some(plugin_result) = crate::plugin::execute_tool(name, args) {
            // Plugin tool: dispatch trực tiếp qua Mutex, không cần spawn_blocking
            plugin_result?
        } else {
            return Err(format!("Unknown tool: {}", name));
        };

        let text = result.content.into_iter()
            .map(|c| match c {
                crate::mcp::types::ToolContent::Text { text } => text,
                crate::mcp::types::ToolContent::Resource { resource } => resource.text,
            })
            .collect::<Vec<_>>()
            .join("\n");

        Ok(text)
    }
}
