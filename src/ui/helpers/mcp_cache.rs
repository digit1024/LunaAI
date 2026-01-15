//! MCP Cache Helpers
//!
//! Helper functions to update MCP cache from async operations

use crate::mcp::conversions::tools_to_definitions;
use crate::ui::state::MCPCache;
use agentic_loop::mcp_servers_registry::MCPServerRegistry;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Update MCP cache from registry (async operation)
pub async fn update_mcp_cache(
    registry: &Arc<RwLock<MCPServerRegistry>>,
    cache: &mut MCPCache,
) -> Result<(), Box<dyn std::error::Error>> {
    let registry_guard = registry.read().await;
    
    // Get all tools
    let all_tools_sdk = registry_guard.get_all_tools().await
        .map_err(|e| format!("Failed to get all tools: {}", e))?;
    cache.all_tools = tools_to_definitions(&all_tools_sdk);
    
    // Get enabled tools
    let enabled_tools_sdk = registry_guard.get_enabled_tools().await
        .map_err(|e| format!("Failed to get enabled tools: {}", e))?;
    cache.enabled_tools = tools_to_definitions(&enabled_tools_sdk);
    
    // Get server statuses
    let server_statuses_vec = registry_guard.get_all_server_names_and_statuses().await
        .map_err(|e| format!("Failed to get server statuses: {}", e))?;
    
    // Convert to HashMap for easier access
    cache.server_statuses.clear();
    cache.all_server_names = server_statuses_vec.iter().map(|s| s.server_name.clone()).collect();
    cache.all_server_names.sort();
    
    for status_info in server_statuses_vec {
        cache.server_statuses.insert(status_info.server_name.clone(), status_info.server_status);
    }
    
    // Build tools by server
    cache.tools_by_server.clear();
    
    for server_name in &cache.all_server_names {
        if let Ok(server_tools_sdk) = registry_guard.get_all_tools_by_server_name(server_name).await {
            cache.tools_by_server.insert(
                server_name.clone(),
                tools_to_definitions(&server_tools_sdk),
            );
        }
    }
    
    // Update tool states from enabled tools
    cache.update_tool_states_from_enabled();
    
    Ok(())
}

