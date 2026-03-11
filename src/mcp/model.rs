//! Model types for MCP registry (server status, list_mcp_servers).

#[derive(Debug, Clone, PartialEq)]
pub enum ServerStatus {
    Connected,
    Failed(String),
}

#[derive(Debug, Clone)]
pub struct ServerWithStatus {
    pub server_name: String,
    pub server_status: ServerStatus,
}
