//! Settings message handlers
//!
//! Handles settings-related messages: OpenSettings, OpenConfigFile, etc.

use cosmic::app;

use crate::ui::app::{CosmicLlmApp, Message};

/// Handle settings-related messages
pub fn handle_settings_messages(
    app: &mut CosmicLlmApp,
    message: Message,
) -> Option<app::Task<Message>> {
    match message {
        Message::OpenSettings => {
            app.current_page = crate::ui::app::NavigationPage::Settings;
            None
        }
        Message::OpenConfigFile => {
            handle_open_config_file(app);
            None
        }
        Message::OpenProfilePrompt(profile_name) => {
            handle_open_profile_prompt(app, profile_name);
            None
        }
        Message::OpenMCPConfig => {
            handle_open_mcp_config(app);
            None
        }
        _ => None, // Not a settings message
    }
}

fn handle_open_config_file(_app: &mut CosmicLlmApp) {
    let config_path = crate::config::AppConfig::config_toml_path();
    let path_str = config_path.to_string_lossy().to_string();
    
    // Open in cosmic-edit
    let _ = std::process::Command::new("cosmic-edit")
        .arg(&path_str)
        .spawn();
}

fn handle_open_profile_prompt(app: &mut CosmicLlmApp, profile_name: String) {
    if let Some(profile) = app.config.get_profile(&profile_name) {
        if let Some(prompt_path) = profile.profile_prompt_file.as_ref() {
            let resolved = crate::config::AppConfig::resolve_config_path(prompt_path);
            let path_str = resolved.to_string_lossy().to_string();
            
            // Open in cosmic-edit
            let _ = std::process::Command::new("cosmic-edit")
                .arg(&path_str)
                .spawn();
        } else {
            app.chat_page.current_error = Some(format!(
                "Profile '{}' does not have a prompt file configured", profile_name
            ));
        }
    } else {
        app.chat_page.current_error = Some(format!(
            "Profile '{}' not found", profile_name
        ));
    }
}

fn handle_open_mcp_config(_app: &mut CosmicLlmApp) {
    let mcp_config_path = crate::config::MCPConfig::mcp_config_path();
    let path_str = mcp_config_path.to_string_lossy().to_string();
    
    // Open in cosmic-edit
    let _ = std::process::Command::new("cosmic-edit")
        .arg(&path_str)
        .spawn();
}

