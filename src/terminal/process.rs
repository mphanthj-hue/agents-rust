use std::process::{Child, Command, Stdio};
use std::sync::{Mutex, LazyLock};
use std::collections::HashMap;

static MANAGER: LazyLock<Mutex<ProcessManager>> = LazyLock::new(|| {
    Mutex::new(ProcessManager::new())
});

pub struct ProcessManager {
    processes: HashMap<String, ProcessInfo>,
}

pub struct ProcessInfo {
    pub child: Child,
    pub output_lines: Vec<String>,
}

impl ProcessManager {
    pub fn new() -> Self {
        Self {
            processes: HashMap::new(),
        }
    }

    pub fn spawn(session_id: &str, command: &str, shell: &str) -> Result<(), String> {
        let child = Command::new(shell)
            .arg("-c")
            .arg(command)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Cannot spawn: {}", e))?;

        let mut mgr = MANAGER.lock().map_err(|e| format!("Lock: {}", e))?;
        mgr.processes.insert(session_id.to_string(), ProcessInfo {
            child,
            output_lines: Vec::new(),
        });
        Ok(())
    }

    pub fn kill(session_id: &str) -> Result<(), String> {
        let mut mgr = MANAGER.lock().map_err(|e| format!("Lock: {}", e))?;
        if let Some(info) = mgr.processes.get_mut(session_id) {
            info.child.kill().map_err(|e| format!("Kill error: {}", e))?;
            mgr.processes.remove(session_id);
            Ok(())
        } else {
            Err("Session not found".into())
        }
    }
}
