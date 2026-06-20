use std::io::{self, Write};
use std::sync::Arc;

mod config;
mod plugin;
mod mcp;
mod tools;
mod security;
mod llm;
mod agent;
mod server;
mod orchestrator;
mod dashboard;

use server::AgentsRustServer;
use rmcp::service::serve_server;
use llm::router::LlmRouter;
use llm::client::LlmClient;
use llm::types::ChatMessage;
use agent::Agent;

const AGENT_SYSTEM_PROMPT: &str = "\
Bạn là một AI agent thông minh, được trang bị đầy đủ tools để tương tác với hệ thống. \
Hãy trả lời bằng tiếng Việt, xưng hô với người dùng là 'Anh Nghĩa' và tự xưng là 'em'.\n\n\
Bạn có các tools sau:\n\
- read_file: Đọc nội dung file (có phân trang, hỗ trợ tail)\n\
- write_file: Ghi hoặc nối thêm vào file\n\
- list_directory: Liệt kê thư mục (có depth control)\n\
- create_directory: Tạo thư mục\n\
- move_file: Di chuyển/đổi tên file\n\
- get_file_info: Xem metadata file\n\
- search_files: Tìm file theo tên, nội dung, hoặc glob pattern\n\
- get_environment_info: Xem thông tin hệ thống (OS, shell, thư mục hiện tại)\n\
- edit_block: Sửa nội dung file bằng SEARCH/REPLACE (có fuzzy matching)\n\
- start_process: Chạy lệnh terminal\n\
- read_process_output: Đọc output từ process đang chạy\n\
- interact_with_process: Gửi input vào process\n\
- force_terminate: Tắt process\n\
- ask_llm: Hỏi LLM trực tiếp (không cần dùng tool)\n\
- browser_action: Truy cập web (navigate: đọc nội dung trang, get_html: lấy raw HTML)\n\n\
Hãy suy nghĩ từng bước, chọn tool phù hợp, và giải thích cho Anh Nghĩa biết em đang làm gì.\
Khi hoàn thành task, tổng kết lại kết quả rõ ràng.";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .with_target(false)
        .init();

    plugin::init().unwrap_or_else(|e| eprintln!("[plugin] init error: {}", e));
    tools::command::init();

    let mode = Mode::from_args();

    match mode {
        Mode::Chat(prompt) => run_chat_test(&prompt).await?,
        Mode::Agent { dashboard } => run_agent(dashboard).await?,
        Mode::Orchestrate(task) => run_orchestrator(&task).await?,
        Mode::Dashboard(port) => run_dashboard(port).await?,
        Mode::Mcp(tcp_port) => run_mcp_server(tcp_port).await?,
    }

    Ok(())
}

enum Mode {
    Chat(String),
    Agent { dashboard: bool },
    Orchestrate(String),
    Dashboard(u16),
    Mcp(Option<u16>),
}

impl Mode {
    fn from_args() -> Self {
        let args: Vec<String> = std::env::args().collect();

        if let Some(pos) = args.iter().position(|a| a == "--chat") {
            if let Some(prompt) = args.get(pos + 1) {
                return Self::Chat(prompt.clone());
            }
        }

        if args.iter().any(|a| a == "--agent") {
            let dashboard = args.iter().any(|a| a == "--dashboard");
            return Self::Agent { dashboard };
        }

        if let Some(pos) = args.iter().position(|a| a == "--orchestrate") {
            if let Some(task) = args.get(pos + 1) {
                return Self::Orchestrate(task.clone());
            }
        }

        if let Some(pos) = args.iter().position(|a| a == "--dashboard") {
            let port = args.get(pos + 1)
                .and_then(|v| v.parse::<u16>().ok())
                .unwrap_or(3000);
            return Self::Dashboard(port);
        }

        let tcp_port = std::env::var("AGENTS_RUST_TCP_PORT")
            .ok()
            .and_then(|v| v.parse::<u16>().ok());
        Self::Mcp(tcp_port)
    }
}

async fn run_agent(dashboard_enabled: bool) -> Result<(), Box<dyn std::error::Error>> {
    println!("=== agents-rust Agent ===");
    println!("Chào Anh Nghĩa! Em là AI agent, có thể giúp anh làm việc với file, terminal, web.");
    if dashboard_enabled {
        let port = 3000u16;
        let dash = dashboard::DashboardServer::new(port);
        dash.start().await?;
        println!("📊 Dashboard: http://localhost:{}", port + 1);
    }
    println!("Anh gõ task vào đây, em sẽ tự động dùng tools để hoàn thành.");
    println!("Gõ 'exit' hoặc 'quit' để thoát.\n");

    loop {
        print!("Anh Nghĩa > ");
        io::stdout().flush().ok();
        let mut input = String::new();
        io::stdin().read_line(&mut input).ok();
        let input = input.trim();
        if input.is_empty() || input == "exit" || input == "quit" {
            break;
        }

        let mut agent = Agent::new();
        agent.add_system_prompt(AGENT_SYSTEM_PROMPT);
        agent.add_user_message(input);
        match agent.run().await {
            Ok(answer) => {
                println!("\n{}", answer);
            }
            Err(e) => {
                eprintln!("\nLỗi: {}", e);
            }
        }
        println!();
    }

    Ok(())
}

async fn run_orchestrator(task: &str) -> Result<(), Box<dyn std::error::Error>> {
    println!("🧠 Orchestrator: '{}'", task);

    let tools: Vec<String> = tools::get_all_tool_definitions()
        .into_iter()
        .map(|t| t.name)
        .collect();

    let orch = orchestrator::Orchestrator::new();
    let config = orchestrator::types::WorkerConfig::default();

    let result = orch.run(task, &tools, config).await
        .map_err(|e| format!("Orchestrator lỗi: {}", e))?;

    println!("\n✅ {} / {} subtask thành công", result.successes, result.total);
    if !result.synthesis.is_empty() {
        println!("\n📝 Tổng hợp:\n{}", result.synthesis);
    }
    for r in &result.results {
        if let Some(ref err) = r.error {
            println!("❌ [{}]: {}", r.id, err);
        }
    }

    Ok(())
}

async fn run_dashboard(port: u16) -> Result<(), Box<dyn std::error::Error>> {
    println!("📊 Starting dashboard on ws://localhost:{}", port);
    let dash = dashboard::DashboardServer::new(port);
    dash.start().await?;
    println!("📊 HTTP dashboard: http://localhost:{}", port + 1);
    println!("Press Ctrl+C to stop.");

    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(3600)).await;
    }
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

async fn run_mcp_server(tcp_port: Option<u16>) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(port) = tcp_port {
        serve_tcp(port).await?;
    } else {
        serve_stdio().await?;
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