use cosmic::{
    iced::Length,
    widget::{self, settings},
    Element,
};
use std::collections::HashMap;

use crate::{
    config::{AppConfig, LlmProfile, MCPServerConfig},
};

#[derive(Debug, Clone)]
pub struct SettingsPage {
    pub selected_profile: String,
    pub editing_profile: Option<String>,
    pub new_profile: NewProfileState,
    pub mcp_servers: Vec<MCPServerConfig>,
    pub app_preferences: AppPreferences,
    pub validation_errors: HashMap<String, String>,
    pub text_input_ids: SettingsTextInputIds,
}

#[derive(Debug, Clone)]
pub struct NewProfileState {
    pub name: String,
    pub backend: String,
    pub model: String,
    pub endpoint: String,
    pub api_key: String,
}

impl Default for NewProfileState {
    fn default() -> Self {
        Self {
            name: String::new(),
            backend: "openai".to_string(), // Default to OpenAI
            model: String::new(),
            endpoint: String::new(),
            api_key: String::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct AppPreferences {
    pub theme: usize,
    pub auto_save: bool,
    pub notifications: bool,
    pub auto_scroll: bool,
}

impl Default for AppPreferences {
    fn default() -> Self {
        Self {
            theme: 0, // System theme
            auto_save: true,
            notifications: true,
            auto_scroll: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SettingsTextInputIds {
    pub profile_name: cosmic::widget::Id,
    pub profile_backend: cosmic::widget::Id,
    pub profile_model: cosmic::widget::Id,
    pub profile_endpoint: cosmic::widget::Id,
    pub profile_api_key: cosmic::widget::Id,
    pub mcp_server_name: cosmic::widget::Id,
    pub mcp_server_command: cosmic::widget::Id,
}

impl Default for SettingsTextInputIds {
    fn default() -> Self {
        Self {
            profile_name: cosmic::widget::Id::unique(),
            profile_backend: cosmic::widget::Id::unique(),
            profile_model: cosmic::widget::Id::unique(),
            profile_endpoint: cosmic::widget::Id::unique(),
            profile_api_key: cosmic::widget::Id::unique(),
            mcp_server_name: cosmic::widget::Id::unique(),
            mcp_server_command: cosmic::widget::Id::unique(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum SettingsMessage {
    // Profile Management
    SelectProfile(usize),
    EditProfile(String),
    SaveProfile(String, LlmProfile),
    DeleteProfile(String),
    AddNewProfile,
    CancelEditProfile,
    UpdateNewProfile(NewProfileField, String),
    SaveNewProfile,
    
    // MCP Management  
    AddMCPServer,
    EditMCPServer(usize),
    DeleteMCPServer(usize),
    UpdateMCPServer(usize, String, String), // index, name, command
    SaveMCPServer,
    CancelEditMCPServer,
    
    // App Preferences
    ChangeTheme(usize),
    ToggleAutoSave(bool),
    ToggleNotifications(bool),
    ToggleAutoScroll(bool),
    
    // Validation
    ValidateInput(String, String),
    ClearValidation(String),
    
    // Navigation
    BackToMain,
}

#[derive(Debug, Clone)]
pub enum NewProfileField {
    Name,
    Backend,
    Model,
    Endpoint,
    ApiKey,
}

impl SettingsPage {
    pub fn new(config: &AppConfig) -> Self {
        Self {
            selected_profile: config.default.clone(),
            editing_profile: None,
            new_profile: NewProfileState::default(),
            mcp_servers: config.mcp.servers.clone(),
            app_preferences: AppPreferences::default(),
            validation_errors: HashMap::new(),
            text_input_ids: SettingsTextInputIds::default(),
        }
    }

    pub fn view(&self, config: &AppConfig) -> Element<SettingsMessage> {
        let mut settings_column = settings::view_column(Vec::new());

        // LLM Profiles Section
        settings_column = settings_column
            .push(
                settings::section()
                    .title("LLM Profiles")
                    .add(
                        settings::item::item(
                            "Default Profile",
                            widget::dropdown(
                                &config.profiles.keys().cloned().collect::<Vec<_>>(),
                                config.profiles.keys().position(|k| k == &self.selected_profile),
                                SettingsMessage::SelectProfile,
                            )
                        )
                    )
                    .add(
                        settings::item::item(
                            "Add New Profile",
                            widget::button::suggested("Add Profile")
                                .on_press(SettingsMessage::AddNewProfile)
                        )
                    )
            );

        // Profile List
        if !config.profiles.is_empty() {
            let mut profile_items = Vec::new();
            for (name, profile) in &config.profiles {
                let is_selected = name == &self.selected_profile;
                let status_text = if is_selected { "✓ Current" } else { "Click to select" };
                
                profile_items.push(
                        settings::item::item(
                            name,
                            widget::row()
                                .push(
                                    widget::column()
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
                );
            }
            
            for item in profile_items {
                settings_column = settings_column.push(
                    settings::section()
                        .title("Configured Profiles")
                        .add(item)
                );
            }
        }

        // MCP Configuration Section
        settings_column = settings_column
            .push(
                settings::section()
                    .title("MCP Configuration")
                    .add(
                        settings::item::item(
                            "Add MCP Server",
                            widget::button::suggested("Add Server")
                                .on_press(SettingsMessage::AddMCPServer)
                        )
                    )
            );

        // MCP Server List
        if !self.mcp_servers.is_empty() {
            let mut server_items = Vec::new();
            for (index, server) in self.mcp_servers.iter().enumerate() {
                server_items.push(
                        settings::item::item(
                            &server.name,
                            widget::row()
                                .push(widget::text(server.command.as_deref().unwrap_or("")).size(12))
                                .push(
                                    widget::row()
                                        .push(
                                            widget::button::icon(cosmic::widget::icon::from_name("edit-symbolic"))
                                                .on_press(SettingsMessage::EditMCPServer(index))
                                        )
                                        .push(
                                            widget::button::icon(cosmic::widget::icon::from_name("user-trash-full-symbolic"))
                                                .on_press(SettingsMessage::DeleteMCPServer(index))
                                        )
                                )
                        )
                );
            }
            
            for item in server_items {
                settings_column = settings_column.push(
                    settings::section()
                        .title("Configured MCP Servers")
                        .add(item)
                );
            }
        }

        // App Preferences Section
        settings_column = settings_column
            .push(
                settings::section()
                    .title("App Preferences")
                    .add(
                        settings::item::item(
                            "Theme",
                            widget::dropdown(
                                &["System", "Dark", "Light"],
                                Some(self.app_preferences.theme),
                                SettingsMessage::ChangeTheme,
                            )
                        )
                    )
                    .add(
                        settings::item::item(
                            "Auto-save conversations",
                            widget::checkbox("Enable auto-save", self.app_preferences.auto_save)
                                .on_toggle(SettingsMessage::ToggleAutoSave)
                        )
                    )
                    .add(
                        settings::item::item(
                            "Notifications",
                            widget::checkbox("Enable notifications", self.app_preferences.notifications)
                                .on_toggle(SettingsMessage::ToggleNotifications)
                        )
                    )
                    .add(
                        settings::item::item(
                            "Auto-scroll to bottom",
                            widget::checkbox("Auto-scroll during streaming", self.app_preferences.auto_scroll)
                                .on_toggle(SettingsMessage::ToggleAutoScroll)
                        )
                    )
            );

        // Back button
        settings_column = settings_column
            .push(
                widget::row()
                    .push(widget::button::standard("Back to Chat")
                        .on_press(SettingsMessage::BackToMain))
                    .push(widget::Space::with_width(Length::Fill))
            );

        widget::scrollable(settings_column)
            .into()
    }

    pub fn update(&mut self, message: SettingsMessage, config: &mut AppConfig) -> cosmic::app::Task<SettingsMessage> {
        match message {
            SettingsMessage::SelectProfile(profile_index) => {
                if let Some(profile_name) = config.profiles.keys().nth(profile_index) {
                    self.selected_profile = profile_name.clone();
                    config.default = self.selected_profile.clone();
                }
            }
            SettingsMessage::EditProfile(profile_name) => {
                self.editing_profile = Some(profile_name);
            }
            SettingsMessage::SaveProfile(profile_name, profile) => {
                config.profiles.insert(profile_name.clone(), profile);
                self.editing_profile = None;
            }
            SettingsMessage::DeleteProfile(profile_name) => {
                config.profiles.remove(&profile_name);
                if self.selected_profile == profile_name && !config.profiles.is_empty() {
                    self.selected_profile = config.profiles.keys().next().unwrap().clone();
                    config.default = self.selected_profile.clone();
                }
            }
            SettingsMessage::AddNewProfile => {
                self.new_profile = NewProfileState::default();
            }
            SettingsMessage::CancelEditProfile => {
                self.editing_profile = None;
                self.new_profile = NewProfileState::default();
            }
            SettingsMessage::UpdateNewProfile(field, value) => {
                match field {
                    NewProfileField::Name => self.new_profile.name = value,
                    NewProfileField::Backend => self.new_profile.backend = value,
                    NewProfileField::Model => self.new_profile.model = value,
                    NewProfileField::Endpoint => self.new_profile.endpoint = value,
                    NewProfileField::ApiKey => self.new_profile.api_key = value,
                }
            }
            SettingsMessage::SaveNewProfile => {
                if !self.new_profile.name.is_empty() && !self.new_profile.model.is_empty() {
                    let profile = LlmProfile {
                        backend: self.new_profile.backend.clone(),
                        model: self.new_profile.model.clone(),
                        endpoint: self.new_profile.endpoint.clone(),
                        api_key: self.new_profile.api_key.clone(),
                        temperature: Some(0.7),
                        max_tokens: Some(1000),
                    };
                    config.profiles.insert(self.new_profile.name.clone(), profile);
                    self.selected_profile = self.new_profile.name.clone();
                    config.default = self.selected_profile.clone();
                    self.new_profile = NewProfileState::default();
                }
            }
            SettingsMessage::AddMCPServer => {
                self.mcp_servers.push(MCPServerConfig {
                    name: "New Server".to_string(),
                    r#type: "stdio".to_string(),
                    url: None,
                    command: Some("".to_string()),
                    args: None,
                });
            }
            SettingsMessage::EditMCPServer(index) => {
                if index < self.mcp_servers.len() {
                    // TODO: Implement edit dialog
                }
            }
            SettingsMessage::DeleteMCPServer(index) => {
                if index < self.mcp_servers.len() {
                    self.mcp_servers.remove(index);
                }
            }
            SettingsMessage::UpdateMCPServer(index, name, command) => {
                if index < self.mcp_servers.len() {
                    self.mcp_servers[index].name = name;
                    self.mcp_servers[index].command = Some(command);
                }
            }
            SettingsMessage::SaveMCPServer => {
                config.mcp.servers = self.mcp_servers.clone();
            }
            SettingsMessage::CancelEditMCPServer => {
                // TODO: Cancel edit state
            }
            SettingsMessage::ChangeTheme(theme_index) => {
                self.app_preferences.theme = theme_index;
            }
            SettingsMessage::ToggleAutoSave(enabled) => {
                self.app_preferences.auto_save = enabled;
            }
            SettingsMessage::ToggleNotifications(enabled) => {
                self.app_preferences.notifications = enabled;
            }
            SettingsMessage::ToggleAutoScroll(enabled) => {
                self.app_preferences.auto_scroll = enabled;
            }
            SettingsMessage::ValidateInput(field, value) => {
                // TODO: Implement validation logic
                self.validation_errors.insert(field, value);
            }
            SettingsMessage::ClearValidation(field) => {
                self.validation_errors.remove(&field);
            }
            SettingsMessage::BackToMain => {
                // This will be handled by the parent app
            }
        }

        cosmic::app::Task::none()
    }
}
