use cosmic::widget;
use cosmic::Element;

use super::page::{SettingsMessage, AppPreferences};

pub struct AppPreferencesSection;

impl AppPreferencesSection {
    pub fn new() -> Self {
        Self
    }

    pub fn view(&self, preferences: &AppPreferences) -> Vec<Element<SettingsMessage>> {
        let mut items = Vec::new();

        // Theme selection
        items.push(
            widget::settings::item::item(
                "Theme",
                widget::dropdown(
                    &["System", "Dark", "Light"],
                    Some(preferences.theme),
                    SettingsMessage::ChangeTheme,
                )
            )
        );

        // Auto-save toggle
        items.push(
            widget::settings::item::item(
                "Auto-save conversations",
                widget::checkbox("Enable auto-save", preferences.auto_save)
                    .on_toggle(SettingsMessage::ToggleAutoSave)
            )
        );

        // Notifications toggle
        items.push(
            widget::settings::item::item(
                "Notifications",
                widget::checkbox("Enable notifications", preferences.notifications)
                    .on_toggle(SettingsMessage::ToggleNotifications)
            )
        );

        // Auto-scroll toggle
        items.push(
            widget::settings::item::item(
                "Auto-scroll to bottom",
                widget::checkbox("Auto-scroll during streaming", preferences.auto_scroll)
                    .on_toggle(SettingsMessage::ToggleAutoScroll)
            )
        );

        items
    }

    pub fn get_theme_name(&self, theme_index: usize) -> &'static str {
        match theme_index {
            0 => "System",
            1 => "Dark", 
            2 => "Light",
            _ => "System",
        }
    }

    pub fn validate_preferences(&self, preferences: &AppPreferences) -> Vec<String> {
        let mut errors = Vec::new();
        
        if preferences.theme > 2 {
            errors.push("Invalid theme selection".to_string());
        }

        errors
    }
}
