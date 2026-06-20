use crate::llm::LlmClient;
use crate::llm::types::*;
use crate::tools;
use crate::orchestrator::types::*;
use serde_json::Value;
use std::time::Duration;
use tokio::time::timeout;

pub struct Worker {
    client: LlmClient,
    tools: Vec<ToolDefinition>,
    messages: Vec<ChatMessage>,
    config: WorkerConfig,
    name: String,
}

impl Worker {
    pub fn new(name: &str, instruction: &str, allowed_tools: &[String], config: WorkerConfig) -> Self {
        let all_tool_defs = tools::get_all_tool_definitions();
        
        let openai_tools: Vec<ToolDefinition> = all_tool_defs
            .iter()
            .filter(|t| allowed_tools.is_empty() || allowed_tools.contains(&t.name))
            .map(|t| {
                ToolDefinition {
                    type_: "function".into(),
                    function: ToolInfo {
                        name: t.name.clone(),
                        description: t.description.clone(),
                        parameters: t.input_schema.clone(),
                    },
                }
            })
            .collect();

        Self {
            client: LlmClient::new(),
            tools: openai_tools,
            messages: vec![ChatMessage::system(instruction)],
            config,
            name: name.to_string(),
        }
    }

    pub async fn run(&mut self, task: &str) -> SubTaskResult {
        self.messages.push(ChatMessage::user(task));
        
        let result = timeout(Duration::from_secs(self.config.timeout_secs), self.run_loop()).await;
        
        match result {
            Ok(Ok(output)) => SubTaskResult {
                id: self.name.clone(),
                output,
                error: None,
            },
            Ok(Err(e)) => SubTaskResult {
                id: self.name.clone(),
                output: String::new(),
                error: Some(e),
            },
            Err(_) => SubTaskResult {
                id: self.name.clone(),
                output: String::new(),
                error: Some(format!("Worker '{}' timed out after {}s", self.name, self.config.timeout_secs)),
            },
        }
    }

    async fn run_loop(&mut self) -> Result<String, String> {
        let max_iterations = self.config.max_iterations;
        let mut iteration = 0;

        loop {
            if iteration >= max_iterations {
                return Err("Worker reached maximum iterations".into());
            }
            iteration += 1;

            let response = self.client
                .chat_with_intelligent_fallback(self.messages.clone(), self.tools.clone(), None)
                .await?;

            let choice = response.choices.into_iter().next()
                .ok_or("Empty response from LLM")?;

            let has_tool_calls = choice.message.tool_calls
                .as_ref()
                .map(|c| !c.is_empty())
                .unwrap_or(false);

            if has_tool_calls {
                let tool_calls = choice.message.tool_calls.unwrap();
                self.messages.push(ChatMessage::assistant(None, Some(tool_calls.clone())));

                for tc in &tool_calls {
                    let result = self.execute_tool(&tc.function.name, &tc.function.arguments);
                    match result {
                        Ok(output) => {
                            self.messages.push(ChatMessage::tool(&tc.id, &output));
                        }
                        Err(e) => {
                            self.messages.push(ChatMessage::tool(&tc.id, format!("Error: {}", e)));
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

    fn execute_tool(&self, name: &str, arguments: &str) -> Result<String, String> {
        let args: Value = serde_json::from_str(arguments)
            .map_err(|e| format!("Failed to parse tool arguments: {}", e))?;

        let handler = tools::get_tool_handler(name)
            .ok_or_else(|| format!("Unknown tool: {}", name))?;

        let result = handler(args)?;

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
