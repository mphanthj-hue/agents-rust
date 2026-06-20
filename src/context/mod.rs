use crate::llm::LlmClient;
use crate::llm::types::ChatMessage;

pub struct ContextManager {
    max_tokens: usize,
    client: LlmClient,
}

impl ContextManager {
    pub fn new(max_tokens: usize) -> Self {
        Self {
            max_tokens,
            client: LlmClient::new(),
        }
    }

    pub async fn summarize(&self, text: &str, target_length: &str) -> Result<String, String> {
        let prompt = format!(
            r#"Tóm tắt nội dung sau đây, giữ lại thông tin quan trọng nhất.
Yêu cầu: tóm tắt {}.
Nội dung:
{}"#,
            target_length, text
        );

        let response = self.client.chat(
            vec![
                ChatMessage::system("Bạn là AI tóm tắt văn bản. Chỉ trả về tóm tắt, không thêm gì khác."),
                ChatMessage::user(&prompt),
            ]
        ).await?;

        Ok(response.choices.first()
            .and_then(|c| c.message.content.clone())
            .unwrap_or_default())
    }

    pub fn estimate_tokens(text: &str) -> usize {
        text.len() / 4 + 10
    }

    pub fn truncate_to_limit(&self, text: &str) -> String {
        let estimated = Self::estimate_tokens(text);
        if estimated <= self.max_tokens {
            return text.to_string();
        }
        let ratio = self.max_tokens as f64 / estimated as f64;
        let new_len = (text.len() as f64 * ratio * 0.9) as usize;
        let truncated: String = text.chars().take(new_len).collect();
        format!("{}... [cắt bớt từ {} tokens xuống còn {} tokens]", 
            truncated, estimated, self.max_tokens)
    }

    pub fn compress_messages(&self, messages: &[ChatMessage]) -> Vec<ChatMessage> {
        let mut compressed = Vec::new();
        for msg in messages {
            match msg.role.as_str() {
                "system" => {
                    compressed.push(msg.clone());
                }
                "user" | "assistant" => {
                    compressed.push(msg.clone());
                }
                "tool" => {
                    if let Some(ref content) = msg.content {
                        if Self::estimate_tokens(content) > self.max_tokens / 10 {
                            let summary = format!("[tool result: {} bytes]", content.len());
                            compressed.push(ChatMessage::tool(
                                msg.tool_call_id.as_deref().unwrap_or(""),
                                summary,
                            ));
                        } else {
                            compressed.push(msg.clone());
                        }
                    }
                }
                _ => {
                    compressed.push(msg.clone());
                }
            }
        }

        compressed
    }

    pub fn trim_conversation(&self, messages: &[ChatMessage], max_messages: usize) -> Vec<ChatMessage> {
        if messages.len() <= max_messages {
            return messages.to_vec();
        }

        let mut kept = Vec::new();
        for msg in messages {
            if msg.role == "system" {
                kept.push(msg.clone());
            }
        }

        let remaining = max_messages - kept.len();
        let non_system: Vec<_> = messages.iter()
            .filter(|m| m.role != "system")
            .collect();

        let start = non_system.len().saturating_sub(remaining);
        for msg in &non_system[start..] {
            kept.push((*msg).clone());
        }

        kept
    }
}
