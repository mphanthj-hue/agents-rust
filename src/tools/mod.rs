pub mod filesystem;
pub mod edit;
pub mod fuzzy_search;
pub mod command;
pub mod llm;
pub mod browser;
pub mod obscura_browser;
pub mod mcp_client;
pub mod research;

use std::sync::LazyLock;
use crate::mcp::types::{ToolDefinition, ToolResult};
use serde_json::Value;

pub type ToolHandler = fn(Value) -> Result<ToolResult, String>;

pub struct ToolEntry {
    pub definition: ToolDefinition,
    pub handler: ToolHandler,
}

impl ToolEntry {
    fn new(definition: ToolDefinition, handler: ToolHandler) -> Self {
        Self { definition, handler }
    }
}

fn all_entries() -> &'static [ToolEntry] {
    static ENTRIES: LazyLock<Vec<ToolEntry>> = LazyLock::new(|| {
        vec![
            ToolEntry::new(filesystem::read_file_definition(), filesystem::handle_read_file),
            ToolEntry::new(filesystem::write_file_definition(), filesystem::handle_write_file),
            ToolEntry::new(filesystem::list_directory_definition(), filesystem::handle_list_directory),
            ToolEntry::new(filesystem::create_directory_definition(), filesystem::handle_create_directory),
            ToolEntry::new(filesystem::move_file_definition(), filesystem::handle_move_file),
            ToolEntry::new(filesystem::get_file_info_definition(), filesystem::handle_get_file_info),
            ToolEntry::new(filesystem::search_files_definition(), filesystem::handle_search_files),
            ToolEntry::new(filesystem::get_environment_info_definition(), filesystem::handle_get_environment_info),
            ToolEntry::new(edit::edit_block_definition(), edit::handle_edit_block),
            ToolEntry::new(command::start_process_definition(), command::handle_start_process),
            ToolEntry::new(command::read_process_output_definition(), command::handle_read_process_output),
            ToolEntry::new(command::interact_with_process_definition(), command::handle_interact_with_process),
            ToolEntry::new(command::force_terminate_definition(), command::handle_force_terminate),
            ToolEntry::new(llm::ask_llm_definition(), llm::handle_ask_llm),
            ToolEntry::new(browser::browser_action_definition(), browser::handle_browser_action),
            ToolEntry::new(mcp_client::call_mcp_server_definition(), mcp_client::handle_call_mcp_server),
            ToolEntry::new(research::deep_research_definition(), research::handle_deep_research),
        ]
    });
    &ENTRIES
}

pub fn get_all_tool_definitions() -> Vec<ToolDefinition> {
    let mut defs: Vec<ToolDefinition> = all_entries().iter().map(|e| e.definition.clone()).collect();
    defs.extend(crate::plugin::get_tool_definitions());
    defs
}

pub fn get_tool_handler(name: &str) -> Option<ToolHandler> {
    all_entries().iter().find(|e| e.definition.name == name).map(|e| e.handler)
}
