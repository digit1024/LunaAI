//! MCP message handlers
//!
//! Handles MCP tool-related messages.

use cosmic::app;

use crate::ui::app::{CosmicLlmApp, Message};
use crate::ui::helpers::mcp_cache::update_mcp_cache;

pub fn handle_mcp_messages(app: &mut CosmicLlmApp, message: &Message) -> Option<app::Task<Message>> {
    match message {
        Message::MCPToolsUpdated(tools) => {
            // Update cache with provided tools
            app.mcp_cache.all_tools = tools.clone();
            app.mcp_cache.update_tool_states_from_enabled();
            Some(app::Task::none())
        }
        Message::RefreshMCPTools => {
            let mcp_registry = app.mcp_registry.clone();
            Some(app::Task::perform(
                async move {
                    let mut cache = crate::ui::state::MCPCache::new();
                    if let Err(e) = update_mcp_cache(&mcp_registry, &mut cache).await {
                        tracing::error!(error = %e, "Failed to update MCP cache");
                    }
                    cosmic::Action::App(Message::MCPCacheUpdated(cache))
                },
                |action| action,
            ))
        }
        Message::MCPCacheUpdated(cache) => {
            app.mcp_cache = cache.clone();
            Some(app::Task::none())
        }
        Message::ToggleAllTools(enabled) => {
            let enabled_val = *enabled;
            // Update cache optimistically
            for tool in &app.mcp_cache.all_tools {
                app.mcp_cache.tool_states.insert(tool.name.clone(), enabled_val);
            }
            let mcp_registry = app.mcp_registry.clone();
            Some(cosmic::Task::perform(
                async move {
                    let mut registry = mcp_registry.write().await;
                    if enabled_val {
                        registry.enable_all_tools().await;
                    } else {
                        registry.disable_all_tools().await;
                    }
                    cosmic::Action::App(Message::RefreshMCPTools)
                },
                |msg| msg,
            ))
        }
        Message::ToggleTool(tool_name, enabled) => {
            let tool_name_clone = tool_name.clone();
            let enabled_val = *enabled;
            // Update cache optimistically
            app.mcp_cache.tool_states.insert(tool_name_clone.clone(), enabled_val);
            let mcp_registry = app.mcp_registry.clone();
            Some(cosmic::Task::perform(
                async move {
                    let mut registry = mcp_registry.write().await;
                    if enabled_val {
                        registry.enable_tool(&tool_name_clone).await;
                    } else {
                        registry.disable_tool(&tool_name_clone).await;
                    }
                    cosmic::Action::App(Message::RefreshMCPTools)
                },
                |msg| msg,
            ))
        }
        Message::ToggleMCPServerEnabled(server_name, enabled) => {
            let profile_name = app.config.default.clone();
            let server_name_clone = server_name.clone();
            let enabled_val = *enabled;
            
            if let Some(profile) = app.config.profiles.get_mut(&profile_name) {
                if enabled_val {
                    if !profile.enabled_mcp.iter().any(|s| s.eq_ignore_ascii_case(&server_name_clone)) {
                        profile.enabled_mcp.push(server_name_clone.clone());
                    }
                } else {
                    profile.enabled_mcp.retain(|s| !s.eq_ignore_ascii_case(&server_name_clone));
                }
            }
            
            // Update cache optimistically - update all tools for this server
            if let Some(tools) = app.mcp_cache.tools_by_server.get(&server_name_clone) {
                for tool in tools {
                    app.mcp_cache.tool_states.insert(tool.name.clone(), enabled_val);
                }
            }
            
            let mcp_registry = app.mcp_registry.clone();
            let server_name_for_task = server_name_clone.clone();
            let config = app.config.clone();
            
            Some(cosmic::Task::perform(
                async move {
                    {
                        let mut registry = mcp_registry.write().await;
                        if enabled_val {
                            registry.enable_tools_by_server_name(&server_name_for_task).await;
                        } else {
                            registry.disable_tool_by_server_name(&server_name_for_task).await;
                        }
                    }
                    
                    if let Err(e) = config.save() {
                        tracing::error!(error = %e, "Failed to save config");
                    }
                    
                    cosmic::Action::App(Message::RefreshMCPTools)
                },
                |msg| msg,
            ))
        }
        Message::ShowToolsContext => {
            app.context_state.show_tools_context = true;
            app.core.window.show_context = true;
            Some(app::Task::none())
        }
        Message::HideToolsContext => {
            app.context_state.show_tools_context = false;
            app.core.window.show_context = false;
            Some(app::Task::none())
        }
        _ => None,
    }
}

