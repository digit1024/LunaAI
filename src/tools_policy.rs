//! Tools policy application: resolve profile → policy, filter MCP servers and tools by glob patterns.
//!
//! Pattern matching uses the `globset` crate (glob syntax: `*`, `?`, `**`).
//! Empty `enabled_mcp` => no MCP servers in scope; empty `enabled_tools` => no tools allowed.

use crate::config::{AppConfig, LlmProfile, ToolsPolicy};
use agentic_loop::mcp_servers_registry::MCPServerRegistry;
use anyhow::{Context, Result};
use std::collections::HashSet;
use tokio::sync::RwLock;

/// Internal tool names (not in MCP registry); filtered by policy like MCP tools.
pub const INTERNAL_TOOL_NAMES: &[&str] = &[
    "schedule_task",
    "cancel_scheduled_task",
    "store_memory",
    "search_memory",
    "search_memory_by_category",
    "delete_memory",
    "search_attachment_chunks",
];

/// Result of applying a tools policy: tool names to set on the registry (MCP only) and full allowed set (for internal tool filtering in loop_engine).
#[derive(Debug, Clone)]
pub struct AppliedToolsPolicy {
    /// MCP tool names to set as registry allow-list (tools_white_list).
    pub registry_tool_names: Vec<String>,
    /// All allowed tool names (MCP + internal); use when building available_tools to filter internal tools.
    pub allowed_tool_names: HashSet<String>,
}

/// Returns true if `name` matches any of the glob `patterns`. Uses globset; empty patterns = no match.
/// Logs a warning for invalid glob patterns.
fn name_matches_any(name: &str, patterns: &[String]) -> bool {
    if patterns.is_empty() {
        return false;
    }
    for pat in patterns {
        match globset::Glob::new(pat.as_str()) {
            Ok(glob) => {
                if glob.compile_matcher().is_match(name) {
                    return true;
                }
            }
            Err(e) => {
                tracing::warn!(
                    pattern = %pat,
                    error = %e,
                    "Invalid glob pattern in tools policy; pattern skipped"
                );
            }
        }
    }
    false
}

/// Pure policy computation: given registry guard and policy, returns (registry_tool_names, allowed_tool_names).
/// Shared by compute_allowed_tool_names and apply_tools_policy.
async fn compute_policy_result(
    guard: &MCPServerRegistry,
    policy: &ToolsPolicy,
) -> Result<(Vec<String>, HashSet<String>)> {
    let server_names: Vec<String> = guard.servers.keys().cloned().collect();
    let allowed_servers: Vec<String> = if policy.enabled_mcp.is_empty() {
        Vec::new()
    } else {
        server_names
            .into_iter()
            .filter(|name| name_matches_any(name, &policy.enabled_mcp))
            .collect()
    };

    let mut mcp_tool_names = Vec::new();
    for server_name in &allowed_servers {
        let tools = guard
            .get_all_tools_by_server_name(server_name)
            .await
            .context("get tools by server")?;
        for t in tools {
            mcp_tool_names.push(t.name);
        }
    }

    let mut candidates: HashSet<String> = mcp_tool_names.iter().cloned().collect();
    for &name in INTERNAL_TOOL_NAMES {
        candidates.insert(name.to_string());
    }

    let after_allow: HashSet<String> = if policy.enabled_tools.is_empty() {
        HashSet::new()
    } else {
        candidates
            .into_iter()
            .filter(|name| name_matches_any(name, &policy.enabled_tools))
            .collect()
    };

    let allowed_tool_names: HashSet<String> = after_allow
        .into_iter()
        .filter(|name| !name_matches_any(name, &policy.disabled_tools))
        .collect();

    let registry_tool_names: Vec<String> = mcp_tool_names
        .into_iter()
        .filter(|name| allowed_tool_names.contains(name))
        .collect();

    Ok((registry_tool_names, allowed_tool_names))
}

/// Compute allowed tool names for a profile without modifying the registry (e.g. for scheduled jobs that share the registry).
pub async fn compute_allowed_tool_names(
    registry: &RwLock<MCPServerRegistry>,
    app_config: &AppConfig,
    profile: &LlmProfile,
) -> Result<HashSet<String>> {
    let policy = app_config
        .tools_policies
        .get(&profile.tools_policy)
        .cloned()
        .unwrap_or_else(ToolsPolicy::default);
    let guard = registry.read().await;
    let (_, allowed_tool_names) = compute_policy_result(&*guard, &policy).await?;
    Ok(allowed_tool_names)
}

/// Apply the profile's tools_policy: filter MCP servers by enabled_mcp, then tools by enabled_tools and disabled_tools.
/// Registry must already be initialized (servers connected). Sets registry state to the allowed MCP tools only.
pub async fn apply_tools_policy(
    registry: &RwLock<MCPServerRegistry>,
    app_config: &AppConfig,
    profile: &LlmProfile,
) -> Result<AppliedToolsPolicy> {
    let policy = app_config
        .tools_policies
        .get(&profile.tools_policy)
        .cloned()
        .unwrap_or_else(ToolsPolicy::default);

    let (registry_tool_names, allowed_tool_names) = {
        let guard = registry.read().await;
        compute_policy_result(&*guard, &policy).await?
    };

    {
        let mut guard = registry.write().await;
        guard.disable_all_tools().await;
        for name in &registry_tool_names {
            guard.enable_tool(name).await;
        }
    }

    Ok(AppliedToolsPolicy {
        registry_tool_names,
        allowed_tool_names,
    })
}
