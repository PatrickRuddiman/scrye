//! The MCP server handler: one `#[tool_router]` impl aggregating the six
//! read-only tool wrappers (each delegates to a free function in
//! [`crate::tools::read`] and serializes the returned DTO to JSON content)
//! plus the `#[tool_handler] impl ServerHandler` that advertises the tool
//! capability. There is no write/control surface — fetch + index + reconcile
//! run automatically inside the daemon.

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{
    CallToolResult, Content, Implementation, ServerCapabilities, ServerInfo,
};
use rmcp::{tool, tool_handler, tool_router, ErrorData as McpError, ServerHandler};
use serde::Serialize;

use crate::dto::{IdArg, SearchArgs};
use crate::state::McpState;
use crate::tools::read;

/// Serialize a tool DTO into a successful MCP result with one JSON text block.
fn ok_json<T: Serialize>(dto: &T) -> Result<CallToolResult, McpError> {
    let text = serde_json::to_string(dto).map_err(|e| McpError::internal_error(e.to_string(), None))?;
    Ok(CallToolResult::success(vec![Content::text(text)]))
}

/// The scoped MCP server. Cloned once per session by the transport's service
/// factory; every clone shares the same [`McpState`].
#[derive(Clone)]
pub struct McpServer {
    state: McpState,
    tool_router: ToolRouter<McpServer>,
}

#[tool_router]
impl McpServer {
    pub fn new(state: McpState) -> Self {
        Self {
            state,
            tool_router: Self::tool_router(),
        }
    }

    #[tool(description = "Search the mailbox; ranked hits scoped to your accounts.")]
    async fn search(
        &self,
        Parameters(args): Parameters<SearchArgs>,
    ) -> Result<CallToolResult, McpError> {
        ok_json(&read::search(&self.state, args).await?)
    }

    #[tool(description = "Fetch a single parsed message by id.")]
    async fn get_message(
        &self,
        Parameters(args): Parameters<IdArg>,
    ) -> Result<CallToolResult, McpError> {
        ok_json(&read::get_message(&self.state, args).await?)
    }

    #[tool(description = "Fetch the raw RFC 5322 source of a message by id.")]
    async fn get_raw_message(
        &self,
        Parameters(args): Parameters<IdArg>,
    ) -> Result<CallToolResult, McpError> {
        ok_json(&read::get_raw_message(&self.state, args).await?)
    }

    #[tool(description = "Fetch every message in a thread, oldest first.")]
    async fn get_thread(
        &self,
        Parameters(args): Parameters<IdArg>,
    ) -> Result<CallToolResult, McpError> {
        ok_json(&read::get_thread(&self.state, args).await?)
    }

    #[tool(description = "List your configured accounts.")]
    async fn list_accounts(&self) -> Result<CallToolResult, McpError> {
        ok_json(&read::list_accounts(&self.state).await?)
    }

    #[tool(description = "Daemon health: uptime, per-account sync state, drainer + crash-loop status.")]
    async fn status(&self) -> Result<CallToolResult, McpError> {
        ok_json(&read::status(&self.state).await?)
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for McpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::from_build_env())
            .with_instructions(
                "scryd MCP server. All responses are scoped to the USER_EMAIL account(s). \
                 Read-only tools: search, get_message, get_raw_message, get_thread, \
                 list_accounts, status. Fetching and indexing run automatically inside \
                 the daemon; there is no write or control surface."
                    .to_string(),
            )
    }
}
