use reqwest::Client as HttpClient;
use crate::config;
use crate::llm::types::*;

pub struct LlmClient {
    client: HttpClient,
    base_url: String,
    api_key: String,
    model: String,
    fallback_models: Vec<String>,
    vision_model: String,
}

impl LlmClient {
    pub fn new() -> Self {
        let cfg = config::get();
        Self {
            client: HttpClient::new(),
            base_url: cfg.llm.base_url,
            api_key: cfg.llm.api_key,
            model: cfg.llm.model,
            fallback_models: cfg.llm.fallback_models.clone(),
            vision_model: cfg.llm.vision_model.clone(),
        }
    }

    pub fn with_config(base_url: &str, api_key: &str, model: &str) -> Self {
        let cfg = config::get();
        Self {
            client: HttpClient::new(),
            base_url: base_url.to_string(),
            api_key: api_key.to_string(),
            model: model.to_string(),
            fallback_models: cfg.llm.fallback_models.clone(),
            vision_model: cfg.llm.vision_model.clone(),
        }
    }

    pub fn model(&self) -> &str { &self.model }

    pub fn vision_model(&self) -> &str { &self.vision_model }

    pub fn has_vision(prompt: &str) -> bool {
        let lower = prompt.to_lowercase();
        let image_extensions = [".jpg", ".jpeg", ".png", ".gif", ".bmp", ".webp", ".svg"];
        let image_keywords = ["hình ảnh", "ảnh", "picture", "image", "photo", "screenshot"];

        for ext in &image_extensions {
            if lower.contains(ext) { return true; }
        }
        for kw in &image_keywords {
            if lower.contains(kw) { return true; }
        }

        // Check for base64 image data in prompt
        if lower.contains("base64") && (lower.contains("image/") || lower.contains("data:image")) {
            return true;
        }

        false
    }

    pub fn select_model(prompt: &str, preferred: Option<&str>) -> String {
        if let Some(m) = preferred {
            return m.to_string();
        }
        let cfg = config::get();
        if Self::has_vision(prompt) {
            cfg.llm.vision_model
        } else {
            cfg.llm.model
        }
    }

    pub fn set_model(&mut self, model: &str) {
        self.model = model.to_string();
    }

    pub fn build_request(&self, messages: Vec<ChatMessage>) -> ChatRequest {
        let cfg = config::get();
        ChatRequest {
            model: self.model.clone(),
            messages,
            temperature: Some(cfg.llm.temperature),
            max_tokens: Some(cfg.llm.max_tokens),
            top_p: Some(cfg.llm.top_p),
            stream: None,
            tools: None,
            tool_choice: None,
        }
    }

    pub fn build_request_with_tools(&self, messages: Vec<ChatMessage>, tools: Vec<ToolDefinition>) -> ChatRequest {
        let cfg = config::get();
        ChatRequest {
            model: self.model.clone(),
            messages,
            temperature: Some(cfg.llm.temperature),
            max_tokens: Some(cfg.llm.max_tokens),
            top_p: Some(cfg.llm.top_p),
            stream: None,
            tools: Some(tools),
            tool_choice: Some(serde_json::json!("auto")),
        }
    }

    async fn send_request(&self, request: &ChatRequest) -> Result<ChatResponse, String> {
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));

        let response = self.client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(request)
            .send()
            .await
            .map_err(|e| format!("Request failed: {}", e))?;

        let status = response.status();
        let body = response.text().await
            .map_err(|e| format!("Failed to read response body: {}", e))?;

        if !status.is_success() {
            if let Ok(api_err) = serde_json::from_str::<ApiError>(&body) {
                return Err(format!("API error ({}): {}", status.as_u16(), api_err.error.message));
            }
            return Err(format!("HTTP {}: {}", status.as_u16(), body));
        }

        serde_json::from_str::<ChatResponse>(&body)
            .map_err(|e| format!("Failed to parse response: {} - body: {}", e, &body[..body.len().min(200)]))
    }

    pub async fn chat(&self, messages: Vec<ChatMessage>) -> Result<ChatResponse, String> {
        self.send_request(&self.build_request(messages)).await
    }

    pub async fn chat_with_tools(&self, messages: Vec<ChatMessage>, tools: Vec<ToolDefinition>) -> Result<ChatResponse, String> {
        self.send_request(&self.build_request_with_tools(messages, tools)).await
    }

    pub async fn chat_with_fallback(&self, messages: Vec<ChatMessage>, tools: Vec<ToolDefinition>) -> Result<ChatResponse, String> {
        let mut last_error = String::new();

        // Try primary model
        match self.send_request(&self.build_request_with_tools(messages.clone(), tools.clone())).await {
            Ok(r) => return Ok(r),
            Err(e) => last_error = e,
        }

        // Try fallback models
        for fb in &self.fallback_models {
            let req = ChatRequest {
                model: fb.clone(),
                messages: messages.clone(),
                temperature: self.build_request_with_tools(messages.clone(), tools.clone()).temperature,
                max_tokens: self.build_request_with_tools(messages.clone(), tools.clone()).max_tokens,
                top_p: self.build_request_with_tools(messages.clone(), tools.clone()).top_p,
                stream: None,
                tools: Some(tools.clone()),
                tool_choice: Some(serde_json::json!("auto")),
            };

            match self.send_request(&req).await {
                Ok(r) => {
                    // Update primary model to the working one for subsequent calls
                    // (We can't mutate self here, but caller can check)
                    return Ok(r);
                }
                Err(e) => {
                    last_error = format!("{} | {} failed: {}", last_error, fb, e);
                }
            }
        }

        Err(format!("All models failed: {}", last_error))
    }

    pub async fn chat_stream(
        &self,
        messages: Vec<ChatMessage>,
        mut on_chunk: impl FnMut(String) -> Result<(), String>,
    ) -> Result<String, String> {
        let mut request = self.build_request(messages);
        request.stream = Some(true);

        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));

        let response = self.client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .header("Accept", "text/event-stream")
            .json(&request)
            .send()
            .await
            .map_err(|e| format!("Stream request failed: {}", e))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            if let Ok(api_err) = serde_json::from_str::<ApiError>(&body) {
                return Err(format!("API error ({}): {}", status.as_u16(), api_err.error.message));
            }
            return Err(format!("HTTP {}: {}", status.as_u16(), body));
        }

        let mut full_content = String::new();
        let mut stream = response.bytes_stream();
        use futures_util::StreamExt;
        let mut buffer = String::new();

        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result.map_err(|e| format!("Stream read error: {}", e))?;
            let text = String::from_utf8_lossy(&chunk);
            buffer.push_str(&text);

            loop {
                let Some(line_end) = buffer.find('\n') else { break };
                let line = buffer[..line_end].trim().to_string();
                buffer = buffer[line_end + 1..].to_string();

                if line.is_empty() { continue; }
                if !line.starts_with("data: ") { continue; }
                let data = &line[6..];
                if data == "[DONE]" { break; }

                if let Ok(chunk_data) = serde_json::from_str::<StreamChunk>(data) {
                    for choice in &chunk_data.choices {
                        if let Some(ref content) = choice.delta.content {
                            full_content.push_str(content);
                            on_chunk(content.clone())?;
                        }
                    }
                }
            }
        }

        Ok(full_content)
    }
}
