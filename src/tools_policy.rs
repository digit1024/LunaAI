//! Tools policy application: resolve profile → policy, filter MCP servers and tools by glob patterns.
//!
//! Pattern matching uses the `globset` crate (glob syntax: `*`, `?`, `**`).
//! Empty `enabled_mcp` => no MCP servers in scope; empty `enabled_tools` => no tools allowed.
//!
//! Policy is **pure**: it never mutates the shared `MCPServerRegistry`. The resulting
//! `allowed_tool_names` set is the single source of truth for filtering tools per run
//! (see `RunContext.allowed_tool_names` and `loop_engine::build_available_tools`).
//! Multiple sessions/profiles can therefore coexist without trampling each other.

use crate::config::{AppConfig, LlmProfile, ToolsPolicy};
use agentic_loop::mcp_servers_registry::MCPServerRegistry;
use anyhow::{Context, Result};
use std::collections::HashSet;
use tokio::sync::RwLock;

/// Internal tool names (not in MCP registry); filtered by policy like MCP tools.
pub const INTERNAL_TOOL_NAMES: &[&str] = &[
    "schedule_task",
    "cancel_scheduled_task",
    "list_scheduled_tasks",
    "publish_image",
    "store_memory",
    "search_memory",
    "search_memory_by_category",
    "delete_memory",
    "search_attachment_chunks",
    "vision_analyze",
    "search_history",
    "list_conversations",
    "get_conversation_messages",
    "spawn_worker",
];

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

/// Pure policy computation: given registry guard and policy, returns the full allowed tool name set
/// (MCP tools + internal tools + enabled embedded tools, after enabled_mcp / enabled_tools /
/// disabled_tools filtering).
async fn compute_policy_result(
    guard: &MCPServerRegistry,
    policy: &ToolsPolicy,
    embedded_tool_names: &[String],
) -> Result<HashSet<String>> {
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

    let mut candidates: HashSet<String> = mcp_tool_names.into_iter().collect();
    for &name in INTERNAL_TOOL_NAMES {
        candidates.insert(name.to_string());
    }
    // Embedded tools (compiled-in, formerly MCP servers) behave like internal tools:
    // they are candidates whenever enabled, gated only by enabled_tools/disabled_tools.
    for name in embedded_tool_names {
        candidates.insert(name.clone());
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

    Ok(allowed_tool_names)
}

/// Compute the allow-set of tool names for a profile. Pure — does **not** mutate the registry.
///
/// This is the single source of truth for "what can this run call?". The resulting set is
/// snapshotted into `RunContext.allowed_tool_names` and used by `loop_engine` to filter
/// both MCP tools and internal tools at message-build and tool-execution time.
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
    let embedded_names = crate::tools::enabled_tool_names(&app_config.embedded_tools);
    let guard = registry.read().await;
    compute_policy_result(&guard, &policy, &embedded_names).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{EmbeddedToolsConfig, ShellToolConfig, TimeToolConfig};

    fn profile_with_policy(policy: &str) -> LlmProfile {
        LlmProfile {
            tools_policy: policy.to_string(),
            ..LlmProfile::default()
        }
    }

    #[tokio::test]
    async fn embedded_names_are_candidates_when_enabled() {
        let registry = RwLock::new(MCPServerRegistry::new());
        let mut app_config = AppConfig::default();
        app_config.embedded_tools = EmbeddedToolsConfig {
            shell: ShellToolConfig {
                enabled: true,
                ..ShellToolConfig::default()
            },
            time: TimeToolConfig {
                enabled: true,
                ..TimeToolConfig::default()
            },
        };
        app_config.tools_policies.insert(
            "p".to_string(),
            ToolsPolicy {
                enabled_mcp: Vec::new(),
                enabled_tools: vec!["*".to_string()],
                disabled_tools: Vec::new(),
            },
        );

        let allowed = compute_allowed_tool_names(&registry, &app_config, &profile_with_policy("p"))
            .await
            .unwrap();

        assert!(allowed.contains("shell_execute"));
        assert!(allowed.contains("get_current_time"));
        assert!(allowed.contains("convert_time"));
    }

    #[tokio::test]
    async fn disabled_embedded_tools_are_not_candidates() {
        let registry = RwLock::new(MCPServerRegistry::new());
        let mut app_config = AppConfig::default();
        app_config.embedded_tools = EmbeddedToolsConfig {
            shell: ShellToolConfig {
                enabled: false,
                ..ShellToolConfig::default()
            },
            time: TimeToolConfig {
                enabled: false,
                ..TimeToolConfig::default()
            },
        };
        app_config.tools_policies.insert(
            "p".to_string(),
            ToolsPolicy {
                enabled_mcp: Vec::new(),
                enabled_tools: vec!["*".to_string()],
                disabled_tools: Vec::new(),
            },
        );

        let allowed = compute_allowed_tool_names(&registry, &app_config, &profile_with_policy("p"))
            .await
            .unwrap();
        assert!(!allowed.contains("shell_execute"));
    }

    #[tokio::test]
    async fn disabled_tools_glob_removes_embedded() {
        let registry = RwLock::new(MCPServerRegistry::new());
        let mut app_config = AppConfig::default();
        app_config.embedded_tools = EmbeddedToolsConfig {
            shell: ShellToolConfig::default(),
            time: TimeToolConfig::default(),
        };
        app_config.tools_policies.insert(
            "p".to_string(),
            ToolsPolicy {
                enabled_mcp: Vec::new(),
                enabled_tools: vec!["*".to_string()],
                disabled_tools: vec!["get_*".to_string()],
            },
        );

        let allowed = compute_allowed_tool_names(&registry, &app_config, &profile_with_policy("p"))
            .await
            .unwrap();
        assert!(allowed.contains("shell_execute"));
        assert!(!allowed.contains("get_current_time"));
        assert!(allowed.contains("convert_time"));
    }
}
