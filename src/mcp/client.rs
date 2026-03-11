//! Connect to a single MCP server via stdio (TokioChildProcess).

use crate::config::MCPServerConfig;
use anyhow::{Context, Result};
use rmcp::model::ClientInfo;
use rmcp::service::{ServerSink, RoleClient};
use rmcp::transport::TokioChildProcess;
use rmcp::ServiceExt;
use tokio::process::Command;

/// Connected MCP server: tools, peer for rmcp_tools, and the client handle that must stay alive.
pub struct McpServerConnection {
    pub tools: Vec<rmcp::model::Tool>,
    pub peer: ServerSink,
    _client: rmcp::service::RunningService<RoleClient, ClientInfo>,
}

/// Connect to an MCP server. The returned connection must be kept alive for the peer to work.
pub async fn connect_mcp_server(
    server_name: &str,
    config: &MCPServerConfig,
) -> Result<McpServerConnection> {
    let mut cmd = Command::new(&config.command);
    cmd.args(&config.args);
    if !config.env.is_empty() {
        cmd.envs(&config.env);
    }

    let transport = TokioChildProcess::new(cmd)
        .with_context(|| format!("Failed to spawn MCP server '{}'", server_name))?;

    let client_info = ClientInfo::default();

    let client = client_info
        .serve(transport)
        .await
        .with_context(|| format!("Failed to connect to MCP server '{}'", server_name))?;

    let tools = client
        .list_tools(Default::default())
        .await
        .with_context(|| format!("Failed to list tools from MCP server '{}'", server_name))?
        .tools;

    let peer = client.peer().to_owned();

    Ok(McpServerConnection {
        tools,
        peer,
        _client: client,
    })
}
