//! MCP server registry: connect at startup, filter tools by policy per turn.

use crate::config::MCPConfig;
use crate::mcp::client::{connect_mcp_server, McpServerConnection};
use crate::mcp::model::{ServerStatus, ServerWithStatus};
use anyhow::Result;
use rmcp::model::Tool;
use rmcp::service::ServerSink;
use std::collections::{HashMap, HashSet};
use std::time::Duration;

/// Timeout for MCP server startup (spawn + connect + list_tools). Prevents app hang if a server blocks.
const MCP_STARTUP_TIMEOUT_SECS: u64 = 30;

/// Entry for a connected MCP server.
struct McpServerEntry {
    connection: McpServerConnection,
}

/// Registry of MCP servers. Clients are built at startup via initialize_from_config.
pub struct McpRegistry {
    servers: HashMap<String, McpServerEntry>,
    failed_servers: HashMap<String, String>,
}

impl McpRegistry {
    pub fn new() -> Self {
        Self {
            servers: HashMap::new(),
            failed_servers: HashMap::new(),
        }
    }

    /// Initialize from config: spawn rmcp client per server, connect, discover tools.
    /// Each server has a startup timeout; hung servers are skipped and logged.
    pub async fn initialize_from_config(&mut self, config: &MCPConfig) -> Result<()> {
        let timeout = Duration::from_secs(MCP_STARTUP_TIMEOUT_SECS);
        for (server_name, server_config) in &config.servers {
            match tokio::time::timeout(
                timeout,
                connect_mcp_server(server_name, server_config),
            )
            .await
            {
                Ok(Ok(connection)) => {
                    self.servers.insert(
                        server_name.clone(),
                        McpServerEntry { connection },
                    );
                    tracing::info!(server = %server_name, "Connected MCP server");
                }
                Ok(Err(e)) => {
                    let err_msg = e.to_string();
                    tracing::error!(server = %server_name, error = %err_msg, "Failed to connect to MCP server");
                    self.failed_servers.insert(server_name.clone(), err_msg);
                }
                Err(_) => {
                    let err_msg = format!(
                        "Startup timeout after {}s (spawn/connect/list_tools)",
                        MCP_STARTUP_TIMEOUT_SECS
                    );
                    tracing::error!(server = %server_name, timeout_secs = MCP_STARTUP_TIMEOUT_SECS, "MCP server startup timed out");
                    self.failed_servers.insert(server_name.clone(), err_msg);
                }
            }
        }
        Ok(())
    }

    /// Get (tools, peer) per server, filtered by allowed_tool_names.
    pub fn get_mcp_servers_for_turn(
        &self,
        allowed_tool_names: &HashSet<String>,
    ) -> Vec<(Vec<Tool>, ServerSink)> {
        let mut result = Vec::new();
        for entry in self.servers.values() {
            let filtered: Vec<Tool> = entry
                .connection
                .tools
                .iter()
                .filter(|t| allowed_tool_names.contains(&t.name.to_string()))
                .cloned()
                .collect();
            if !filtered.is_empty() {
                result.push((filtered, entry.connection.peer.clone()));
            }
        }
        result
    }

    /// Get all tools from all servers (for tools policy computation).
    #[allow(dead_code)]
    pub fn get_all_tools(&self) -> Vec<Tool> {
        self.servers
            .values()
            .flat_map(|e| e.connection.tools.clone())
            .collect()
    }

    /// Get all tools from a specific server.
    pub fn get_all_tools_by_server_name(&self, server_name: &str) -> Vec<Tool> {
        self.servers
            .get(server_name)
            .map(|e| e.connection.tools.clone())
            .unwrap_or_default()
    }

    /// Get connected server names (for tools policy).
    pub fn get_server_names(&self) -> Vec<String> {
        self.servers.keys().cloned().collect()
    }

    /// Get server names and statuses (for list_mcp_servers).
    pub fn get_all_server_names_and_statuses(&self) -> Vec<ServerWithStatus> {
        let mut result: Vec<ServerWithStatus> = self
            .servers
            .keys()
            .map(|name| ServerWithStatus {
                server_name: name.clone(),
                server_status: ServerStatus::Connected,
            })
            .collect();
        for (name, error) in &self.failed_servers {
            result.push(ServerWithStatus {
                server_name: name.clone(),
                server_status: ServerStatus::Failed(error.clone()),
            });
        }
        result
    }
}

impl Default for McpRegistry {
    fn default() -> Self {
        Self::new()
    }
}
