use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::config;
use crate::llm::types::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmRouterConfig {
    pub circuit_breaker: Option<CircuitBreakerConfig>,
    pub models: Vec<ModelConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerConfig {
    pub failure_threshold: u32,
    pub recovery_timeout_secs: u64,
    pub min_requests: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub name: String,
    pub provider: String,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub fallbacks: Option<Vec<String>>,
    pub rate_limit_fallbacks: Option<Vec<String>>,
    pub context_window_fallbacks: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq)]
enum CircuitState {
    Closed,
    Open(Instant),
}

struct CircuitBreaker {
    state: CircuitState,
    failures: AtomicU32,
    threshold: u32,
    recovery_timeout: Duration,
}

impl CircuitBreaker {
    fn new(config: &CircuitBreakerConfig) -> Self {
        Self {
            state: CircuitState::Closed,
            failures: AtomicU32::new(0),
            threshold: config.failure_threshold,
            recovery_timeout: Duration::from_secs(config.recovery_timeout_secs),
        }
    }

    fn is_allowed(&self) -> bool {
        match self.state {
            CircuitState::Closed => true,
            CircuitState::Open(opened_at) => opened_at.elapsed() >= self.recovery_timeout,
        }
    }

    fn record_success(&mut self) {
        self.failures.store(0, Ordering::SeqCst);
        self.state = CircuitState::Closed;
    }

    fn record_failure(&mut self) {
        let fails = self.failures.fetch_add(1, Ordering::SeqCst) + 1;
        if fails >= self.threshold && self.state == CircuitState::Closed {
            self.state = CircuitState::Open(Instant::now());
        }
    }
}

pub struct LlmRouter {
    model_map: HashMap<String, ModelConfig>,
    circuits: Arc<RwLock<HashMap<String, CircuitBreaker>>>,
    http_client: reqwest::Client,
}

fn build_chain(model_config: &ModelConfig) -> Vec<String> {
    let mut chain = Vec::new();
    chain.push(model_config.name.clone());

    for fb in model_config.fallbacks.iter().flatten() {
        if !chain.contains(fb) {
            chain.push(fb.clone());
        }
    }
    for fb in model_config.rate_limit_fallbacks.iter().flatten() {
        if !chain.contains(fb) {
            chain.push(fb.clone());
        }
    }
    for fb in model_config.context_window_fallbacks.iter().flatten() {
        if !chain.contains(fb) {
            chain.push(fb.clone());
        }
    }

    chain
}

impl LlmRouter {
    pub fn new(config_path: &str) -> Result<Self, String> {
        let content = std::fs::read_to_string(config_path)
            .map_err(|e| format!("Không đọc được config {}: {}", config_path, e))?;
        let router_config: LlmRouterConfig = serde_yaml::from_str(&content)
            .map_err(|e| format!("Không parse được YAML: {}", e))?;

        let cb_config = router_config.circuit_breaker.unwrap_or(CircuitBreakerConfig {
            failure_threshold: 3,
            recovery_timeout_secs: 30,
            min_requests: 1,
        });

        let model_map: HashMap<String, ModelConfig> = router_config.models
            .into_iter()
            .map(|m| (m.name.clone(), m))
            .collect();

        let circuits = model_map.keys()
            .map(|name| (name.clone(), CircuitBreaker::new(&cb_config)))
            .collect();

        Ok(Self {
            model_map,
            circuits: Arc::new(RwLock::new(circuits)),
            http_client: reqwest::Client::builder()
                .timeout(Duration::from_secs(120))
                .build()
                .map_err(|e| format!("HTTP client: {}", e))?,
        })
    }

    pub fn from_default() -> Result<Self, String> {
        let paths = [
            "config/llm.yaml",
            "/home/mrken/.config/agents-rust/llm.yaml",
            "/etc/agents-rust/llm.yaml",
        ];
        for path in &paths {
            if std::path::Path::new(path).exists() {
                return Self::new(path);
            }
        }
        Err("Không tìm thấy config/llm.yaml.".into())
    }

    pub async fn chat_with_fallback(
        &self,
        messages: Vec<ChatMessage>,
        tools: Vec<ToolDefinition>,
        preferred_model: Option<&str>,
    ) -> Result<ChatResponse, String> {
        let model_name = preferred_model
            .map(String::from)
            .unwrap_or_else(|| config::get().llm.model);

        let model_config = self.model_map.get(&model_name);
        let model_config = match model_config {
            Some(m) => m,
            None => return self.chat_direct(&model_name, messages, tools).await,
        };

        let chain = build_chain(model_config);
        let mut errors: Vec<String> = Vec::new();

        for (idx, name) in chain.iter().enumerate() {
            {
                let cb = self.circuits.read().await;
                if let Some(c) = cb.get(name) {
                    if !c.is_allowed() {
                        errors.push(format!("{}: circuit breaker đang mở", name));
                        continue;
                    }
                }
            }

            match self.chat_direct(name, messages.clone(), tools.clone()).await {
                Ok(resp) => {
                    if let Some(c) = self.circuits.write().await
                        .get_mut(name) { c.record_success() }
                    return Ok(resp);
                }
                Err(e) => {
                    if let Some(c) = self.circuits.write().await
                        .get_mut(name) { c.record_failure() }
                    let label = if idx == 0 { "primary" } else { name };
                    errors.push(format!("{}: {}", label, e));
                }
            }
        }

        Err(format!("Tất cả models đều lỗi: {}", errors.join(" | ")))
    }

    pub async fn chat_direct(
        &self,
        model: &str,
        messages: Vec<ChatMessage>,
        tools: Vec<ToolDefinition>,
    ) -> Result<ChatResponse, String> {
        let cfg = config::get();

        let model_config = self.model_map.get(model);
        let base_url = model_config
            .and_then(|m| m.base_url.as_deref())
            .filter(|s| !s.is_empty())
            .unwrap_or(&cfg.llm.base_url);

        let api_key = model_config
            .and_then(|m| m.api_key.as_deref())
            .filter(|s| !s.is_empty())
            .unwrap_or(&cfg.llm.api_key);

        let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
        let has_tools = !tools.is_empty();

        let request = ChatRequest {
            model: model.to_string(),
            messages,
            temperature: Some(cfg.llm.temperature),
            max_tokens: Some(cfg.llm.max_tokens),
            top_p: Some(cfg.llm.top_p),
            stream: None,
            tools: if has_tools { Some(tools) } else { None },
            tool_choice: if has_tools { Some(serde_json::json!("auto")) } else { None },
        };

        let mut req_builder = self.http_client
            .post(&url)
            .header("Content-Type", "application/json");

        if !api_key.is_empty() {
            req_builder = req_builder.header("Authorization", format!("Bearer {}", api_key));
        }

        let response = req_builder
            .json(&request)
            .send()
            .await
            .map_err(|e| format!("Request failed: {}", e))?;

        let status = response.status();
        let body = response.text().await
            .map_err(|e| format!("Failed to read response: {}", e))?;

        if !status.is_success() {
            if let Ok(api_err) = serde_json::from_str::<ApiError>(&body) {
                return Err(format!("API error ({}): {}", status.as_u16(), api_err.error.message));
            }
            return Err(format!("HTTP {}: {}", status.as_u16(), body));
        }

        serde_json::from_str::<ChatResponse>(&body)
            .map_err(|e| format!("Parse response failed: {} - body: {}", e, &body[..body.len().min(200)]))
    }

    #[allow(dead_code)]
    pub fn list_models(&self) -> Vec<String> {
        self.model_map.keys().cloned().collect()
    }

    #[allow(dead_code)]
    pub fn get_model_config(&self, name: &str) -> Option<&ModelConfig> {
        self.model_map.get(name)
    }
}
