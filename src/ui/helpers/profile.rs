//! Profile helpers
//!
//! Helper functions for profile management, extracted from app.rs.

use crate::config::AppConfig;
use crate::prompts::{ProfilePromptError, PromptManager};
use crate::ui::pages::chat;

/// Load the active profile's prompt file
///
/// Returns the prompt content if successful, or None if there's an error.
/// Updates the chat page's error state if there's a problem.
pub fn load_active_profile_prompt(
    config: &AppConfig,
    prompt_manager: &PromptManager,
    chat_page: &mut chat::Page,
) -> Option<String> {
    let profile = config.get_default_profile()?;
    let path = profile.profile_prompt_file.as_deref()?;

    let resolved_path = AppConfig::resolve_config_path(path);
    let resolved = resolved_path.to_string_lossy().to_string();

    match prompt_manager.load_profile_prompt(&resolved) {
        Ok(content) => {
            if chat_page
                .current_error
                .as_deref()
                .map(|msg| msg.starts_with("Profile prompt"))
                .unwrap_or(false)
            {
                chat_page.current_error = None;
            }
            Some(content)
        }
        Err(err) => {
            let message = match &err {
                ProfilePromptError::NotFound(_) => {
                    format!("Profile prompt not found: {}", resolved)
                }
                _ => err.to_string(),
            };
            chat_page.current_error = Some(message);
            None
        }
    }
}

