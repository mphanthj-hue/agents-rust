use std::sync::Arc;
use rmcp::model::*;
use rmcp::service::RequestContext;
use rmcp::handler::server::ServerHandler;
use serde_json::Value;

use crate::tools;

#[derive(Clone)]
pub struct AgentsRustServer;

impl ServerHandler for AgentsRustServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .build()
        )
        .with_server_info(
            Implementation::new("agents-rust", "0.1.0")
        )
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<rmcp::service::RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        let defs = tools::get_all_tool_definitions();
        let tools_list: Vec<Tool> = defs.into_iter().map(|d| {
            let schema: Arc<JsonObject> = Arc::new(
                serde_json::from_value(d.input_schema).unwrap_or_default()
            );
            Tool::new(d.name, d.description, schema)
        }).collect();

        Ok(ListToolsResult::with_all_items(tools_list))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<rmcp::service::RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let name = &request.name;
        let args = match request.arguments {
            Some(map) => Value::Object(map),
            None => Value::Null,
        };

        let result = if let Some(handler) = tools::get_tool_handler(name) {
            handler(args).map_err(|e| ErrorData::new(ErrorCode::INTERNAL_ERROR, e, None))?
        } else if let Some(plugin_result) = crate::plugin::execute_tool(name, args.clone()) {
            plugin_result.map_err(|e| ErrorData::new(ErrorCode::INTERNAL_ERROR, e, None))?
        } else {
            return Err(ErrorData::new(
                ErrorCode::METHOD_NOT_FOUND,
                format!("Tool not found: {}", name),
                None,
            ));
        };

        let content: Vec<Content> = result.content.into_iter().map(|c| match c {
            crate::mcp::types::ToolContent::Text { text } => {
                Content::text(text)
            }
            crate::mcp::types::ToolContent::Resource { resource } => {
                Content::resource(
                    ResourceContents::text(resource.text, resource.uri)
                )
            }
        }).collect();

        if result.is_error.unwrap_or(false) {
            Ok(CallToolResult::error(content))
        } else {
            Ok(CallToolResult::success(content))
        }
    }
}
