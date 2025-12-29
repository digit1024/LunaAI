// pub mod page;
// pub mod llm_profiles;
// pub mod mcp_config;
// pub mod app_preferences;
// pub mod components;
pub mod simple_settings;

pub use simple_settings::{SimpleSettingsMessage, SimpleSettingsPage};

use crate::ui::app::CosmicLlmApp;
use cosmic::Element;

/// View function that delegates to Page::view
pub fn settings_view(app: &CosmicLlmApp) -> Element<crate::ui::app::Message> {
    app.settings_page
        .view(&app.config)
        .map(|msg| crate::ui::app::Message::SettingsPage(msg))
}
