use std::fs;
use std::path::Path;
use serde_json::Value;
use crate::mcp::types::{ToolDefinition, ToolResult, ToolContent};
use crate::tools::fuzzy_search::{find_closest, highlight_diff, is_similar_enough};
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

pub fn edit_block_definition() -> ToolDefinition {
    ToolDefinition {
        name: "edit_block".into(),
        description: "Apply surgical text replacements using SEARCH/REPLACE blocks.

Best practice: Make multiple small, focused edits rather than one large edit.

Format:
```
filepath.ext
<<<<<<< SEARCH
exact text to find
=======
replacement text
>>>>>>> REPLACE
```

Supports fuzzy matching if exact text is not found.
Use expected_replacements parameter (default: 1).".into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": { "type": "string", "description": "Path to the file to edit" },
                "old_string": { "type": "string", "description": "Text to search for" },
                "new_string": { "type": "string", "description": "Replacement text" },
                "expected_replacements": { "type": "integer", "description": "Expected number of matches", "default": 1 }
            },
            "required": ["file_path", "old_string", "new_string"]
        }),
    }
}

pub fn handle_edit_block(args: Value) -> Result<ToolResult, String> {
    let file_path = args.get("file_path")
        .and_then(|v| v.as_str())
        .ok_or("Missing: file_path")?;
    let old_string = args.get("old_string")
        .and_then(|v| v.as_str())
        .ok_or("Missing: old_string")?;
    let new_string = args.get("new_string")
        .and_then(|v| v.as_str())
        .ok_or("Missing: new_string")?;
    let expected = args.get("expected_replacements")
        .and_then(|v| v.as_i64())
        .unwrap_or(1) as usize;

    if old_string.is_empty() {
        return error_result("Empty search strings are not allowed.");
    }

    let valid_path = validate_path(file_path)?;
    let content = fs::read_to_string(&valid_path)
        .map_err(|e| format!("Cannot read '{}': {}", file_path, e))?;

    // Count exact matches
    let count = content.matches(old_string).count();

    if count > 0 && count == expected {
        let new_content = content.replace(old_string, new_string);
        let limit = config::get().file_write_line_limit as usize;

        let new_lines = new_content.lines().count();
        if new_lines > limit {
            fs::write(&valid_path, &new_content)
                .map_err(|e| format!("Write error: {}", e))?;

            // Show preview centered on edit
            let pos = new_content.find(new_string).unwrap_or(0);
            let before = &new_content[..pos];
            let start_line = before.lines().count();
            let context = 10;
            let lines: Vec<&str> = new_content.lines().collect();
            let total = lines.len();
            let preview_start = if start_line > context { start_line - context } else { 0 };
            let preview_end = std::cmp::min(start_line + new_string.lines().count() + context, total);
            let preview: Vec<&str> = lines[preview_start..preview_end].iter().copied().collect();
            let remaining = total - preview_end;

            return Ok(ToolResult {
                content: vec![ToolContent::Text {
                    text: format!(
                        "[Reading {} lines from line {} (total: {} lines, {} remaining)]\n\n{}",
                        preview.len(), preview_start, total, remaining, preview.join("\n")
                    ),
                }],
                is_error: None,
            });
        }

        fs::write(&valid_path, &new_content)
            .map_err(|e| format!("Write error: {}", e))?;
        return text_result(format!("Successfully replaced {} occurrence(s) in {}", count, file_path));
    }

    if count > 0 && count != expected {
        return error_result(format!(
            "Expected {} occurrences but found {} in {}. \
             Use expected_replacements={} to replace all, or make the search string more unique.",
            expected, count, file_path, count
        ));
    }

    // No exact match → fuzzy search
    let fuzzy = find_closest(&content, old_string);

    if fuzzy.similarity == 0.0 {
        return error_result(format!("Search text not found in {}. No similar text found either.", file_path));
    }

    let diff = highlight_diff(old_string, &fuzzy.value);

    if is_similar_enough(fuzzy.similarity) {
        // Replace with fuzzy match
        let new_content = content.replacen(&fuzzy.value, new_string, 1);
        fs::write(&valid_path, &new_content)
            .map_err(|e| format!("Write error: {}", e))?;

        return text_result(format!(
            "Exact match not found, but found similar text ({:.0}% similarity). Applied replacement.\n\
             Differences:\n{}\n\nLocation: line approx {}",
            fuzzy.similarity * 100.0,
            diff,
            content[..fuzzy.start].lines().count()
        ));
    }

    error_result(format!(
        "Search text not found in {}. Closest match has only {:.0}% similarity (threshold: 70%).\n\
         Closest: {:?}\nDifferences:\n{}",
        file_path,
        fuzzy.similarity * 100.0,
        fuzzy.value,
        diff
    ))
}
