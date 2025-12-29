//! MCP message handlers
//!
//! Handles MCP tool-related messages.

use cosmic::app;

use crate::ui::app::{CosmicLlmApp, Message};

pub fn handle_mcp_messages(app: &mut CosmicLlmApp, message: &Message) -> Option<app::Task<Message>> {
    match message {
        Message::MCPToolsUpdated(tools) => {
            app.available_mcp_tools = tools.clone();
            if let Ok(registry) = app.mcp_registry.try_read() {
                app.tool_states = registry.get_tool_states();
            }
            Some(app::Task::none())
        }
        Message::RefreshMCPTools => {
            if let Ok(registry) = app.mcp_registry.try_read() {
                let tools = registry.get_available_tools();
                tracing::debug!(tool_count = tools.len(), "RefreshMCPTools: Found tools");
                app.available_mcp_tools = tools;
                app.tool_states = registry.get_tool_states();
            } else {
                tracing::error!("RefreshMCPTools: Failed to get registry read lock");
            }
            Some(app::Task::none())
        }
        Message::ToggleAllTools(enabled) => {
            let enabled_val = *enabled;
            for tool in &app.available_mcp_tools {
                app.tool_states.insert(tool.name.clone(), enabled_val);
            }
            let mcp_registry = app.mcp_registry.clone();
            Some(cosmic::Task::perform(
                async move {
                    let mut registry = mcp_registry.write().await;
                    if enabled_val {
                        registry.enable_all_tools();
                    } else {
                        registry.disable_all_tools();
                    }
                    cosmic::Action::App(Message::RefreshMCPTools)
                },
                |msg| msg,
            ))
        }
        Message::ToggleTool(tool_name, enabled) => {
            let tool_name_clone = tool_name.clone();
            let enabled_val = *enabled;
            app.tool_states.insert(tool_name_clone.clone(), enabled_val);
            let mcp_registry = app.mcp_registry.clone();
            Some(cosmic::Task::perform(
                async move {
                    let mut registry = mcp_registry.write().await;
                    registry.set_tool_enabled(&tool_name_clone, enabled_val);
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
            
            if let Ok(registry) = app.mcp_registry.try_read() {
                for tool in &app.available_mcp_tools {
                    if let Ok(tool_server) = registry.get_server_for_tool(&tool.name) {
                        if tool_server == &server_name_clone {
                            app.tool_states.insert(tool.name.clone(), enabled_val);
                        }
                    }
                }
            }
            
            let mcp_registry = app.mcp_registry.clone();
            let server_name_for_task = server_name_clone.clone();
            let config = app.config.clone();
            
            Some(cosmic::Task::perform(
                async move {
                    {
                        let mut registry = mcp_registry.write().await;
                        registry.set_server_enabled(&server_name_for_task, enabled_val);
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

