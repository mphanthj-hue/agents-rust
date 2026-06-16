pub mod schemas;
pub mod filesystem;
pub mod edit;
pub mod fuzzy_search;
pub mod command;
pub mod llm;
pub mod browser;

use crate::mcp::types::{ToolDefinition, ToolResult};
use serde_json::Value;

pub fn get_all_tool_definitions() -> Vec<ToolDefinition> {
    vec![
        filesystem::read_file_definition(),
        filesystem::write_file_definition(),
        filesystem::list_directory_definition(),
        filesystem::create_directory_definition(),
        filesystem::move_file_definition(),
        filesystem::get_file_info_definition(),
        filesystem::search_files_definition(),
        filesystem::get_environment_info_definition(),
        edit::edit_block_definition(),
        command::start_process_definition(),
        command::read_process_output_definition(),
        command::interact_with_process_definition(),
        command::force_terminate_definition(),
        llm::ask_llm_definition(),
        browser::browser_action_definition(),
    ]
}

pub type ToolHandler = fn(Value) -> Result<ToolResult, String>;

pub fn get_tool_handler(name: &str) -> Option<ToolHandler> {
    match name {
        "read_file" => Some(filesystem::handle_read_file),
        "write_file" => Some(filesystem::handle_write_file),
        "list_directory" => Some(filesystem::handle_list_directory),
        "create_directory" => Some(filesystem::handle_create_directory),
        "move_file" => Some(filesystem::handle_move_file),
        "get_file_info" => Some(filesystem::handle_get_file_info),
        "search_files" => Some(filesystem::handle_search_files),
        "get_environment_info" => Some(filesystem::handle_get_environment_info),
        "edit_block" => Some(edit::handle_edit_block),
        "start_process" => Some(command::handle_start_process),
        "read_process_output" => Some(command::handle_read_process_output),
        "interact_with_process" => Some(command::handle_interact_with_process),
        "force_terminate" => Some(command::handle_force_terminate),
        "ask_llm" => Some(llm::handle_ask_llm),
        "browser_action" => Some(browser::handle_browser_action),
        _ => None,
    }
}
