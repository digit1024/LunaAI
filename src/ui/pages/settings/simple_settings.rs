use cosmic::{
    iced::{Length, Alignment},
    widget::{self, container, row, column, text, button, text_input},
    Element, theme,
};

use crate::config::{AppConfig, LlmProfile};

#[derive(Debug, Clone)]
pub struct SimpleSettingsPage {
    pub new_profile_name: String,
    pub new_profile_model: String,
    pub new_profile_endpoint: String,
}

#[derive(Debug, Clone)]
pub enum SimpleSettingsMessage {
    BackToMain,
    SetDefaultProfile(String),
    NewProfileNameChanged(String),
    NewProfileModelChanged(String),
    NewProfileEndpointChanged(String),
    AddNewProfile,
}

impl SimpleSettingsPage {
    pub fn new() -> Self {
        Self {
            new_profile_name: String::new(),
            new_profile_model: String::new(),
            new_profile_endpoint: String::new(),
        }
    }

    pub fn view<'a>(&'a self, config: &'a AppConfig) -> Element<'a, SimpleSettingsMessage> {
        let mut content = column().spacing(16);

        // Create profile dropdown - use a simple text display for now
        let current_profile_text = format!("Currently using: {}", config.default);
        let profile_dropdown = text(current_profile_text)
            .size(12)
            .class(cosmic::style::Text::Color(
                theme::active().cosmic().palette.neutral_6.into()
            ))
            .width(Length::Fill);

        // Default LLM Profile Section with dropdown
        content = content.push(
            container(
                column()
                    .push(
                        text("Default LLM Profile")
                            .size(16)
                            .class(cosmic::style::Text::Color(
                                theme::active().cosmic().palette.neutral_9.into()
                            ))
                    )
                    .push(profile_dropdown)
                    .spacing(12)
            )
            .padding(16)
        );

        // Profile Cards
        for (profile_name, profile) in &config.profiles {
            content = content.push(
                self.profile_card(profile_name, profile, &config.default)
            );
        }

        // Add New Profile Section
        content = content.push(
            self.add_profile_section()
        );


        // Back button
        content = content.push(
            row()
                .push(widget::Space::with_width(Length::Fill))
                .push(button::standard("Back to Chat")
                    .on_press(SimpleSettingsMessage::BackToMain))
        );

        widget::scrollable(content)
            .into()
    }

    fn profile_card<'a>(&self, profile_name: &'a str, profile: &'a LlmProfile, default_profile: &'a str) -> Element<'a, SimpleSettingsMessage> {
        let is_current = profile_name == default_profile;
        
        // Prepare the status widget (either label or button) as a unified Element
        let status_widget: Element<'a, SimpleSettingsMessage> = if is_current {
            text("✓ Current")
                .size(12)
                .class(cosmic::style::Text::Color(
                    theme::active().cosmic().accent_color().into()
                ))
                .into()
        } else {
            button::standard("Set as default")
                .on_press(SimpleSettingsMessage::SetDefaultProfile(profile_name.to_string()))
                .into()
        };

        container(
            row()
                .push(
                    column()
                        .push(
                            row()
                                .push(text(profile_name)
                                    .size(14)
                                    .class(cosmic::style::Text::Color(
                                        theme::active().cosmic().palette.neutral_9.into()
                                    )))
                                .push(widget::Space::with_width(Length::Fill))
                                .push(status_widget)
                        )
                        .push(text(format!("Model: {}", profile.model))
                            .size(12)
                            .class(cosmic::style::Text::Color(
                                theme::active().cosmic().palette.neutral_6.into()
                            )))
                        .push(text(format!("Endpoint: {}", profile.endpoint))
                            .size(12)
                            .class(cosmic::style::Text::Color(
                                theme::active().cosmic().palette.neutral_6.into()
                            )))
                        .spacing(4)
                        .align_x(Alignment::Start)
                        .width(Length::Fill)
                )
                .align_y(Alignment::Start)
        )
        .padding(16)
        .into()
    }

    fn add_profile_section<'a>(&'a self) -> Element<'a, SimpleSettingsMessage> {
        container(
            column()
                .push(
                    text("Add New Profile")
                        .size(16)
                        .class(cosmic::style::Text::Color(
                            theme::active().cosmic().palette.neutral_9.into()
                        ))
                )
                .push(
                    row()
                        .push(
                            text_input("Profile Name", &self.new_profile_name)
                                .on_input(SimpleSettingsMessage::NewProfileNameChanged)
                                .width(Length::Fill)
                        )
                        .push(widget::Space::with_width(8))
                        .push(
                            text_input("Model", &self.new_profile_model)
                                .on_input(SimpleSettingsMessage::NewProfileModelChanged)
                                .width(Length::Fill)
                        )
                        .push(widget::Space::with_width(8))
                        .push(
                            text_input("Endpoint", &self.new_profile_endpoint)
                                .on_input(SimpleSettingsMessage::NewProfileEndpointChanged)
                                .width(Length::Fill)
                        )
                )
                .push(
                    row()
                        .push(widget::Space::with_width(Length::Fill))
                        .push(button::suggested("Add Profile")
                            .on_press(SimpleSettingsMessage::AddNewProfile))
                )
                .spacing(12)
        )
        .padding(16)
        .into()
    }

}
