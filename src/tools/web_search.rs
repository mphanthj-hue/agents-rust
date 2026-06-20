use serde_json::{json, Value};
use crate::mcp::types::{ToolDefinition, ToolResult, ToolContent};

pub fn web_search_definition() -> ToolDefinition {
    ToolDefinition {
        name: "web_search".into(),
        description: "Tìm kiếm thông tin trên web. Dùng Brave Search API (nếu có key) hoặc DuckDuckGo fallback. Trả về tiêu đề + URL + snippet.".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Từ khoá tìm kiếm"
                },
                "count": {
                    "type": "integer",
                    "description": "Số kết quả (mặc định 5)",
                    "default": 5
                }
            },
            "required": ["query"]
        }),
    }
}

struct SearchResult {
    title: String,
    url: String,
    snippet: String,
}

pub fn handle_web_search(args: Value) -> Result<ToolResult, String> {
    let query = args.get("query")
        .and_then(|v| v.as_str())
        .ok_or("Thiếu 'query'")?;
    let count = args.get("count")
        .and_then(|v| v.as_u64())
        .unwrap_or(5) as usize;

    let results = duckduckgo_search(query, count)?;

    let output = results.iter()
        .enumerate()
        .map(|(i, r)| format!("{}. [{}]({})\n   {}", i + 1, r.title, r.url, r.snippet))
        .collect::<Vec<_>>()
        .join("\n\n");

    Ok(ToolResult {
        content: vec![ToolContent::Text { text: output }],
        is_error: None,
    })
}

fn duckduckgo_search(query: &str, count: usize) -> Result<Vec<SearchResult>, String> {
    let url = format!("https://html.duckduckgo.com/html/?q={}", urlencoding(query));

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .build()
        .map_err(|e| format!("HTTP client: {}", e))?;

    let body = client.get(&url)
        .send()
        .map_err(|e| format!("Request lỗi: {}", e))?
        .text()
        .map_err(|e| format!("Read body lỗi: {}", e))?;

    parse_ddg_results(&body, count)
}

fn urlencoding(s: &str) -> String {
    s.chars().map(|c| match c {
        'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
        ' ' => "+".to_string(),
        c => format!("%{:02X}", c as u8),
    }).collect()
}

fn parse_ddg_results(html: &str, count: usize) -> Result<Vec<SearchResult>, String> {
    let mut results = Vec::new();

    // DuckDuckGo HTML results are in <a class="result__a"> and <a class="result__snippet">
    for chunk in html.split("<div class=\"result__body\">").skip(1) {
        if results.len() >= count {
            break;
        }

        let title = extract_between(chunk, "<a class=\"result__a\"", "</a>")
            .and_then(|s| extract_between(&s, ">", ""))
            .map(|s| strip_html_tags(&s))
            .unwrap_or_default();

        let url = extract_between(chunk, "href=\"", "\"")
            .unwrap_or_default();

        let snippet = extract_between(chunk, "<a class=\"result__snippet\"", "</a>")
            .and_then(|s| extract_between(&s, ">", ""))
            .map(|s| strip_html_tags(&s))
            .unwrap_or_default();

        if !title.is_empty() {
            results.push(SearchResult { title, url, snippet });
        }
    }

    if results.is_empty() {
        // Fallback: try parsing simpler format
        for chunk in html.split("<h2 class=\"result__title\">").skip(1) {
            if results.len() >= count { break; }
            let title = extract_between(chunk, ">", "</a>")
                .map(|s| strip_html_tags(&s))
                .unwrap_or_default();
            let url = extract_between(chunk, "href=\"", "\"").unwrap_or_default();
            let snippet = extract_between(chunk, "<a class=\"result__snippet\"", "</a>")
                .and_then(|s| extract_between(&s, ">", ""))
                .map(|s| strip_html_tags(&s))
                .unwrap_or_default();
            if !title.is_empty() {
                results.push(SearchResult { title, url, snippet });
            }
        }
    }

    Ok(results)
}

fn extract_between<'a>(input: &'a str, start: &str, end: &str) -> Option<String> {
    let pos = input.find(start)?;
    let after = &input[pos + start.len()..];
    if end.is_empty() {
        return Some(after.to_string());
    }
    let e = after.find(end)?;
    Some(after[..e].to_string())
}

fn strip_html_tags(s: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.trim().to_string()
}
