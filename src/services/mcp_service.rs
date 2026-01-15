//! MCP service
//!
//! Handles MCP-related operations like applying profile tool defaults.

use agentic_loop::mcp_servers_registry::MCPServerRegistry;
use std::sync::Arc;
use tokio::sync::RwLock;

/// MCP service for tool management
pub struct MCPService;

impl MCPService {
    /// Create a task to apply profile tool defaults
    ///
    /// This extracts the logic from `app.rs::profile_tool_defaults_task()` to
    /// centralize MCP-related operations.
    ///
    /// # Arguments
    /// - `registry`: MCP server registry
    /// - `allowed_servers`: List of server names to enable (empty = enable all)
    ///
    /// # Returns
    /// A task that applies the defaults and returns a refresh message
    pub fn profile_tool_defaults_task(
        registry: Arc<RwLock<MCPServerRegistry>>,
        allowed_servers: Vec<String>,
    ) -> cosmic::app::Task<crate::ui::app::Message> {
        cosmic::Task::perform(
            async move {
                let mut registry = registry.write().await;
                registry.enable_tools_for_multiple_servers(allowed_servers).await;
                cosmic::Action::App(crate::ui::app::Message::RefreshMCPTools)
            },
            |msg| msg,
        )
    }
}















