use cosmic::widget;
use cosmic::Element;

use crate::config::LlmProfile;
use super::super::page::SettingsMessage;

#[derive(Debug, Clone)]
pub struct ProfileCard {
    pub name: String,
    pub profile: LlmProfile,
    pub is_selected: bool,
    pub is_editing: bool,
    pub text_input_ids: ProfileTextInputIds,
}

#[derive(Debug, Clone)]
pub struct ProfileTextInputIds {
    pub name: cosmic::widget::Id,
    pub model: cosmic::widget::Id,
    pub endpoint: cosmic::widget::Id,
    pub api_key: cosmic::widget::Id,
}

impl Default for ProfileTextInputIds {
    fn default() -> Self {
        Self {
            name: cosmic::widget::Id::unique(),
            model: cosmic::widget::Id::unique(),
            endpoint: cosmic::widget::Id::unique(),
            api_key: cosmic::widget::Id::unique(),
        }
    }
}

impl ProfileCard {
    pub fn new(name: String, profile: LlmProfile, is_selected: bool) -> Self {
        Self {
            name,
            profile,
            is_selected,
            is_editing: false,
            text_input_ids: ProfileTextInputIds::default(),
        }
    }

    pub fn view(&self) -> Element<SettingsMessage> {
        if self.is_editing {
            self.edit_view()
        } else {
            self.display_view()
        }
    }

    fn display_view(&self) -> Element<SettingsMessage> {
        let status_text = if self.is_selected { "✓ Current" } else { "Click to select" };
        
        widget::settings::item::item_row(
            &self.name,
            widget::row()
                .push(
                    widget::column()
                        .push(widget::text(format!("Model: {}", self.profile.model)).size(12))
                        .push(widget::text(format!("Endpoint: {}", self.profile.endpoint)).size(12))
                        .push(widget::text(status_text).size(10))
                )
                .push(
                    widget::row()
                        .push(
                            widget::button::icon(cosmic::widget::icon::from_name("edit-symbolic"))
                                .on_press(SettingsMessage::EditProfile(self.name.clone()))
                        )
                        .push(
                            widget::button::icon(cosmic::widget::icon::from_name("user-trash-full-symbolic"))
                                .on_press(SettingsMessage::DeleteProfile(self.name.clone()))
                        )
                )
        ).into()
    }

    fn edit_view(&self) -> Element<SettingsMessage> {
        widget::settings::item::item_row(
            &self.name,
            widget::column()
                .push(
                    widget::text_input("Profile Name", &self.name)
                        .id(self.text_input_ids.name.clone())
                        .on_input(|name| SettingsMessage::UpdateNewProfile(
                            super::super::page::NewProfileField::Name,
                            name
                        ))
                )
                .push(
                    widget::text_input("Model", &self.profile.model)
                        .id(self.text_input_ids.model.clone())
                        .on_input(|model| SettingsMessage::UpdateNewProfile(
                            super::super::page::NewProfileField::Model,
                            model
                        ))
                )
                .push(
                    widget::text_input("Endpoint", &self.profile.endpoint)
                        .id(self.text_input_ids.endpoint.clone())
                        .on_input(|endpoint| SettingsMessage::UpdateNewProfile(
                            super::super::page::NewProfileField::Endpoint,
                            endpoint
                        ))
                )
                .push(
                    widget::text_input("API Key", &self.profile.api_key)
                        .id(self.text_input_ids.api_key.clone())
                        .on_input(|api_key| SettingsMessage::UpdateNewProfile(
                            super::super::page::NewProfileField::ApiKey,
                            api_key
                        ))
                )
                .push(
                    widget::row()
                        .push(
                            widget::button::suggested("Save")
                                .on_press(SettingsMessage::SaveProfile(
                                    self.name.clone(),
                                    self.profile.clone()
                                ))
                        )
                        .push(
                            widget::button::standard("Cancel")
                                .on_press(SettingsMessage::CancelEditProfile)
                        )
                )
        ).into()
    }
}
