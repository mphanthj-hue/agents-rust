use std::collections::HashMap;
use std::sync::{Mutex, LazyLock, Arc};
use std::process::{Stdio, Child};
use std::thread;
use std::time::{Duration, Instant};
use std::io::{BufRead, BufReader, Read, Write};
use serde_json::{json, Value};
use crate::mcp::types::{ToolDefinition, ToolResult, ToolContent};
use crate::config;
use shellwords;

static SESSIONS: LazyLock<Mutex<HashMap<String, SessionState>>> = LazyLock::new(|| {
    Mutex::new(HashMap::new())
});

static CLEANUP_INTERVAL: Duration = Duration::from_secs(30);

struct SessionState {
    child: Arc<Mutex<Child>>,
    stdout_lines: Arc<Mutex<Vec<String>>>,
    stderr_lines: Arc<Mutex<Vec<String>>>,
    finished: Arc<Mutex<bool>>,
    finished_at: Arc<Mutex<Option<Instant>>>,
}

fn text_result(text: impl Into<String>) -> Result<ToolResult, String> {
    Ok(ToolResult {
        content: vec![ToolContent::Text { text: text.into() }],
        is_error: None,
    })
}

#[allow(dead_code)]
fn error_result(text: impl Into<String>) -> Result<ToolResult, String> {
    Ok(ToolResult {
        content: vec![ToolContent::Text { text: text.into() }],
        is_error: Some(true),
    })
}

fn check_blocked(command: &str) -> Result<(), String> {
    let cfg = config::get();
    
    let commands = parse_shell_commands(command);
    
    for blocked in &cfg.blocked_commands {
        for cmd in &commands {
            if cmd == blocked {
                return Err(format!("Command blocked: {}", blocked));
            }
        }
    }
    
    if has_shell_metacharacters(command) {
        return Err("Command contains shell metacharacters that could bypass blocking".into());
    }
    
    Ok(())
}

fn parse_shell_commands(command: &str) -> Vec<String> {
    let mut commands = Vec::new();
    let mut current = String::new();
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut escaped = false;
    let chars: Vec<char> = command.chars().collect();
    
    for (i, &ch) in chars.iter().enumerate() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }
        
        match ch {
            '\\' => {
                if in_double_quote {
                    escaped = true;
                    current.push(ch);
                } else {
                    escaped = true;
                    current.push(ch);
                }
            }
            '\'' => {
                if !in_double_quote {
                    in_single_quote = !in_single_quote;
                }
                current.push(ch);
            }
            '"' => {
                if !in_single_quote {
                    in_double_quote = !in_double_quote;
                }
                current.push(ch);
            }
            ';' | '|' | '&' => {
                if !in_single_quote && !in_double_quote {
                    if !current.trim().is_empty() {
                        commands.push(current.trim().to_string());
                    }
                    current.clear();
                    if i + 1 < chars.len() && (chars[i + 1] == '|' || chars[i + 1] == '&') {
                        current.push(ch);
                        current.push(chars[i + 1]);
                        continue;
                    }
                    continue;
                }
                current.push(ch);
            }
            '$' => {
                if !in_single_quote && i + 1 < chars.len() && chars[i + 1] == '(' {
                    if !current.trim().is_empty() {
                        commands.push(current.trim().to_string());
                    }
                    current.clear();
                    current.push('$');
                    current.push('(');
                    continue;
                }
                current.push(ch);
            }
            '`' => {
                if !in_single_quote && !in_double_quote {
                    if !current.trim().is_empty() {
                        commands.push(current.trim().to_string());
                    }
                    current.clear();
                    current.push('`');
                    continue;
                }
                current.push(ch);
            }
            _ => {
                current.push(ch);
            }
        }
    }
    
    if !current.trim().is_empty() {
        commands.push(current.trim().to_string());
    }
    
    commands.iter()
        .filter_map(|c| shellwords::split(c).ok())
        .flatten()
        .map(|s| s.split_whitespace().next().unwrap_or("").to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn has_shell_metacharacters(command: &str) -> bool {
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut escaped = false;
    
    for ch in command.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        
        match ch {
            '\\' => escaped = true,
            '\'' => if !in_double_quote { in_single_quote = !in_single_quote },
            '"' => if !in_single_quote { in_double_quote = !in_double_quote },
            ';' | '|' | '&' | '$' | '`' => if !in_single_quote && !in_double_quote { return true },
            _ => {}
        }
    }
    
    false
}

pub fn start_process_definition() -> ToolDefinition {
    ToolDefinition {
        name: "start_process".into(),
        description: "Execute a command and get output. For long-running processes, use session_id to keep the process alive.".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "command": { "type": "string", "description": "Command to execute" },
                "session_id": { "type": "string", "description": "Optional session ID to keep process alive for later interaction" },
                "timeout": { "type": "integer", "description": "Max execution time in seconds (0 = no limit)", "default": 30 }
            },
            "required": ["command"]
        }),
    }
}

pub fn handle_start_process(args: Value) -> Result<ToolResult, String> {
    tokio::task::block_in_place(|| {
        let command = args.get("command")
            .and_then(|v| v.as_str())
            .ok_or("Missing: command")?;
        let session_id = args.get("session_id").and_then(|v| v.as_str()).unwrap_or("");
        let timeout = args.get("timeout").and_then(|v| v.as_i64()).unwrap_or(30);

        check_blocked(command)?;

        if session_id.is_empty() {
            return run_sync(command, timeout);
        }

        let cfg = config::get();
        let shell = &cfg.default_shell;

        let mut child = std::process::Command::new(shell)
            .arg("-c")
            .arg(command)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Failed to spawn: {}", e))?;

        let child_stdout = child.stdout.take().ok_or("No stdout")?;
        let child_stderr = child.stderr.take().ok_or("No stderr")?;

        let stdout_lines: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let stderr_lines: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let finished: Arc<Mutex<bool>> = Arc::new(Mutex::new(false));
        let finished_at: Arc<Mutex<Option<Instant>>> = Arc::new(Mutex::new(None));

        let sl = Arc::clone(&stdout_lines);
        thread::spawn(move || read_lines(child_stdout, sl));
        let sl = Arc::clone(&stderr_lines);
        thread::spawn(move || read_lines(child_stderr, sl));

        let child = Arc::new(Mutex::new(child));
        {
            let child = Arc::clone(&child);
            let fin = Arc::clone(&finished);
            let fin_at = Arc::clone(&finished_at);
            thread::spawn(move || {
                loop {
                    match child.lock().unwrap().try_wait() {
                        Ok(Some(_)) => {
                            *fin.lock().unwrap() = true;
                            *fin_at.lock().unwrap() = Some(Instant::now());
                            return;
                        }
                        Ok(None) => thread::sleep(std::time::Duration::from_millis(100)),
                        Err(_) => return,
                    }
                }
            });
        }

        let state = SessionState {
            child,
            stdout_lines,
            stderr_lines,
            finished,
            finished_at,
        };

        SESSIONS.lock().map_err(|e| format!("Lock: {}", e))?
            .insert(session_id.to_string(), state);

        thread::sleep(std::time::Duration::from_millis(200));
        let out = collect_output(session_id, 0, 100)?;

        text_result(format!("Started process '{}' (session: {})\n\n{}", command, session_id, out))
    })
}

fn run_sync(command: &str, timeout_secs: i64) -> Result<ToolResult, String> {
    let cfg = config::get();
    let shell = &cfg.default_shell;

    let (tx, rx) = std::sync::mpsc::channel();
    let cmd = command.to_string();
    let sh = shell.clone();

    thread::spawn(move || {
        let result = std::process::Command::new(sh)
            .arg("-c")
            .arg(&cmd)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output();
        let _ = tx.send(result);
    });

    let output = if timeout_secs > 0 {
        let timeout = std::time::Duration::from_secs(timeout_secs as u64);
        rx.recv_timeout(timeout)
            .map_err(|_| format!("Command timed out after {}s", timeout_secs))?
            .map_err(|e| format!("Failed to execute: {}", e))?
    } else {
        rx.recv()
            .map_err(|_| "Command thread error".to_string())?
            .map_err(|e| format!("Failed to execute: {}", e))?
    };

    let mut result = String::new();
    if !output.stdout.is_empty() {
        result.push_str(&String::from_utf8_lossy(&output.stdout));
    }
    if !output.stderr.is_empty() {
        if !result.is_empty() { result.push_str("\n--- stderr ---\n"); }
        result.push_str(&String::from_utf8_lossy(&output.stderr));
    }
    if result.is_empty() {
        result = "Command completed with no output.".into();
    }

    let exit_code = output.status.code().unwrap_or(-1);
    let prefix = format!("[Exit code: {}]\n", exit_code);
    text_result(format!("{}{}", prefix, result))
}

fn read_lines<R: Read + Send + 'static>(reader: R, lines: Arc<Mutex<Vec<String>>>) {
    let buf = BufReader::new(reader);
    for l in buf.lines().map_while(Result::ok) {
        lines.lock().unwrap().push(l);
    }
}

fn collect_output(session_id: &str, offset: i64, length: i64) -> Result<String, String> {
    let sessions = SESSIONS.lock().map_err(|e| format!("Lock: {}", e))?;
    let state = sessions.get(session_id).ok_or("Session not found")?;

    let stdout = state.stdout_lines.lock().unwrap();
    let stderr = state.stderr_lines.lock().unwrap();
    let finished = *state.finished.lock().unwrap();

    let all_lines: Vec<String> = {
        let mut combined = stdout.clone();
        if !stderr.is_empty() {
            combined.push("--- stderr ---".into());
            combined.extend(stderr.clone());
        }
        combined
    };

    let total = all_lines.len();
    let start = if offset < 0 {
        let tail = (-offset) as usize;
        total.saturating_sub(tail)
    } else {
        offset as usize
    };
    let end = std::cmp::min(start + length as usize, total);

    if start >= total {
        return Ok("[No new output]".into());
    }

    let content = all_lines[start..end].join("\n");
    let remaining = total - end;

    Ok(format!(
        "[Read {} lines from line {} (total: {} lines, {} remaining)]{}\n\n{}",
        end - start, start, total, remaining,
        if finished { " [PROCESS FINISHED]" } else { "" },
        content
    ))
}

pub fn read_process_output_definition() -> ToolDefinition {
    ToolDefinition {
        name: "read_process_output".into(),
        description: "Read output from a running process session.".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "session_id": { "type": "string", "description": "Session ID" },
                "offset": { "type": "integer", "description": "Line offset (negative = tail)", "default": 0 },
                "length": { "type": "integer", "description": "Max lines to read", "default": 1000 }
            },
            "required": ["session_id"]
        }),
    }
}

pub fn handle_read_process_output(args: Value) -> Result<ToolResult, String> {
    let session_id = args.get("session_id")
        .and_then(|v| v.as_str())
        .ok_or("Missing: session_id")?;
    let offset = args.get("offset").and_then(|v| v.as_i64()).unwrap_or(0);
    let length = args.get("length").and_then(|v| v.as_i64()).unwrap_or(1000);

    let out = collect_output(session_id, offset, length)?;
    text_result(out)
}

pub fn interact_with_process_definition() -> ToolDefinition {
    ToolDefinition {
        name: "interact_with_process".into(),
        description: "Send input to a running process (stdin).".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "session_id": { "type": "string", "description": "Session ID" },
                "input": { "type": "string", "description": "Input text to send" }
            },
            "required": ["session_id", "input"]
        }),
    }
}

pub fn handle_interact_with_process(args: Value) -> Result<ToolResult, String> {
    let session_id = args.get("session_id")
        .and_then(|v| v.as_str())
        .ok_or("Missing: session_id")?;
    let input = args.get("input")
        .and_then(|v| v.as_str())
        .ok_or("Missing: input")?;

    let sessions = SESSIONS.lock().map_err(|e| format!("Lock: {}", e))?;
    let state = sessions.get(session_id).ok_or("Session not found")?;

    let mut child = state.child.lock().unwrap();
    if let Some(stdin) = child.stdin.as_mut() {
        stdin.write_all(input.as_bytes())
            .map_err(|e| format!("Write to stdin failed: {}", e))?;
        stdin.flush().map_err(|e| format!("Flush failed: {}", e))?;
        text_result(format!("Sent {} bytes to process '{}'", input.len(), session_id))
    } else {
        text_result(String::from("Process has no stdin (already finished or piped)"))
    }
}

pub fn force_terminate_definition() -> ToolDefinition {
    ToolDefinition {
        name: "force_terminate".into(),
        description: "Force terminate a running process session.".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "session_id": { "type": "string", "description": "Session ID to kill" }
            },
            "required": ["session_id"]
        }),
    }
}

#[allow(dead_code)]
pub fn init() {
    start_cleanup_thread();
}

pub fn handle_force_terminate(args: Value) -> Result<ToolResult, String> {
    let session_id = args.get("session_id")
        .and_then(|v| v.as_str())
        .ok_or("Missing: session_id")?;

    let mut sessions = SESSIONS.lock().map_err(|e| format!("Lock: {}", e))?;
    let state = sessions.remove(session_id).ok_or("Session not found")?;

    let mut child = state.child.lock().unwrap();
    let _ = child.kill();
    let _ = child.wait();

    *state.finished_at.lock().unwrap() = Some(Instant::now());

    text_result(format!("Process '{}' terminated", session_id))
}

fn cleanup_finished_sessions() {
    let mut sessions = SESSIONS.lock().unwrap();
    let mut to_remove = Vec::new();
    let now = Instant::now();
    let ttl = Duration::from_secs(300); // 5 minutes TTL

    for (session_id, state) in sessions.iter() {
        let finished_at = state.finished_at.lock().unwrap();
        if let Some(finished_time) = *finished_at {
            if now.duration_since(finished_time) > ttl {
                to_remove.push(session_id.clone());
            }
        }
    }

    for session_id in to_remove {
        let state = sessions.remove(&session_id).unwrap();
        let mut child = state.child.lock().unwrap();
        let _ = child.kill();
        let _ = child.wait();
    }
}

fn start_cleanup_thread() {
    thread::spawn(|| {
        loop {
            thread::sleep(CLEANUP_INTERVAL);
            cleanup_finished_sessions();
        }
    });
}
