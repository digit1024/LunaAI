//! MCP service
//!
//! Handles MCP-related operations for the server.

use agentic_loop::mcp_servers_registry::MCPServerRegistry;
use std::sync::Arc;
use tokio::sync::RwLock;

/// MCP service for tool management
pub struct MCPService;

impl MCPService {
    /// Apply profile tool defaults
    ///
    /// # Arguments
    /// - `registry`: MCP server registry
    /// - `allowed_servers`: List of server names to enable (empty = enable all)
    pub async fn apply_profile_tool_defaults(
        registry: Arc<RwLock<MCPServerRegistry>>,
        allowed_servers: Vec<String>,
    ) {
        let mut registry = registry.write().await;
        registry.enable_tools_for_multiple_servers(allowed_servers).await;
    }
}















