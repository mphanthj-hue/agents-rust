use std::io::{self, Write};
use agents_rust::agent::Agent;

const SYSTEM_PROMPT: &str = "\
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

fn new_agent() -> Agent {
    let mut agent = Agent::new();
    agent.add_system_prompt(SYSTEM_PROMPT);
    agent
}

#[tokio::main]
async fn main() {
    println!("=== agents-rust Agent ===");
    println!("Chào Anh Nghĩa! Em là AI agent, có thể giúp anh làm việc với file, terminal, web.");
    println!("Anh gõ task vào đây, em sẽ tự động dùng tools để hoàn thành.");
    println!("Gõ 'exit' hoặc 'quit' để thoát.\n");

    let mut agent = new_agent();

    loop {
        print!("Anh Nghĩa > ");
        io::stdout().flush().ok();
        let mut input = String::new();
        io::stdin().read_line(&mut input).ok();
        let input = input.trim();
        if input.is_empty() || input == "exit" || input == "quit" {
            break;
        }

        agent.add_user_message(input);
        match agent.run().await {
            Ok(answer) => {
                println!("\n{}", answer);
                agent = new_agent();
            }
            Err(e) => {
                eprintln!("\nLỗi: {}", e);
                agent = new_agent();
            }
        }
        println!();
    }
}
