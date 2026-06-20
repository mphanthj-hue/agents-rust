use std::fs;
use std::path::Path;

use std::os::unix::fs::PermissionsExt;
use std::env;
use serde_json::{json, Value};
use walkdir::WalkDir;
use crate::mcp::types::{ToolDefinition, ToolResult, ToolContent};
use crate::security::validate_path;
use crate::config;

fn text_result(text: impl Into<String>) -> Result<ToolResult, String> {
    Ok(ToolResult {
        content: vec![ToolContent::Text { text: text.into() }],
        is_error: None,
    })
}

fn error_result(text: impl Into<String>) -> Result<ToolResult, String> {
    Ok(ToolResult {
        content: vec![ToolContent::Text { text: text.into() }],
        is_error: Some(true),
    })
}

pub fn read_file_definition() -> ToolDefinition {
    ToolDefinition {
        name: "read_file".into(),
        description: "Read file contents with pagination support.

Supports:
- offset/length for line-based pagination
- Negative offset for tail behavior (last N lines)

Examples:
- offset:0, length:10  → First 10 lines
- offset:-20           → Last 20 lines
- offset:100, length:5 → Lines 100-104".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "file_path": { "type": "string", "description": "Path to the file" },
                "offset": { "type": "integer", "description": "Start line (0-based). Negative = tail", "default": 0 },
                "length": { "type": "integer", "description": "Max lines to read", "default": 1000 }
            },
            "required": ["file_path"]
        }),
    }
}

pub fn handle_read_file(args: Value) -> Result<ToolResult, String> {
    let file_path = args.get("file_path")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: file_path")?;
    let offset = args.get("offset").and_then(|v| v.as_i64()).unwrap_or(0);
    let length = args.get("length").and_then(|v| v.as_i64())
        .map(|l| l as usize)
        .unwrap_or(config::get().file_read_line_limit);

    let valid_path = validate_path(file_path)?;
    let path = Path::new(&valid_path);

    if path.is_dir() {
        return error_result(format!("'{}' is a directory. Use list_directory instead.", file_path));
    }

    // Check file size (max 10MB)
    let meta = fs::metadata(path).map_err(|e| format!("Cannot stat '{}': {}", file_path, e))?;
    if meta.len() > 10_000_000 {
        return error_result(format!(
            "File '{}' is {:.1}MB (max: 10MB). Use search_files with content type instead.",
            file_path, meta.len() as f64 / 1_000_000.0
        ));
    }

    // Read once, detect binary
    let data = fs::read(path).map_err(|e| format!("Cannot read '{}': {}", file_path, e))?;
    let sample = &data[..data.len().min(8192)];
    if sample.contains(&0) {
        return text_result(format!(
            "[Binary file: {} bytes. Contents not displayed.]",
            meta.len()
        ));
    }

    let content = String::from_utf8(data)
        .map_err(|e| format!("File '{}' is not valid UTF-8: {}", file_path, e))?;
    let lines: Vec<&str> = content.lines().collect();

    let total = lines.len();

    let (start, end) = if offset < 0 {
        let tail = (-offset) as usize;
        let start = total.saturating_sub(tail);
        (start, total)
    } else {
        let start = offset as usize;
        let end = std::cmp::min(start + length, total);
        (start, end)
    };

    if start >= total {
        return text_result(format!(
            "[Reading 0 lines (total: {} lines)]\n(offset {} is beyond file end)",
            total, offset
        ));
    }

    let selected: Vec<&str> = lines[start..end].to_vec();
    let content = selected.join("\n");
    let remaining = total - end;

    Ok(ToolResult {
        content: vec![ToolContent::Text {
            text: format!(
                "[Reading {} lines from line {} (total: {} lines, {} remaining)]\n\n{}",
                end - start, start, total, remaining, content
            ),
        }],
        is_error: None,
    })
}

pub fn write_file_definition() -> ToolDefinition {
    ToolDefinition {
        name: "write_file".into(),
        description: "Write or append to a file. Use chunking for large files (25-30 lines per chunk).".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "file_path": { "type": "string", "description": "Path to file" },
                "content": { "type": "string", "description": "File content" },
                "mode": { "type": "string", "enum": ["rewrite", "append"], "description": "Write mode", "default": "rewrite" }
            },
            "required": ["file_path", "content"]
        }),
    }
}

pub fn handle_write_file(args: Value) -> Result<ToolResult, String> {
    let file_path = args.get("file_path")
        .and_then(|v| v.as_str())
        .ok_or("Missing: file_path")?;
    let content = args.get("content")
        .and_then(|v| v.as_str())
        .ok_or("Missing: content")?;
    let mode = args.get("mode").and_then(|v| v.as_str()).unwrap_or("rewrite");

    let valid_path = validate_path(file_path)?;
    let path = Path::new(&valid_path);

    // Create parent dirs if needed
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Cannot create dirs: {}", e))?;
    }

    let line_count = content.lines().count();
    let limit = config::get().file_write_line_limit;

    match mode {
        "append" => {
            let mut file = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .map_err(|e| format!("Cannot append: {}", e))?;
            use std::io::Write;
            file.write_all(content.as_bytes())
                .map_err(|e| format!("Write error: {}", e))?;
        }
        _ => {
            fs::write(path, content)
                .map_err(|e| format!("Cannot write '{}': {}", file_path, e))?;
        }
    }

    let mut msg = format!("Successfully wrote {} lines to {}", line_count, file_path);
    if line_count > limit {
        msg.push_str(&format!(
            "\n\nWARNING: File has {} lines (recommended max: {}). Consider smaller chunks.",
            line_count, limit
        ));
    }

    text_result(msg)
}

pub fn list_directory_definition() -> ToolDefinition {
    ToolDefinition {
        name: "list_directory".into(),
        description: "List directory contents with depth control. Shows [DIR] and [FILE] prefixes.".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Directory path" },
                "depth": { "type": "integer", "description": "Recursion depth (default: 2)", "default": 2 }
            },
            "required": ["path"]
        }),
    }
}

pub fn handle_list_directory(args: Value) -> Result<ToolResult, String> {
    let dir_path = args.get("path")
        .and_then(|v| v.as_str())
        .ok_or("Missing: path")?;
    let depth = args.get("depth").and_then(|v| v.as_i64()).unwrap_or(2) as usize;

    let valid_path = validate_path(dir_path)?;
    let mut results: Vec<String> = Vec::new();

    let _max_nested = 100usize;

    for entry in WalkDir::new(&valid_path).max_depth(depth).into_iter().filter_entry(|e| {
        // Skip hidden dirs at top level
        e.depth() == 0 || !e.file_name().to_string_lossy().starts_with('.')
    }) {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        let display_path = entry.path().strip_prefix(&valid_path)
            .unwrap_or(entry.path())
            .to_string_lossy()
            .to_string();

        if display_path.is_empty() { continue; }

        if entry.file_type().is_dir() {
            results.push(format!("[DIR] {}", display_path));
        } else {
            results.push(format!("[FILE] {}", display_path));
        }
    }

    text_result(results.join("\n"))
}

pub fn create_directory_definition() -> ToolDefinition {
    ToolDefinition {
        name: "create_directory".into(),
        description: "Create directory(ies) recursively.".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Directory path" }
            },
            "required": ["path"]
        }),
    }
}

pub fn handle_create_directory(args: Value) -> Result<ToolResult, String> {
    let dir_path = args.get("path")
        .and_then(|v| v.as_str())
        .ok_or("Missing: path")?;
    let valid_path = validate_path(dir_path)?;
    fs::create_dir_all(&valid_path)
        .map_err(|e| format!("Cannot create directory '{}': {}", dir_path, e))?;
    text_result(format!("Created directory {}", dir_path))
}

pub fn move_file_definition() -> ToolDefinition {
    ToolDefinition {
        name: "move_file".into(),
        description: "Move or rename a file/directory.".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "source": { "type": "string", "description": "Source path" },
                "destination": { "type": "string", "description": "Destination path" }
            },
            "required": ["source", "destination"]
        }),
    }
}

pub fn handle_move_file(args: Value) -> Result<ToolResult, String> {
    let src = args.get("source").and_then(|v| v.as_str()).ok_or("Missing: source")?;
    let dst = args.get("destination").and_then(|v| v.as_str()).ok_or("Missing: destination")?;
    let valid_src = validate_path(src)?;
    let valid_dst = validate_path(dst)?;
    fs::rename(&valid_src, &valid_dst)
        .map_err(|e| format!("Cannot move '{}' to '{}': {}", src, dst, e))?;
    text_result(format!("Moved {} to {}", src, dst))
}

pub fn get_file_info_definition() -> ToolDefinition {
    ToolDefinition {
        name: "get_file_info".into(),
        description: "Get metadata for a file or directory (size, modified, permissions).".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "file_path": { "type": "string", "description": "Path to file/directory" }
            },
            "required": ["file_path"]
        }),
    }
}

pub fn handle_get_file_info(args: Value) -> Result<ToolResult, String> {
    let file_path = args.get("file_path")
        .and_then(|v| v.as_str())
        .ok_or("Missing: file_path")?;
    let valid_path = validate_path(file_path)?;
    let meta = fs::metadata(&valid_path)
        .map_err(|e| format!("Cannot stat '{}': {}", file_path, e))?;

    let info = json!({
        "size": meta.len(),
        "is_directory": meta.is_dir(),
        "is_file": meta.is_file(),
        "permissions": format!("{:o}", meta.permissions().mode() & 0o777),
        "modified": meta.modified()
            .map(|t| chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339())
            .unwrap_or_default(),
        "created": meta.created()
            .map(|t| chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339())
            .unwrap_or_default(),
    });

    text_result(serde_json::to_string_pretty(&info).unwrap())
}

pub fn search_files_definition() -> ToolDefinition {
    ToolDefinition {
        name: "search_files".into(),
        description: "Search for files by name pattern, content pattern, or glob pattern.".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "root_path": { "type": "string", "description": "Root directory to search" },
                "pattern": { "type": "string", "description": "Search pattern (substring or glob)" },
                "search_type": { "type": "string", "enum": ["files", "content", "glob"], "description": "Search by filename, file content, or glob pattern", "default": "files" }
            },
            "required": ["root_path", "pattern"]
        }),
    }
}

fn glob_match(pattern: &str, name: &str) -> bool {
    let pat_chars: Vec<char> = pattern.chars().collect();
    let name_chars: Vec<char> = name.chars().collect();
    let mut dp = vec![vec![false; name_chars.len() + 1]; pat_chars.len() + 1];
    dp[0][0] = true;

    for i in 1..=pat_chars.len() {
        if pat_chars[i - 1] == '*' {
            dp[i][0] = dp[i - 1][0];
        }
    }

    for i in 1..=pat_chars.len() {
        for j in 1..=name_chars.len() {
            match pat_chars[i - 1] {
                '*' => dp[i][j] = dp[i - 1][j] || dp[i][j - 1],
                '?' => dp[i][j] = dp[i - 1][j - 1],
                c => dp[i][j] = dp[i - 1][j - 1] && c.eq_ignore_ascii_case(&name_chars[j - 1]),
            }
        }
    }
    dp[pat_chars.len()][name_chars.len()]
}

pub fn handle_search_files(args: Value) -> Result<ToolResult, String> {
    let root = args.get("root_path").and_then(|v| v.as_str()).ok_or("Missing: root_path")?;
    let pattern = args.get("pattern").and_then(|v| v.as_str()).ok_or("Missing: pattern")?;
    let search_type = args.get("search_type").and_then(|v| v.as_str()).unwrap_or("files");

    let valid_root = validate_path(root)?;
    let mut results: Vec<String> = Vec::new();
    let lower_pattern = pattern.to_lowercase();

    for entry in WalkDir::new(&valid_root).max_depth(10).into_iter().filter_entry(|e| {
        !e.file_name().to_string_lossy().starts_with('.')
    }) {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        if !entry.file_type().is_file() { continue; }

        let name = entry.file_name().to_string_lossy();

        match search_type {
            "glob" => {
                if glob_match(pattern, &name) {
                    let rel = entry.path().strip_prefix(&valid_root).unwrap_or(entry.path());
                    results.push(rel.to_string_lossy().to_string());
                }
            }
            "files" => {
                if name.to_lowercase().contains(&lower_pattern) {
                    let rel = entry.path().strip_prefix(&valid_root).unwrap_or(entry.path());
                    results.push(rel.to_string_lossy().to_string());
                }
            }
            "content" => {
                if let Ok(content) = fs::read_to_string(entry.path()) {
                    if content.to_lowercase().contains(&lower_pattern) {
                        let rel = entry.path().strip_prefix(&valid_root).unwrap_or(entry.path());
                        results.push(rel.to_string_lossy().to_string());
                    }
                }
            }
            _ => {}
        }
    }

    if results.is_empty() {
        return text_result(format!("No {} found matching pattern '{}' in {}", search_type, pattern, root));
    }

    text_result(format!(
        "Found {} matching {}:\n{}",
        results.len(),
        if results.len() == 1 { "file" } else { "files" },
        results.join("\n")
    ))
}

pub fn get_environment_info_definition() -> ToolDefinition {
    ToolDefinition {
        name: "get_environment_info".into(),
        description: "Get system environment information (OS, shell, current directory, env vars).".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "include_env_vars": { "type": "boolean", "description": "Include environment variables", "default": false }
            }
        }),
    }
}

pub fn handle_get_environment_info(args: Value) -> Result<ToolResult, String> {
    let include_env_vars = args.get("include_env_vars").and_then(|v| v.as_bool()).unwrap_or(false);

    let cwd = env::current_dir().map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
    let os = env::consts::OS;
    let shell = config::get().default_shell;
    let home = env::var("HOME").unwrap_or_default();
    let user = env::var("USER").unwrap_or_default();

    let mut info = json!({
        "os": os,
        "current_directory": cwd,
        "default_shell": shell,
        "home": home,
        "user": user,
        "hostname": env::var("HOSTNAME").unwrap_or_default(),
        "path_separator": if cfg!(windows) { ";" } else { ":" },
    });

    if include_env_vars {
        let vars: std::collections::BTreeMap<String, String> = env::vars().collect();
        info["environment_variables"] = json!(vars);
    }

    text_result(serde_json::to_string_pretty(&info).unwrap())
}
