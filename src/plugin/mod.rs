use std::sync::{Mutex, LazyLock};
use serde_json::Value;
use crate::mcp::types::{ToolDefinition, ToolResult};

mod runtime;
use runtime::WasmPlugin;

static PLUGIN_MANAGER: LazyLock<Mutex<PluginManager>> = LazyLock::new(|| {
    Mutex::new(PluginManager::new())
});

pub struct PluginManager {
    plugins: Vec<WasmPlugin>,
}

impl PluginManager {
    fn new() -> Self {
        Self { plugins: Vec::new() }
    }

    fn register(&mut self, plugin: WasmPlugin) {
        self.plugins.push(plugin);
    }

    fn find_tool(&self, name: &str) -> Option<(usize, &WasmPlugin, &ToolDefinition)> {
        for (idx, plugin) in self.plugins.iter().enumerate() {
            for tool in &plugin.tools {
                if tool.name == name {
                    return Some((idx, plugin, tool));
                }
            }
        }
        None
    }

    fn execute(&self, plugin_idx: usize, tool_name: &str, args: Value) -> Result<ToolResult, String> {
        let plugin = &self.plugins[plugin_idx];
        plugin.execute(tool_name, args)
    }
}

fn default_plugin_dir() -> std::path::PathBuf {
    let base = std::env::var("AGENTS_RUST_PLUGINS_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME")
                .or_else(|_| std::env::var("USERPROFILE"))
                .unwrap_or_else(|_| ".".into());
            std::path::PathBuf::from(home).join(".config").join("agents-rust").join("plugins")
        });
    base
}

pub fn init() -> Result<(), String> {
    let dir = default_plugin_dir();
    if !dir.exists() {
        let _ = std::fs::create_dir_all(&dir);
        return Ok(());
    }

    let mut mgr = PLUGIN_MANAGER.lock().map_err(|e| e.to_string())?;

    let entries = std::fs::read_dir(&dir).map_err(|e| format!("Cannot read plugins dir: {}", e))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("Cannot read entry: {}", e))?;
        let path = entry.path();
        if path.extension().map_or(false, |ext| ext == "wasm") {
            match WasmPlugin::load(&path) {
                Ok(plugin) => {
                    let _tool_count = plugin.tools.len();
                    mgr.register(plugin);
                }
                Err(e) => {
                    eprintln!("[plugin] Failed to load {}: {}", path.display(), e);
                }
            }
        }
    }

    Ok(())
}

pub fn get_tool_definitions() -> Vec<ToolDefinition> {
    match PLUGIN_MANAGER.lock() {
        Ok(mgr) => {
            mgr.plugins.iter().flat_map(|p| p.tools.clone()).collect()
        }
        Err(_) => Vec::new(),
    }
}

pub fn execute_tool(name: &str, args: Value) -> Option<Result<ToolResult, String>> {
    let mgr = PLUGIN_MANAGER.lock().ok()?;
    let (plugin_idx, _plugin, _tool) = mgr.find_tool(name)?;
    Some(mgr.execute(plugin_idx, name, args))
}
