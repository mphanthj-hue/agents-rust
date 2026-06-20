use std::time::Duration;
use serde_json::{json, Value};
use crate::mcp::types::{ToolDefinition, ToolResult, ToolContent};

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

pub fn browser_action_definition() -> ToolDefinition {
    ToolDefinition {
        name: "browser_action".into(),
        description: "Interact with web pages. Uses Obscura CDP headless browser for full automation (screenshot, click, type, execute_js), falls back to HTTP for simple navigate/get_html.

Actions:
- navigate: Fetch a URL via HTTP and return visible text content
- get_html: Fetch a URL via HTTP and return raw HTML
- screenshot: Navigate via Obscura CDP, capture screenshot saved to /tmp/
- click: Navigate via Obscura CDP, click element by CSS selector
- type: Navigate via Obscura CDP, type text into an input field
- execute_js: Navigate via Obscura CDP, run JavaScript in page context

Use navigate for reading articles, documentation, and general web content.
Use CDP actions (screenshot/click/type/execute_js) for interactive web automation.
Install Obscura with: cargo install obscura".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["navigate", "get_html", "screenshot", "click", "type", "execute_js"],
                    "description": "Action to perform"
                },
                "url": { "type": "string", "description": "URL to navigate to" },
                "selector": { "type": "string", "description": "CSS selector (for click/type actions)" },
                "value": { "type": "string", "description": "Text value (for type action)" },
                "script": { "type": "string", "description": "JavaScript code (for execute_js action)" }
            },
            "required": ["action"]
        }),
    }
}

pub fn handle_browser_action(args: Value) -> Result<ToolResult, String> {
    let action = args.get("action")
        .and_then(|v| v.as_str())
        .ok_or("Missing: action")?;

    match action {
        "navigate" => handle_navigate(args),
        "get_html" => handle_get_html(args),
        "screenshot" => handle_screenshot(args),
        "click" => handle_click(args),
        "type" => handle_type(args),
        "execute_js" => handle_execute_js(args),
        _ => error_result(format!("Unknown action: {}. Valid: navigate, get_html, screenshot, click, type, execute_js", action)),
    }
}

fn handle_navigate(args: Value) -> Result<ToolResult, String> {
    let url = validate_url(&args)?;

    let body = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(fetch_url(&url))
    })?;

    let text = extract_text(&body);

    let preview: String = text.chars().take(5000).collect();
    let truncated = if text.len() > 5000 { format!("\n\n... (truncated, {} total chars)", text.len()) } else { String::new() };

    text_result(format!("Title: {}\n\n{}{}", extract_title(&body), preview, truncated))
}

fn handle_get_html(args: Value) -> Result<ToolResult, String> {
    let url = validate_url(&args)?;

    let body = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(fetch_url(&url))
    })?;

    let preview: String = body.chars().take(8000).collect();
    let truncated = if body.len() > 8000 { format!("\n\n... (truncated, {} total chars)", body.len()) } else { String::new() };

    text_result(format!("{}{}", preview, truncated))
}

fn handle_screenshot(args: Value) -> Result<ToolResult, String> {
    let url = args.get("url")
        .and_then(|v| v.as_str())
        .ok_or("Missing: url (required for screenshot)")?;

    let full_page = args.get("full_page")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async {
            let browser = crate::tools::obscura_browser::get_browser().await?;
            browser.navigate(url).await?;
            tokio::time::sleep(Duration::from_millis(1500)).await;
            let bytes = browser.screenshot(full_page).await?;

            let filename = format!(
                "screenshot_{}.png",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs()
            );
            let filepath = format!("/tmp/{}", filename);
            std::fs::write(&filepath, &bytes)
                .map_err(|e| format!("Failed to save screenshot: {}", e))?;

            text_result(format!(
                "Screenshot captured for {}\nFile: {}\nSize: {} bytes",
                url,
                filepath,
                bytes.len()
            ))
        })
    })
}

fn handle_click(args: Value) -> Result<ToolResult, String> {
    let url = args.get("url")
        .and_then(|v| v.as_str())
        .ok_or("Missing: url (required for click)")?;
    let selector = args.get("selector")
        .and_then(|v| v.as_str())
        .ok_or("Missing: selector (CSS selector for element to click)")?;

    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async {
            let browser = crate::tools::obscura_browser::get_browser().await?;
            browser.navigate(url).await?;
            tokio::time::sleep(Duration::from_millis(500)).await;
            browser.click(selector).await
        })
    })?;

    text_result(format!("Clicked element '{}' on {}", selector, url))
}

fn handle_type(args: Value) -> Result<ToolResult, String> {
    let url = args.get("url")
        .and_then(|v| v.as_str())
        .ok_or("Missing: url (required for type)")?;
    let selector = args.get("selector")
        .and_then(|v| v.as_str())
        .ok_or("Missing: selector (CSS selector for input element)")?;
    let value = args.get("value")
        .and_then(|v| v.as_str())
        .ok_or("Missing: value (text to type)")?;

    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async {
            let browser = crate::tools::obscura_browser::get_browser().await?;
            browser.navigate(url).await?;
            tokio::time::sleep(Duration::from_millis(500)).await;
            browser.fill(selector, value).await
        })
    })?;

    text_result(format!("Typed '{}' into element '{}' on {}", value, selector, url))
}

fn handle_execute_js(args: Value) -> Result<ToolResult, String> {
    let url = args.get("url")
        .and_then(|v| v.as_str())
        .ok_or("Missing: url (required for execute_js)")?;
    let script = args.get("script")
        .and_then(|v| v.as_str())
        .ok_or("Missing: script (JavaScript code to execute)")?;

    let result = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async {
            let browser = crate::tools::obscura_browser::get_browser().await?;
            browser.navigate(url).await?;
            tokio::time::sleep(Duration::from_millis(500)).await;
            browser.evaluate_js(script).await
        })
    })?;

    text_result(format!("JavaScript result:\n{}", result))
}

fn validate_url(args: &Value) -> Result<String, String> {
    let url = args.get("url")
        .and_then(|v| v.as_str())
        .ok_or("Missing: url (required for navigate/get_html)")?;

    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err(format!("Invalid URL: {}. Must start with http:// or https://", url));
    }

    Ok(url.to_string())
}

async fn fetch_url(url: &str) -> Result<String, String> {
    let timeout = std::time::Duration::from_secs(15);

    let client = reqwest::Client::builder()
        .timeout(timeout)
        .user_agent("Mozilla/5.0 (compatible; agents-rust/0.1.0)")
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let response = client.get(url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch {}: {}", url, e))?;

    let status = response.status();
    let body = response.text().await
        .map_err(|e| format!("Failed to read response body: {}", e))?;

    if !status.is_success() {
        return Ok(format!("HTTP {} - {}\n\n{}", status.as_u16(), status.canonical_reason().unwrap_or("Unknown"), body));
    }

    Ok(body)
}

fn extract_text(html: &str) -> String {
    let document = scraper::Html::parse_document(html);
    let sel = scraper::Selector::parse("body").unwrap();
    let mut lines: Vec<String> = Vec::new();

    if let Some(body) = document.select(&sel).next() {
        let text = body.text().collect::<Vec<_>>().join(" ");
        for line in text.lines() {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                lines.push(trimmed.to_string());
            }
        }
    }

    lines.join("\n")
}

fn extract_title(html: &str) -> String {
    let document = scraper::Html::parse_document(html);
    let sel = scraper::Selector::parse("title").unwrap();
    document.select(&sel)
        .next()
        .map(|e| e.text().collect::<String>())
        .unwrap_or_default()
        .trim()
        .to_string()
}
