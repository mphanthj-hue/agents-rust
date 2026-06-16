use std::collections::HashSet;
use std::sync::{Mutex, LazyLock};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub allowed_directories: Vec<String>,
    pub blocked_commands: HashSet<String>,
    pub default_shell: String,
    pub file_read_line_limit: usize,
    pub file_write_line_limit: usize,
    pub telemetry_enabled: bool,
    pub llm: LlmConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    pub base_url: String,
    pub model: String,
    pub api_key: String,
    pub max_tokens: u32,
    pub temperature: f32,
    pub top_p: f32,
    pub fallback_models: Vec<String>,
    pub vision_model: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            allowed_directories: vec!["/".to_string()],
            blocked_commands: HashSet::from([
                "sudo".into(), "su".into(), "passwd".into(),
                "shutdown".into(), "reboot".into(), "init".into(),
                "poweroff".into(), "halt".into(),
                "mkfs".into(), "dd".into(), "format".into(),
            ]),
            default_shell: "bash".into(),
            file_read_line_limit: 1000,
            file_write_line_limit: 50,
            telemetry_enabled: false,
            llm: LlmConfig {
                base_url: "https://opencode.ai/zen/v1".into(),
                model: "deepseek-v4-flash-free".into(),
                api_key: String::new(),
                max_tokens: 4096,
                temperature: 0.7,
                top_p: 0.95,
                fallback_models: vec![
                    "big-pickle".into(),
                    "nemotron-3-ultra-free".into(),
                    "mimo-v2.5-free".into(),
                    "north-mini-code-free".into(),
                ],
                vision_model: "mimo-v2.5-free".into(),
            },
        }
    }
}

static CONFIG: LazyLock<Mutex<AppConfig>> = LazyLock::new(|| {
    Mutex::new(AppConfig::default())
});

pub fn get() -> AppConfig {
    CONFIG.lock().unwrap().clone()
}

pub fn set(cfg: AppConfig) {
    *CONFIG.lock().unwrap() = cfg;
}

pub fn update<F>(f: F) where F: FnOnce(&mut AppConfig) {
    f(&mut *CONFIG.lock().unwrap());
}
