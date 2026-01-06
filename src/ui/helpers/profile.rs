//! Profile helper functions
//!
//! Helper functions for profile management.

use cosmic::app;

use crate::ui::app::{CosmicLlmApp, Message};

/// Load active profile prompt
pub fn load_active_profile_prompt(app: &mut CosmicLlmApp) -> Option<String> {
    let profile = app.config.get_default_profile()?;
    let path = profile.profile_prompt_file.as_deref()?;

    let resolved_path = crate::config::AppConfig::resolve_config_path(path);
    let resolved = resolved_path.to_string_lossy().to_string();

    match app.prompt_manager.load_profile_prompt(&resolved) {
        Ok(content) => {
            if app.chat_page
                .current_error
                .as_deref()
                .map(|msg| msg.starts_with("Profile prompt"))
                .unwrap_or(false)
            {
                app.chat_page.current_error = None;
            }
            Some(content)
        }
        Err(err) => {
            let message = match &err {
                crate::prompts::ProfilePromptError::NotFound(_) => {
                    format!("Profile prompt not found: {}", resolved)
                }
                _ => err.to_string(),
            };
            app.chat_page.current_error = Some(message);
            None
        }
    }
}

/// Get profile tool defaults task
pub fn profile_tool_defaults_task(app: &CosmicLlmApp) -> Option<app::Task<Message>> {
    let profile = app.config.get_default_profile()?;
    // Always apply profile defaults, even if enabled_mcp is empty
    // (empty list means enable all tools)
    let allowed_servers = profile.enabled_mcp.clone();
    let registry = app.mcp_registry.clone();

    Some(crate::services::MCPService::profile_tool_defaults_task(registry, allowed_servers))
}



