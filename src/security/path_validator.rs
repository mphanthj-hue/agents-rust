use std::path::{Path, PathBuf, Component};
use std::fs;
use crate::config;

pub fn validate_path(requested: &str) -> Result<String, String> {
    let expanded = shellexpand(requested);
    let abs = if Path::new(&expanded).is_absolute() {
        PathBuf::from(&expanded)
    } else {
        let cwd = std::env::current_dir().map_err(|e| format!("Cannot get cwd: {}", e))?;
        cwd.join(&expanded)
    };

    let abs = fs::canonicalize(&abs).map_err(|e| {
        format!("Path '{}' cannot be accessed: {}", requested, e)
    })?;

    let abs_str = abs.to_string_lossy().to_string();

    let cfg = config::get();
    if cfg.allowed_directories.contains(&"/".to_string()) {
        return Ok(abs_str);
    }

    let allowed = cfg.allowed_directories.iter().any(|dir| {
        let dir_norm = normalize_path(dir);
        abs_str.starts_with(&dir_norm)
            && (abs_str.len() == dir_norm.len()
                || abs_str[dir_norm.len()..].starts_with('/'))
    });

    if !allowed {
        return Err(format!(
            "Path not allowed: {}. Must be within: {:?}",
            requested, cfg.allowed_directories
        ));
    }

    Ok(abs_str)
}

fn shellexpand(s: &str) -> String {
    if s.starts_with("~/") || s == "~" {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/root".into());
        format!("{}{}", home, &s[1..])
    } else {
        s.to_string()
    }
}

fn normalize_path(p: &str) -> String {
    let path = Path::new(p);
    let mut result = PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::Normal(c) => result.push(c),
            Component::RootDir => result.push("/"),
            Component::Prefix(c) => result.push(c.as_os_str()),
            _ => {}
        }
    }
    result.to_string_lossy().to_string()
}
