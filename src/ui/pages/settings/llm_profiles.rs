use cosmic::widget;
use cosmic::Element;
use std::collections::HashMap;

use crate::config::{AppConfig, LlmProfile};
use super::page::{SettingsMessage, NewProfileState, NewProfileField};

pub struct LlmProfilesSection;

impl LlmProfilesSection {
    pub fn new() -> Self {
        Self
    }

    pub fn view(
        &self,
        config: &AppConfig,
        selected_profile: &str,
        new_profile: &NewProfileState,
        text_input_ids: &super::page::SettingsTextInputIds,
    ) -> Vec<Element<SettingsMessage>> {
        let mut items = Vec::new();

        // Profile selection dropdown
        items.push(
            widget::settings::item::item(
                "Default Profile",
                widget::dropdown(
                    &config.profiles.keys().cloned().collect::<Vec<_>>(),
                    Some(selected_profile.to_string()),
                    SettingsMessage::SelectProfile,
                )
            )
        );

        // Add new profile button
        items.push(
            widget::settings::item::item(
                "Add New Profile",
                widget::button::suggested("Add Profile")
                    .on_press(SettingsMessage::AddNewProfile)
            )
        );

        // New profile form (if adding)
        if !new_profile.name.is_empty() || !new_profile.model.is_empty() || 
           !new_profile.endpoint.is_empty() || !new_profile.api_key.is_empty() {
            items.push(self.new_profile_form(new_profile, text_input_ids));
        }

        // Profile list
        if !config.profiles.is_empty() {
            items.push(
                widget::settings::item::item(
                    "Configured Profiles",
                    self.profile_list(config, selected_profile)
                )
            );
        }

        items
    }

    fn new_profile_form(
        &self,
        new_profile: &NewProfileState,
        text_input_ids: &super::page::SettingsTextInputIds,
    ) -> Element<SettingsMessage> {
        let backends = vec![
            "openai".to_string(),
            "anthropic".to_string(),
            "deepseek".to_string(),
            "ollama".to_string(),
            "gemini".to_string(),
        ];
        
        let selected_backend_idx = backends.iter().position(|b| b == &new_profile.backend);
        
        widget::settings::item::item(
            "New Profile",
            widget::column()
                .push(
                    widget::text_input("Profile Name", &new_profile.name)
                        .id(text_input_ids.profile_name.clone())
                        .on_input(|name| SettingsMessage::UpdateNewProfile(
                            NewProfileField::Name,
                            name
                        ))
                )
                .push(
                    widget::row()
                        .push(widget::text("Backend: "))
                        .push(
                            widget::dropdown(
                                &backends,
                                selected_backend_idx,
                                |idx| {
                                    let backend = backends.get(idx).cloned().unwrap_or_else(|| "openai".to_string());
                                    SettingsMessage::UpdateNewProfile(NewProfileField::Backend, backend)
                                }
                            )
                        )
                )
                .push(
                    widget::text_input("Model", &new_profile.model)
                        .id(text_input_ids.profile_model.clone())
                        .on_input(|model| SettingsMessage::UpdateNewProfile(
                            NewProfileField::Model,
                            model
                        ))
                )
                .push(
                    widget::text_input("Endpoint", &new_profile.endpoint)
                        .id(text_input_ids.profile_endpoint.clone())
                        .on_input(|endpoint| SettingsMessage::UpdateNewProfile(
                            NewProfileField::Endpoint,
                            endpoint
                        ))
                )
                .push(
                    widget::text_input("API Key", &new_profile.api_key)
                        .id(text_input_ids.profile_api_key.clone())
                        .on_input(|api_key| SettingsMessage::UpdateNewProfile(
                            NewProfileField::ApiKey,
                            api_key
                        ))
                )
                .push(
                    widget::row()
                        .push(
                            widget::button::suggested("Save Profile")
                                .on_press(SettingsMessage::SaveNewProfile)
                        )
                        .push(
                            widget::button::standard("Cancel")
                                .on_press(SettingsMessage::CancelEditProfile)
                        )
                )
        )
    }

    fn profile_list(&self, config: &AppConfig, selected_profile: &str) -> Element<SettingsMessage> {
        let mut profile_widgets = Vec::new();
        
        for (name, profile) in &config.profiles {
            let is_selected = name == selected_profile;
            let status_text = if is_selected { "✓ Current" } else { "Click to select" };
            
            profile_widgets.push(
                widget::container(
                    widget::column()
                        .push(
                            widget::row()
                                .push(
                                    widget::column()
                                        .push(widget::text(name).size(14))
                                        .push(widget::text(format!("Backend: {}", profile.backend)).size(12))
                                        .push(widget::text(format!("Model: {}", profile.model)).size(12))
                                        .push(widget::text(format!("Endpoint: {}", profile.endpoint)).size(12))
                                        .push(widget::text(status_text).size(10))
                                )
                                .push(
                                    widget::row()
                                        .push(
                                            widget::button::icon(cosmic::widget::icon::from_name("edit-symbolic"))
                                                .on_press(SettingsMessage::EditProfile(name.clone()))
                                        )
                                        .push(
                                            widget::button::icon(cosmic::widget::icon::from_name("user-trash-full-symbolic"))
                                                .on_press(SettingsMessage::DeleteProfile(name.clone()))
                                        )
                                )
                        )
                )
                .padding(12)
                .class(cosmic::style::Container::Card)
                .into()
            );
        }

        widget::column().push(widget::Space::with_height(8.0))
            .push(widget::column::with_children(profile_widgets))
            .into()
    }

    pub fn validate_profile(&self, profile: &NewProfileState) -> Vec<String> {
        let mut errors = Vec::new();
        
        if profile.name.is_empty() {
            errors.push("Profile name is required".to_string());
        }
        
        if profile.model.is_empty() {
            errors.push("Model is required".to_string());
        }
        
        if profile.endpoint.is_empty() {
            errors.push("Endpoint is required".to_string());
        }
        
        if profile.api_key.is_empty() {
            errors.push("API key is required".to_string());
        }

        // Validate endpoint format
        if !profile.endpoint.starts_with("http://") && !profile.endpoint.starts_with("https://") {
            errors.push("Endpoint must start with http:// or https://".to_string());
        }

        errors
    }
}
