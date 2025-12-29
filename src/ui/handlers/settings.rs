//! Settings message handlers
//!
//! Handles settings-related messages: OpenSettings, OpenConfigFile, etc.

use cosmic::app;

use crate::ui::app::{CosmicLlmApp, Message, NavigationPage};
use crate::ui::dialogs::DialogPage;
use crate::ui::pages::settings::SimpleSettingsMessage;

/// Handle settings-related messages
pub fn handle_settings_messages(
    app: &mut CosmicLlmApp,
    message: Message,
) -> Option<app::Task<Message>> {
    match message {
        Message::OpenSettings => {
            app.current_page = NavigationPage::Settings;
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
        Message::ChangeDefaultProfile(profile_index) => {
            handle_change_default_profile(app, profile_index)
        }
        Message::SaveSettings => {
            handle_save_settings(app);
            None
        }
        Message::ResetSettings => {
            handle_reset_settings(app);
            None
        }
        Message::SettingsPage(msg) => {
            handle_settings_page(app, msg)
        }
        _ => None, // Not a settings message
    }
}

fn handle_change_default_profile(app: &mut CosmicLlmApp, profile_index: usize) -> Option<app::Task<Message>> {
    // Must sort the same way as in the view to maintain index consistency
    // Filter out hidden profiles
    let mut profile_names: Vec<String> = app.config.profiles
        .iter()
        .filter(|(_, p)| !p.hidden)
        .map(|(name, _)| name.clone())
        .collect();
    profile_names.sort();
    if let Some(profile_name) = profile_names.get(profile_index) {
        let new_profile = profile_name.clone();
        app.config.default = new_profile.clone();
        app.settings_changed = true;
        // Recreate LLM client for new default provider
        if let Some(profile) = app.config.get_default_profile().cloned() {
            let masked = if profile.api_key.len() > 6 {
                format!(
                    "{}...{}",
                    &profile.api_key[..3],
                    &profile.api_key[profile.api_key.len().saturating_sub(3)..]
                )
            } else {
                "***".to_string()
            };
            tracing::debug!(
                profile_name = %app.config.default,
                model = %profile.model,
                endpoint = %profile.endpoint,
                api_key_masked = %masked,
                "Switching default profile"
            );
            app.llm_client = crate::llm::build_llm_client(&profile);
            
            // Update active conversation's profile in database if there is one
            if let Some(conv_id) = app.conversation_state.current_conversation_id {
                if let Err(e) = app.storage.update_conversation_profile(&conv_id, Some(&new_profile)) {
                    tracing::error!(error = %e, "Failed to update conversation profile");
                } else {
                    tracing::debug!(
                        conversation_id = %conv_id,
                        profile_name = %new_profile,
                        "Updated conversation profile"
                    );
                }
            }
        }
        crate::ui::helpers::profile::profile_tool_defaults_task(app)
    } else {
        None
    }
}

fn handle_save_settings(app: &mut CosmicLlmApp) {
    if let Err(e) = app.config.save() {
        tracing::error!(error = %e, "Failed to save settings");
    } else {
        app.settings_changed = false;
        tracing::debug!("Settings saved successfully");
    }
}

fn handle_reset_settings(app: &mut CosmicLlmApp) {
    app.config = crate::config::AppConfig::default();
    app.settings_changed = true;
}

fn handle_settings_page(app: &mut CosmicLlmApp, msg: SimpleSettingsMessage) -> Option<app::Task<Message>> {
    // Handle app-level messages before delegating to page
    match &msg {
        SimpleSettingsMessage::BackToMain => {
            app.current_page = NavigationPage::Chat;
            Some(app::Task::none())
        }
        SimpleSettingsMessage::OpenConfigFile => {
            Some(cosmic::Task::perform(
                async {},
                |_| cosmic::Action::App(Message::OpenConfigFile),
            ))
        }
        SimpleSettingsMessage::OpenProfilePrompt(profile_name) => {
            let profile_name_for_task = profile_name.clone();
            Some(cosmic::Task::perform(
                async { profile_name_for_task },
                |profile_name_for_task| cosmic::Action::App(Message::OpenProfilePrompt(profile_name_for_task)),
            ))
        }
        SimpleSettingsMessage::SaveConfig => {
            // Apply all staged changes to actual config
            app.config.profiles = app.settings_page.staged_profiles.clone();
            app.config.default = app.settings_page.staged_default.clone();
            app.config.server = app.settings_page.staged_server.clone();
            app.config.title_summary = app.settings_page.staged_title_summary.clone();
            
            // Save to file
            if let Err(e) = app.config.save() {
                tracing::error!(error = %e, "Failed to save settings");
                app.dialog = Some(DialogPage::message_text(format!(
                    "Failed to save settings:\n{}",
                    e
                )));
            } else {
                app.settings_page.has_changes = false;
                app.settings_changed = false;
                // Update LLM client if default profile changed
                let profile_changed = app.config.default.clone();
                if let Some(profile) = app.config.get_default_profile().cloned() {
                    app.llm_client = crate::llm::build_llm_client(&profile);
                }
                // Update active conversation's profile in database if there is one
                if let Some(conv_id) = app.conversation_state.current_conversation_id {
                    if let Err(e) = app.storage.update_conversation_profile(&conv_id, Some(&profile_changed)) {
                        tracing::error!(error = %e, "Failed to update conversation profile");
                    } else {
                        tracing::debug!(
                            conversation_id = %conv_id,
                            profile_name = %profile_changed,
                            "Updated conversation profile"
                        );
                    }
                }
                if let Some(task) = app.profile_tool_defaults_task() {
                    return Some(task);
                }
            }
            Some(app::Task::none())
        }
        SimpleSettingsMessage::CancelConfig => {
            // Reload from config to discard all staged changes
            app.settings_page.load_from_config(&app.config);
            app.current_page = NavigationPage::Chat;
            Some(app::Task::none())
        }
        _ => {
            // Delegate to page module for page state updates
            let _task = app.settings_page.update(msg.clone(), &app.config);
            None
        }
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

