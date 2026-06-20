use tracing_subscriber;
use std::sync::Arc;

mod config;
mod plugin;
mod mcp;
mod tools;
mod security;
mod llm;
mod agent;
mod server;

use server::AgentsRustServer;
use rmcp::service::serve_server;
use llm::router::LlmRouter;
use llm::client::LlmClient;
use llm::types::ChatMessage;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .with_target(false)
        .init();

    // Allow quick chat test via CLI: `cargo run -- --chat "hello"`
    let args: Vec<String> = std::env::args().collect();
    if let Some(pos) = args.iter().position(|a| a == "--chat") {
        if let Some(prompt) = args.get(pos + 1) {
            return run_chat_test(prompt).await;
        }
    }

    plugin::init().unwrap_or_else(|e| eprintln!("[plugin] init error: {}", e));

    let tcp_port = std::env::var("AGENTS_RUST_TCP_PORT")
        .ok()
        .and_then(|v| v.parse::<u16>().ok());

    if let Some(port) = tcp_port {
        serve_tcp(port).await?;
    } else {
        serve_stdio().await?;
    }

    Ok(())
}

async fn run_chat_test(prompt: &str) -> Result<(), Box<dyn std::error::Error>> {
    let router = LlmRouter::from_default()?;
    let router = Arc::new(router);
    let client = LlmClient::new().with_router(router);

    let msg = ChatMessage::user(prompt);
    let resp = client.chat_with_intelligent_fallback(
        vec![ChatMessage::system("Bạn là trợ lý AI hữu ích. Trả lời ngắn gọn."), msg],
        Vec::new(),
        None,
    ).await.map_err(|e| format!("Chat lỗi: {}", e))?;

    if let Some(content) = resp.choices.first().and_then(|c| c.message.content.clone()) {
        println!("{}", content);
    }
    Ok(())
}

async fn serve_stdio() -> Result<(), Box<dyn std::error::Error>> {
    let handler = AgentsRustServer;
    let service = serve_server(handler, (tokio::io::stdin(), tokio::io::stdout())).await?;
    service.waiting().await?;
    Ok(())
}

async fn serve_tcp(port: u16) -> Result<(), Box<dyn std::error::Error>> {
    let addr = format!("0.0.0.0:{}", port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    println!("agents-rust TCP MCP server listening on tcp://{}", addr);

    loop {
        let (stream, peer) = listener.accept().await?;
        println!("New connection from: {}", peer);
        tokio::spawn(async move {
            let handler = AgentsRustServer;
            let (r, w) = stream.into_split();
            match serve_server(handler, (r, w)).await {
                Ok(service) => {
                    if let Err(e) = service.waiting().await {
                        eprintln!("Connection error: {}", e);
                    }
                }
                Err(e) => eprintln!("Failed to initialize: {}", e),
            }
        });
    }
}
