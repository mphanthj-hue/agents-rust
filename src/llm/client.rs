use std::sync::Arc;
use std::time::Duration;
use reqwest::Client as HttpClient;
use crate::config;
use crate::llm::types::*;
use crate::llm::router::LlmRouter;

pub struct LlmClient {
    client: HttpClient,
    base_url: String,
    api_key: String,
    model: String,
    #[allow(dead_code)]
    fallback_models: Vec<String>,
    #[allow(dead_code)]
    vision_model: String,
    router: Option<Arc<LlmRouter>>,
}

impl Default for LlmClient {
    fn default() -> Self {
        Self::new()
    }
}

impl LlmClient {
    pub fn new() -> Self {
        let cfg = config::get();
        let router = LlmRouter::from_default().ok().map(Arc::new);
        Self {
            client: HttpClient::builder()
                .timeout(Duration::from_secs(60))
                .build()
                .unwrap_or_else(|e| { eprintln!("[llm] HTTP client error: {}", e); HttpClient::new() }),
            base_url: cfg.llm.base_url,
            api_key: cfg.llm.api_key,
            model: cfg.llm.model,
            fallback_models: cfg.llm.fallback_models.clone(),
            vision_model: cfg.llm.vision_model.clone(),
            router,
        }
    }

    pub fn with_config(base_url: &str, api_key: &str, model: &str) -> Self {
        let cfg = config::get();
        let router = LlmRouter::from_default().ok().map(Arc::new);
        Self {
            client: HttpClient::new(),
            base_url: base_url.to_string(),
            api_key: api_key.to_string(),
            model: model.to_string(),
            fallback_models: cfg.llm.fallback_models.clone(),
            vision_model: cfg.llm.vision_model.clone(),
            router,
        }
    }

    #[allow(dead_code)]
    pub fn model(&self) -> &str { &self.model }

    #[allow(dead_code)]
    pub fn vision_model(&self) -> &str { &self.vision_model }

    #[allow(dead_code)]
    pub fn has_vision(prompt: &str) -> bool {
        let lower = prompt.to_lowercase();
        
        if lower.contains("data:image/") {
            return true;
        }
        
        if lower.contains("base64") && lower.contains("image/") {
            return true;
        }
        
        let image_extensions = [".jpg", ".jpeg", ".png", ".gif", ".bmp", ".webp", ".svg"];
        for ext in &image_extensions {
            if lower.contains(&format!("{}", ext)) {
                if let Some(pos) = lower.find(ext) {
                    let after = &lower[pos..];
                    if after.chars().next().map(|c| c.is_alphanumeric() || c == '.' || c == '?' || c == '#').unwrap_or(false) {
                        return true;
                    }
                }
            }
        }
        
        false
    }

    #[allow(dead_code)]
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

    #[allow(dead_code)]
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

    #[allow(dead_code)]
    pub async fn chat_with_fallback(&self, messages: Vec<ChatMessage>, tools: Vec<ToolDefinition>) -> Result<ChatResponse, String> {
        let mut errors: Vec<String> = Vec::new();

        let primary_req = self.build_request_with_tools(messages.clone(), tools.clone());
        match self.send_request(&primary_req).await {
            Ok(r) => return Ok(r),
            Err(e) => errors.push(format!("primary: {}", e)),
        }

        let base = self.build_request_with_tools(messages.clone(), tools.clone());
        for fb in &self.fallback_models {
            let req = ChatRequest {
                model: fb.clone(),
                messages: messages.clone(),
                temperature: base.temperature,
                max_tokens: base.max_tokens,
                top_p: base.top_p,
                stream: None,
                tools: Some(tools.clone()),
                tool_choice: Some(serde_json::json!("auto")),
            };

            match self.send_request(&req).await {
                Ok(r) => return Ok(r),
                Err(e) => errors.push(format!("{} failed: {}", fb, e)),
            }
        }

        Err(format!("All models failed: {}", errors.join(" | ")))
    }

    pub async fn chat_with_intelligent_fallback(
        &self,
        messages: Vec<ChatMessage>,
        tools: Vec<ToolDefinition>,
        preferred_model: Option<&str>,
    ) -> Result<ChatResponse, String> {
        match &self.router {
            Some(router) => {
                router.chat_with_fallback(messages, tools, preferred_model).await
            }
            None => {
                if tools.is_empty() {
                    self.chat(messages).await
                } else {
                    self.chat_with_tools(messages, tools).await
                }
            }
        }
    }

    pub fn with_router(mut self, router: Arc<LlmRouter>) -> Self {
        self.router = Some(router);
        self
    }

    #[allow(dead_code)]
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

            while let Some(line_end) = buffer.find('\n') {
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
