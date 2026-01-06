use cosmic::{
    app,
    iced::{Alignment, Length, Padding},
    theme,
    widget::{self, button, column, container, row, text, text_input, Space},
    Element,
};

use crate::config::{AppConfig, LlmProfile, ServerConfig, TitleSummaryConfig};

#[derive(Debug, Clone)]
pub struct SimpleSettingsPage {
    pub new_profile_name: String,
    pub new_profile_model: String,
    pub new_profile_endpoint: String,
    pub new_profile_api_key: String,
    pub new_profile_backend: String,
    pub expanded_profiles: std::collections::HashSet<String>,
    pub editing_profiles: std::collections::HashMap<String, EditingProfileState>,
    // Global settings
    pub server_host: String,
    pub server_port: u16,
    pub server_port_str: String,
    pub server_api_key: String,
    pub stream_timeout_secs: u64,
    pub stream_timeout_str: String,
    // Title generation settings
    pub title_generation_profile: String,
    pub summary_chars: u32,
    pub summary_chars_str: String,
    pub summary_loop_sleep_seconds: u64,
    pub summary_loop_str: String,
    pub title_generation_system_prompt: String,
    // Track if config has been changed
    pub has_changes: bool,
    // Staged profiles (for save on button)
    pub staged_profiles: std::collections::HashMap<String, LlmProfile>,
    pub staged_default: String,
    pub staged_server: ServerConfig,
    pub staged_title_summary: TitleSummaryConfig,
}

#[derive(Debug, Clone)]
pub struct EditingProfileState {
    pub name: String,
    pub backend: String,
    pub model: String,
    pub endpoint: String,
    pub api_key: String,
    pub temperature: Option<f32>,
    pub temperature_str: String,
    pub max_tokens: Option<u32>,
    pub max_tokens_str: String,
    pub context_window_size: Option<usize>,
    pub context_window_size_str: String,
    pub summarize_threshold: f32,
    pub summarize_threshold_str: String,
    pub profile_prompt_file: Option<String>,
    pub profile_prompt_file_str: String,
    pub enabled_mcp: Vec<String>,
    pub enabled_mcp_str: String,
    pub hidden: bool,
}

#[derive(Debug, Clone)]
pub enum SimpleSettingsMessage {
    #[allow(dead_code)] // Used in UI but compiler doesn't detect
    BackToMain,
    SetDefaultProfile(String),
    NewProfileNameChanged(String),
    NewProfileModelChanged(String),
    NewProfileEndpointChanged(String),
    NewProfileApiKeyChanged(String),
    NewProfileBackendChanged(String),
    AddNewProfile,
    ToggleProfile(String), // Expand/collapse profile
    StartEditProfile(String), // Start editing a profile
    CancelEditProfile(String), // Cancel editing
    SaveProfile(String), // Save edited profile
    DeleteProfile(String), // Delete profile
    UpdateProfileField(String, ProfileField, String), // profile_name, field, value
    UpdateProfileTemperature(String, Option<f32>),
    UpdateProfileMaxTokens(String, Option<u32>),
    UpdateProfileContextWindowSize(String, Option<usize>),
    UpdateProfileSummarizeThreshold(String, f32),
    // Global settings
    UpdateServerHost(String),
    UpdateServerPort(String),
    UpdateServerApiKey(String),
    UpdateStreamTimeout(String),
    // Title generation
    UpdateTitleGenProfile(String),
    UpdateSummaryChars(String),
    UpdateSummaryLoopSleep(String),
    UpdateTitleGenPrompt(String),
    // Config file
    OpenConfigFile,
    OpenProfilePrompt(String),
    // Save/Cancel
    SaveConfig,
    CancelConfig,
    // Profile prompt and MCP
    UpdateProfilePromptFile(String, String), // profile_name, prompt_file
    UpdateProfileEnabledMCP(String, String), // profile_name, enabled_mcp_str (comma-separated string)
    UpdateProfileHidden(String, bool), // profile_name, hidden
}

#[derive(Debug, Clone)]
pub enum ProfileField {
    #[allow(dead_code)] // Used in match patterns
    Name,
    Backend,
    Model,
    Endpoint,
    ApiKey,
}

impl SimpleSettingsPage {
    pub fn new() -> Self {
        Self {
            new_profile_name: String::new(),
            new_profile_model: String::new(),
            new_profile_endpoint: String::new(),
            new_profile_api_key: String::new(),
            new_profile_backend: "openai".to_string(),
            expanded_profiles: std::collections::HashSet::new(),
            editing_profiles: std::collections::HashMap::new(),
            server_host: String::new(),
            server_port: 8080,
            server_port_str: "8080".to_string(),
            server_api_key: String::new(),
            stream_timeout_secs: 600,
            stream_timeout_str: "600".to_string(),
            title_generation_profile: String::new(),
            summary_chars: 1000,
            summary_chars_str: "1000".to_string(),
            summary_loop_sleep_seconds: 15,
            summary_loop_str: "15".to_string(),
            title_generation_system_prompt: String::new(),
            has_changes: false,
            staged_profiles: std::collections::HashMap::new(),
            staged_default: String::new(),
            staged_server: ServerConfig::default(),
            staged_title_summary: TitleSummaryConfig::default(),
        }
    }

    pub fn load_from_config(&mut self, config: &AppConfig) {
        // Load global settings
        self.server_host = config.server.host.clone();
        self.server_port = config.server.port;
        self.server_port_str = config.server.port.to_string();
        self.server_api_key = config.server.api_key.clone();
        self.stream_timeout_secs = config.server.stream_timeout_secs;
        self.stream_timeout_str = config.server.stream_timeout_secs.to_string();
        
        // Load title generation settings
        self.title_generation_profile = config.title_summary.title_generation_profile.clone().unwrap_or_default();
        self.summary_chars = config.title_summary.summary_chars;
        self.summary_chars_str = config.title_summary.summary_chars.to_string();
        self.summary_loop_sleep_seconds = config.title_summary.summary_loop_sleep_seconds;
        self.summary_loop_str = config.title_summary.summary_loop_sleep_seconds.to_string();
        self.title_generation_system_prompt = config.title_summary.title_generation_system_prompt.clone();
        
        // Initialize staged config (copy of current config)
        self.staged_profiles = config.profiles.clone();
        self.staged_default = config.default.clone();
        self.staged_server = config.server.clone();
        self.staged_title_summary = config.title_summary.clone();
        
        // Reset changes flag
        self.has_changes = false;
    }

    /// Update the page with a message
    /// Returns a task that may produce app-level actions for messages that need app handling
    pub fn update(&mut self, message: SimpleSettingsMessage, config: &AppConfig) -> app::Task<SimpleSettingsMessage> {
        match message {
            SimpleSettingsMessage::BackToMain => {
                // Handled by parent app
            }
            SimpleSettingsMessage::SetDefaultProfile(name) => {
                if self.staged_profiles.contains_key(&name) {
                    self.staged_default = name;
                    self.has_changes = true;
                }
            }
            SimpleSettingsMessage::NewProfileNameChanged(val) => {
                self.new_profile_name = val;
            }
            SimpleSettingsMessage::NewProfileModelChanged(val) => {
                self.new_profile_model = val;
            }
            SimpleSettingsMessage::NewProfileEndpointChanged(val) => {
                self.new_profile_endpoint = val;
            }
            SimpleSettingsMessage::NewProfileApiKeyChanged(val) => {
                self.new_profile_api_key = val;
            }
            SimpleSettingsMessage::NewProfileBackendChanged(val) => {
                self.new_profile_backend = val;
            }
            SimpleSettingsMessage::AddNewProfile => {
                let name = self.new_profile_name.trim().to_string();
                let model = self.new_profile_model.trim().to_string();
                let endpoint = self.new_profile_endpoint.trim().to_string();
                let api_key = self.new_profile_api_key.trim().to_string();
                let backend = self.new_profile_backend.trim().to_string();
                if !name.is_empty() && !model.is_empty() {
                    let mut profile = LlmProfile::default();
                    profile.backend = if backend.is_empty() { "openai".to_string() } else { backend };
                    profile.model = model;
                    profile.endpoint = endpoint;
                    profile.api_key = api_key;
                    profile.temperature = Some(0.7);
                    profile.max_tokens = Some(1000);
                    self.staged_profiles.insert(name.clone(), profile);
                    if self.staged_default.is_empty() {
                        self.staged_default = name.clone();
                    }
                    self.has_changes = true;
                    // Clear inputs
                    self.new_profile_name.clear();
                    self.new_profile_model.clear();
                    self.new_profile_endpoint.clear();
                    self.new_profile_api_key.clear();
                    self.new_profile_backend = "openai".to_string();
                }
            }
            SimpleSettingsMessage::ToggleProfile(profile_name) => {
                if self.expanded_profiles.contains(&profile_name) {
                    self.expanded_profiles.remove(&profile_name);
                } else {
                    self.expanded_profiles.insert(profile_name);
                }
            }
            SimpleSettingsMessage::StartEditProfile(profile_name) => {
                if let Some(profile) = self.staged_profiles.get(&profile_name).cloned() {
                    self.editing_profiles.insert(
                        profile_name.clone(),
                        EditingProfileState {
                            name: profile_name.clone(),
                            backend: profile.backend,
                            model: profile.model,
                            endpoint: profile.endpoint,
                            api_key: profile.api_key,
                            temperature: profile.temperature,
                            temperature_str: profile.temperature.map(|t| t.to_string()).unwrap_or_default(),
                            max_tokens: profile.max_tokens,
                            max_tokens_str: profile.max_tokens.map(|t| t.to_string()).unwrap_or_default(),
                            context_window_size: profile.context_window_size,
                            context_window_size_str: profile.context_window_size.map(|s| s.to_string()).unwrap_or_default(),
                            summarize_threshold: profile.summarize_threshold,
                            summarize_threshold_str: profile.summarize_threshold.to_string(),
                            profile_prompt_file: profile.profile_prompt_file.clone(),
                            profile_prompt_file_str: profile.profile_prompt_file.as_deref().unwrap_or("").to_string(),
                            enabled_mcp: profile.enabled_mcp.clone(),
                            enabled_mcp_str: profile.enabled_mcp.join(", "),
                            hidden: profile.hidden,
                        },
                    );
                }
            }
            SimpleSettingsMessage::CancelEditProfile(profile_name) => {
                self.editing_profiles.remove(&profile_name);
            }
            SimpleSettingsMessage::SaveProfile(profile_name) => {
                if let Some(edit_state) = self.editing_profiles.get(&profile_name) {
                    if let Some(profile) = self.staged_profiles.get_mut(&profile_name) {
                        profile.backend = edit_state.backend.clone();
                        profile.model = edit_state.model.clone();
                        profile.endpoint = edit_state.endpoint.clone();
                        profile.api_key = edit_state.api_key.clone();
                        profile.temperature = edit_state.temperature;
                        profile.max_tokens = edit_state.max_tokens;
                        profile.context_window_size = edit_state.context_window_size;
                        profile.summarize_threshold = edit_state.summarize_threshold;
                        profile.profile_prompt_file = edit_state.profile_prompt_file.clone();
                        profile.enabled_mcp = edit_state.enabled_mcp.clone();
                        profile.hidden = edit_state.hidden;
                        self.has_changes = true;
                    }
                    self.editing_profiles.remove(&profile_name);
                }
            }
            SimpleSettingsMessage::DeleteProfile(profile_name) => {
                self.staged_profiles.remove(&profile_name);
                self.expanded_profiles.remove(&profile_name);
                self.editing_profiles.remove(&profile_name);
                if self.staged_default == profile_name && !self.staged_profiles.is_empty() {
                    self.staged_default = self.staged_profiles.keys().next()
                        .map(|k| k.clone())
                        .unwrap_or_else(|| {
                            tracing::warn!("staged_profiles was empty after check, using first available profile");
                            config.profiles.keys().next()
                                .cloned()
                                .unwrap_or_else(|| "default".to_string())
                        });
                }
                self.has_changes = true;
            }
            SimpleSettingsMessage::UpdateProfileField(profile_name, field, value) => {
                if let Some(edit_state) = self.editing_profiles.get_mut(&profile_name) {
                    match field {
                        ProfileField::Name => edit_state.name = value,
                        ProfileField::Backend => edit_state.backend = value,
                        ProfileField::Model => edit_state.model = value,
                        ProfileField::Endpoint => edit_state.endpoint = value,
                        ProfileField::ApiKey => edit_state.api_key = value,
                    }
                }
            }
            SimpleSettingsMessage::UpdateProfileTemperature(profile_name, temp) => {
                if let Some(edit_state) = self.editing_profiles.get_mut(&profile_name) {
                    edit_state.temperature = temp;
                    edit_state.temperature_str = temp.map(|t| t.to_string()).unwrap_or_default();
                }
            }
            SimpleSettingsMessage::UpdateProfileMaxTokens(profile_name, tokens) => {
                if let Some(edit_state) = self.editing_profiles.get_mut(&profile_name) {
                    edit_state.max_tokens = tokens;
                    edit_state.max_tokens_str = tokens.map(|t| t.to_string()).unwrap_or_default();
                }
            }
            SimpleSettingsMessage::UpdateProfileContextWindowSize(profile_name, size) => {
                if let Some(edit_state) = self.editing_profiles.get_mut(&profile_name) {
                    edit_state.context_window_size = size;
                    edit_state.context_window_size_str = size.map(|s| s.to_string()).unwrap_or_default();
                }
            }
            SimpleSettingsMessage::UpdateProfileSummarizeThreshold(profile_name, threshold) => {
                if let Some(edit_state) = self.editing_profiles.get_mut(&profile_name) {
                    edit_state.summarize_threshold = threshold;
                    edit_state.summarize_threshold_str = threshold.to_string();
                }
            }
            SimpleSettingsMessage::UpdateProfilePromptFile(profile_name, prompt_file) => {
                if let Some(edit_state) = self.editing_profiles.get_mut(&profile_name) {
                    edit_state.profile_prompt_file = if prompt_file.trim().is_empty() {
                        None
                    } else {
                        Some(prompt_file.clone())
                    };
                    edit_state.profile_prompt_file_str = prompt_file;
                }
            }
            SimpleSettingsMessage::UpdateProfileEnabledMCP(profile_name, enabled_mcp_str) => {
                if let Some(edit_state) = self.editing_profiles.get_mut(&profile_name) {
                    edit_state.enabled_mcp_str = enabled_mcp_str.clone();
                    // Parse the comma-separated string into Vec, but keep the raw string for editing
                    edit_state.enabled_mcp = enabled_mcp_str
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                }
            }
            SimpleSettingsMessage::UpdateProfileHidden(profile_name, hidden) => {
                if let Some(edit_state) = self.editing_profiles.get_mut(&profile_name) {
                    edit_state.hidden = hidden;
                }
            }
            SimpleSettingsMessage::UpdateServerHost(val) => {
                self.staged_server.host = val.clone();
                self.server_host = val;
                self.has_changes = true;
            }
            SimpleSettingsMessage::UpdateServerPort(val) => {
                if let Ok(port) = val.parse::<u16>() {
                    self.staged_server.port = port;
                    self.server_port = port;
                    self.server_port_str = val.clone();
                    self.has_changes = true;
                } else {
                    self.server_port_str = val;
                }
            }
            SimpleSettingsMessage::UpdateServerApiKey(val) => {
                self.staged_server.api_key = val.clone();
                self.server_api_key = val;
                self.has_changes = true;
            }
            SimpleSettingsMessage::UpdateStreamTimeout(val) => {
                if let Ok(timeout) = val.parse::<u64>() {
                    self.staged_server.stream_timeout_secs = timeout;
                    self.stream_timeout_secs = timeout;
                    self.stream_timeout_str = val.clone();
                    self.has_changes = true;
                } else {
                    self.stream_timeout_str = val;
                }
            }
            SimpleSettingsMessage::UpdateTitleGenProfile(val) => {
                self.staged_title_summary.title_generation_profile = if val.is_empty() {
                    None
                } else {
                    Some(val.clone())
                };
                self.title_generation_profile = val;
                self.has_changes = true;
            }
            SimpleSettingsMessage::UpdateSummaryChars(val) => {
                if let Ok(chars) = val.parse::<u32>() {
                    self.staged_title_summary.summary_chars = chars;
                    self.summary_chars = chars;
                    self.summary_chars_str = val.clone();
                    self.has_changes = true;
                } else {
                    self.summary_chars_str = val;
                }
            }
            SimpleSettingsMessage::UpdateSummaryLoopSleep(val) => {
                if let Ok(sleep) = val.parse::<u64>() {
                    self.staged_title_summary.summary_loop_sleep_seconds = sleep;
                    self.summary_loop_sleep_seconds = sleep;
                    self.summary_loop_str = val.clone();
                    self.has_changes = true;
                } else {
                    self.summary_loop_str = val;
                }
            }
            SimpleSettingsMessage::UpdateTitleGenPrompt(val) => {
                self.staged_title_summary.title_generation_system_prompt = val.clone();
                self.title_generation_system_prompt = val;
                self.has_changes = true;
            }
            SimpleSettingsMessage::OpenConfigFile | 
            SimpleSettingsMessage::OpenProfilePrompt(_) |
            SimpleSettingsMessage::SaveConfig |
            SimpleSettingsMessage::CancelConfig => {
                // These are handled by the parent app
            }
        }
        app::Task::none()
    }

    pub fn view<'a>(&'a self, _config: &'a AppConfig) -> Element<'a, SimpleSettingsMessage> {
        let mut content = column().spacing(16);

        // Header with Edit Config button
        content = content.push(
            container(
                row()
                    .push(
                        row()
                            .push(widget::icon::from_name("settings-symbolic").size(20))
                            .push(text("Settings").size(20))
                            .spacing(8)
                            .align_y(Alignment::Center),
                    )
                    .push(Space::with_width(Length::Fill))
                    .push(
                        button::text("Edit Config")
                            .on_press(SimpleSettingsMessage::OpenConfigFile)
                            .class(cosmic::style::Button::Text),
                    )
                    .spacing(12)
                    .align_y(Alignment::Center),
            )
            .padding(16),
        );

        // Global Settings Section
        content = content.push(self.global_settings_section(&self.staged_server));

        // Title Generation Section
        content = content.push(self.title_generation_section(&self.staged_title_summary));

        // Profiles Section
        content = content.push(
            container(
                column()
                    .push(
                        row()
                            .push(text("LLM Profiles").size(16).class(
                                cosmic::style::Text::Color(
                                    theme::active().cosmic().palette.neutral_9.into(),
                                ),
                            ))
                            .push(Space::with_width(Length::Fill))
                            .push(
                                button::suggested("Add Profile")
                                    .on_press(SimpleSettingsMessage::AddNewProfile),
                            )
                            .spacing(12)
                            .align_y(Alignment::Center),
                    )
                    .spacing(12),
            )
            .padding(16),
        );

        // Profile list - use staged profiles
        let mut sorted_profiles: Vec<_> = self.staged_profiles.iter().collect();
        sorted_profiles.sort_by_key(|(name, _)| *name);
        
        for (profile_name, profile) in sorted_profiles {
            content = content.push(self.profile_card(profile_name, profile, &self.staged_default));
        }

        // Add New Profile Section
        content = content.push(self.add_profile_section());

        // Save/Cancel buttons and change indicator
        let change_indicator: Element<'a, SimpleSettingsMessage> = if self.has_changes {
            text("● Unsaved changes")
                .size(12)
                .class(cosmic::style::Text::Color(
                    theme::active().cosmic().accent_color().into(),
                ))
                .into()
        } else {
            text("No unsaved changes")
                .size(12)
                .class(cosmic::style::Text::Color(
                    theme::active().cosmic().palette.neutral_6.into(),
                ))
                .into()
        };
        
        content = content.push(
            container(
                column()
                    .push(
                        text("Note: Changes require restart of server and UI to take effect")
                            .size(11)
                            .class(cosmic::style::Text::Color(
                                theme::active().cosmic().palette.neutral_6.into(),
                            ))
                    )
                    .push(
                        row()
                            .push(change_indicator)
                            .push(Space::with_width(Length::Fill))
                            .push(
                                button::standard("Cancel")
                                    .on_press(SimpleSettingsMessage::CancelConfig),
                            )
                            .push(
                                button::suggested("Save")
                                    .on_press(SimpleSettingsMessage::SaveConfig),
                            )
                            .spacing(8)
                            .align_y(Alignment::Center),
                    )
                    .spacing(8),
            )
            .padding(16),
        );

        widget::scrollable(content).into()
    }

    fn global_settings_section<'a>(&'a self, _server: &'a ServerConfig) -> Element<'a, SimpleSettingsMessage> {
        container(
            column()
                .push(
                    text("Global Settings").size(16).class(
                        cosmic::style::Text::Color(
                            theme::active().cosmic().palette.neutral_9.into(),
                        ),
                    ),
                )
                .push(
                    column()
                        .push(
                            column()
                                .push(
                                    row()
                                        .push(text("Server Host:").size(12).width(Length::Fixed(150.0)))
                                        .push(
                                            text_input("0.0.0.0", &self.server_host)
                                                .on_input(|v| SimpleSettingsMessage::UpdateServerHost(v))
                                                .width(Length::Fill),
                                        )
                                        .spacing(8)
                                        .align_y(Alignment::Center),
                                )
                                .push(
                                    text("Host address for the API server (default: 0.0.0.0)")
                                        .size(10)
                                        .class(cosmic::style::Text::Color(
                                            theme::active().cosmic().palette.neutral_6.into(),
                                        ))
                                )
                                .spacing(4),
                        )
                        .push(
                            column()
                                .push(
                                    row()
                                        .push(text("Server Port:").size(12).width(Length::Fixed(150.0)))
                                        .push(
                                            text_input("8080", &self.server_port_str)
                                                .on_input(|v| SimpleSettingsMessage::UpdateServerPort(v))
                                                .width(Length::Fill),
                                        )
                                        .spacing(8)
                                        .align_y(Alignment::Center),
                                )
                                .push(
                                    text("Port number for the API server (default: 8080)")
                                        .size(10)
                                        .class(cosmic::style::Text::Color(
                                            theme::active().cosmic().palette.neutral_6.into(),
                                        ))
                                )
                                .spacing(4),
                        )
                        .push(
                            column()
                                .push(
                                    row()
                                        .push(text("API Key:").size(12).width(Length::Fixed(150.0)))
                                        .push(
                                            text_input("LUna", &self.server_api_key)
                                                .on_input(|v| SimpleSettingsMessage::UpdateServerApiKey(v))
                                                .width(Length::Fill),
                                        )
                                        .spacing(8)
                                        .align_y(Alignment::Center),
                                )
                                .push(
                                    text("API key for authenticating requests to the server (default: LUna)")
                                        .size(10)
                                        .class(cosmic::style::Text::Color(
                                            theme::active().cosmic().palette.neutral_6.into(),
                                        ))
                                )
                                .spacing(4),
                        )
                        .push(
                            column()
                                .push(
                                    row()
                                        .push(text("Stream Timeout (s):").size(12).width(Length::Fixed(150.0)))
                                        .push(
                                            text_input("600", &self.stream_timeout_str)
                                                .on_input(|v| SimpleSettingsMessage::UpdateStreamTimeout(v))
                                                .width(Length::Fill),
                                        )
                                        .spacing(8)
                                        .align_y(Alignment::Center),
                                )
                                .push(
                                    text("Timeout in seconds for streaming responses (default: 600)")
                                        .size(10)
                                        .class(cosmic::style::Text::Color(
                                            theme::active().cosmic().palette.neutral_6.into(),
                                        ))
                                )
                                .spacing(4),
                        )
                        .spacing(8),
                )
                .spacing(12),
        )
        .padding(16)
        .into()
    }

    fn title_generation_section<'a>(&'a self, _title_config: &'a TitleSummaryConfig) -> Element<'a, SimpleSettingsMessage> {
        container(
            column()
                .push(
                    text("Title Generation").size(16).class(
                        cosmic::style::Text::Color(
                            theme::active().cosmic().palette.neutral_9.into(),
                        ),
                    ),
                )
                .push(
                    column()
                        .push(
                            column()
                                .push(
                                    row()
                                        .push(text("Profile:").size(12).width(Length::Fixed(150.0)))
                                        .push(
                                            text_input("Profile name", &self.title_generation_profile)
                                                .on_input(|v| SimpleSettingsMessage::UpdateTitleGenProfile(v))
                                                .width(Length::Fill),
                                        )
                                        .spacing(8)
                                        .align_y(Alignment::Center),
                                )
                                .push(
                                    text("LLM profile to use for generating conversation titles")
                                        .size(10)
                                        .class(cosmic::style::Text::Color(
                                            theme::active().cosmic().palette.neutral_6.into(),
                                        ))
                                )
                                .spacing(4),
                        )
                        .push(
                            column()
                                .push(
                                    row()
                                        .push(text("Summary Chars:").size(12).width(Length::Fixed(150.0)))
                                        .push(
                                            text_input("1000", &self.summary_chars_str)
                                                .on_input(|v| SimpleSettingsMessage::UpdateSummaryChars(v))
                                                .width(Length::Fill),
                                        )
                                        .spacing(8)
                                        .align_y(Alignment::Center),
                                )
                                .push(
                                    text("Maximum characters to summarize for title generation (default: 1000)")
                                        .size(10)
                                        .class(cosmic::style::Text::Color(
                                            theme::active().cosmic().palette.neutral_6.into(),
                                        ))
                                )
                                .spacing(4),
                        )
                        .push(
                            column()
                                .push(
                                    row()
                                        .push(text("Loop Sleep (s):").size(12).width(Length::Fixed(150.0)))
                                        .push(
                                            text_input("15", &self.summary_loop_str)
                                                .on_input(|v| SimpleSettingsMessage::UpdateSummaryLoopSleep(v))
                                                .width(Length::Fill),
                                        )
                                        .spacing(8)
                                        .align_y(Alignment::Center),
                                )
                                .push(
                                    text("Seconds to wait between title generation attempts (default: 15)")
                                        .size(10)
                                        .class(cosmic::style::Text::Color(
                                            theme::active().cosmic().palette.neutral_6.into(),
                                        ))
                                )
                                .spacing(4),
                        )
                        .push(
                            column()
                                .push(
                                    row()
                                        .push(text("System Prompt:").size(12).width(Length::Fixed(150.0)))
                                        .push(
                                            text_input("System prompt", &self.title_generation_system_prompt)
                                                .on_input(|v| SimpleSettingsMessage::UpdateTitleGenPrompt(v))
                                                .width(Length::Fill),
                                        )
                                        .spacing(8)
                                        .align_y(Alignment::Center),
                                )
                                .push(
                                    text("System prompt for title generation. Should instruct the model to generate only a title (max 100 chars)")
                                        .size(10)
                                        .class(cosmic::style::Text::Color(
                                            theme::active().cosmic().palette.neutral_6.into(),
                                        ))
                                )
                                .spacing(4),
                        )
                        .spacing(8),
                )
                .spacing(12),
        )
        .padding(16)
        .into()
    }

    fn profile_card<'a>(
        &'a self,
        profile_name: &'a str,
        profile: &'a LlmProfile,
        default_profile: &'a str,
    ) -> Element<'a, SimpleSettingsMessage> {
        let is_current = profile_name == default_profile;
        let is_expanded = self.expanded_profiles.contains(profile_name);
        let is_editing = self.editing_profiles.contains_key(profile_name);
        let expand_icon = if is_expanded { "▼" } else { "▶" };
        let profile_name_owned7 = profile_name.to_string();

        let status_widget: Element<'a, SimpleSettingsMessage> = if is_current {
            text("✓ Current")
                .size(12)
                .class(cosmic::style::Text::Color(
                    theme::active().cosmic().accent_color().into(),
                ))
                .into()
        } else {
            button::standard("Set as default")
                .on_press(SimpleSettingsMessage::SetDefaultProfile(
                    profile_name.to_string(),
                ))
                .into()
        };

        let mut card_content = column().spacing(8);

        // Header row
        let header_row = row()
            .push(
                button::text(expand_icon)
                    .on_press(SimpleSettingsMessage::ToggleProfile(profile_name.to_string()))
                    .class(cosmic::style::Button::Text),
            )
            .push(text(profile_name).size(14).class(
                cosmic::style::Text::Color(
                    theme::active().cosmic().palette.neutral_9.into(),
                ),
            ))
            .push(Space::with_width(Length::Fill))
            .push(status_widget)
            .push(
                button::text("Edit")
                    .on_press(SimpleSettingsMessage::StartEditProfile(profile_name.to_string()))
                    .class(cosmic::style::Button::Text),
            )
            .push(
                button::text("Delete")
                    .on_press(SimpleSettingsMessage::DeleteProfile(profile_name.to_string()))
                    .class(cosmic::style::Button::Text),
            )
            .push(
                button::text("Prompt")
                    .on_press(SimpleSettingsMessage::OpenProfilePrompt(profile_name.to_string()))
                    .class(cosmic::style::Button::Text),
            )
            .spacing(8)
            .align_y(Alignment::Center);

        card_content = card_content.push(header_row);

        // Expanded content
        if is_expanded {
            if is_editing {
                // Show edit form
                if let Some(edit_state) = self.editing_profiles.get(profile_name) {
                    let profile_name_base = profile_name_owned7.clone();
                    let profile_name_backend = profile_name_base.clone();
                    let profile_name_model = profile_name_base.clone();
                    let profile_name_endpoint = profile_name_base.clone();
                    let profile_name_apikey = profile_name_base.clone();
                    let profile_name_temp = profile_name_base.clone();
                    let profile_name_tokens = profile_name_base.clone();
                    let profile_name_context = profile_name_base.clone();
                    let profile_name_threshold = profile_name_base.clone();
                    let profile_name_prompt = profile_name_base.clone();
                    let profile_name_mcp = profile_name_base.clone();
                    let profile_name_hidden = profile_name_base.clone();
                    let profile_name_save = profile_name_base.clone();
                    let profile_name_cancel = profile_name_base.clone();
                    card_content = card_content.push(
                        container(
                            column()
                                .push(
                                    row()
                                        .push(text("Backend:").size(12).width(Length::Fixed(120.0)))
                                        .push(
                                            text_input("openai", &edit_state.backend)
                                                .on_input(move |v| SimpleSettingsMessage::UpdateProfileField(
                                                    profile_name_backend.clone(),
                                                    ProfileField::Backend,
                                                    v,
                                                ))
                                                .width(Length::Fill),
                                        )
                                        .spacing(8),
                                )
                                .push(
                                    row()
                                        .push(text("Model:").size(12).width(Length::Fixed(120.0)))
                                        .push(
                                            text_input("Model", &edit_state.model)
                                                .on_input(move |v| SimpleSettingsMessage::UpdateProfileField(
                                                    profile_name_model.clone(),
                                                    ProfileField::Model,
                                                    v,
                                                ))
                                                .width(Length::Fill),
                                        )
                                        .spacing(8),
                                )
                                .push(
                                    row()
                                        .push(text("Endpoint:").size(12).width(Length::Fixed(120.0)))
                                        .push(
                                            text_input("Endpoint", &edit_state.endpoint)
                                                .on_input(move |v| SimpleSettingsMessage::UpdateProfileField(
                                                    profile_name_endpoint.clone(),
                                                    ProfileField::Endpoint,
                                                    v,
                                                ))
                                                .width(Length::Fill),
                                        )
                                        .spacing(8),
                                )
                                .push(
                                    row()
                                        .push(text("API Key:").size(12).width(Length::Fixed(120.0)))
                                        .push(
                                            text_input("API Key", &edit_state.api_key)
                                                .on_input(move |v| SimpleSettingsMessage::UpdateProfileField(
                                                    profile_name_apikey.clone(),
                                                    ProfileField::ApiKey,
                                                    v,
                                                ))
                                                .width(Length::Fill),
                                        )
                                        .spacing(8),
                                )
                                .push(
                                    row()
                                        .push(text("Temperature:").size(12).width(Length::Fixed(120.0)))
                                        .push(
                                            text_input("0.7", &edit_state.temperature_str)
                                                .on_input(move |v| {
                                                    let temp = v.parse::<f32>().ok();
                                                    SimpleSettingsMessage::UpdateProfileTemperature(
                                                        profile_name_temp.clone(),
                                                        temp,
                                                    )
                                                })
                                                .width(Length::Fill),
                                        )
                                        .spacing(8),
                                )
                                .push(
                                    row()
                                        .push(text("Max Tokens:").size(12).width(Length::Fixed(120.0)))
                                        .push(
                                            text_input("1000", &edit_state.max_tokens_str)
                                                .on_input(move |v| {
                                                    let tokens = v.parse::<u32>().ok();
                                                    SimpleSettingsMessage::UpdateProfileMaxTokens(
                                                        profile_name_tokens.clone(),
                                                        tokens,
                                                    )
                                                })
                                                .width(Length::Fill),
                                        )
                                        .spacing(8),
                                )
                                .push(
                                    row()
                                        .push(text("Context Window Size:").size(12).width(Length::Fixed(120.0)))
                                        .push(
                                            text_input("Auto", &edit_state.context_window_size_str)
                                                .on_input(move |v| {
                                                    let size = if v.trim().is_empty() {
                                                        None
                                                    } else {
                                                        v.parse::<usize>().ok()
                                                    };
                                                    SimpleSettingsMessage::UpdateProfileContextWindowSize(
                                                        profile_name_context.clone(),
                                                        size,
                                                    )
                                                })
                                                .width(Length::Fill),
                                        )
                                        .spacing(8),
                                )
                                .push(
                                    row()
                                        .push(text("Summarize Threshold:").size(12).width(Length::Fixed(120.0)))
                                        .push(
                                            text_input("0.7", &edit_state.summarize_threshold_str)
                                                .on_input(move |v| {
                                                    let threshold = v.parse::<f32>().unwrap_or(0.7);
                                                    SimpleSettingsMessage::UpdateProfileSummarizeThreshold(
                                                        profile_name_threshold.clone(),
                                                        threshold,
                                                    )
                                                })
                                                .width(Length::Fill),
                                        )
                                        .spacing(8),
                                )
                                .push(
                                    row()
                                        .push(text("Prompt File:").size(12).width(Length::Fixed(120.0)))
                                        .push(
                                            text_input("prompt.txt", &edit_state.profile_prompt_file_str)
                                                .on_input(move |v| {
                                                    let prompt_file = if v.is_empty() { None } else { Some(v) };
                                                    SimpleSettingsMessage::UpdateProfilePromptFile(
                                                        profile_name_prompt.clone(),
                                                        prompt_file.unwrap_or_default(),
                                                    )
                                                })
                                                .width(Length::Fill),
                                        )
                                        .spacing(8),
                                )
                                .push(
                                    row()
                                        .push(text("Enabled MCP:").size(12).width(Length::Fixed(120.0)))
                                        .push(
                                            text_input("server1, server2", &edit_state.enabled_mcp_str)
                                                .on_input(move |v| {
                                                    SimpleSettingsMessage::UpdateProfileEnabledMCP(
                                                        profile_name_mcp.clone(),
                                                        v,
                                                    )
                                                })
                                                .width(Length::Fill),
                                        )
                                        .spacing(8),
                                )
                                .push(
                                    row()
                                        .push(text("Hidden:").size(12).width(Length::Fixed(120.0)))
                                        .push(
                                            widget::checkbox("Hide profile from dropdowns", edit_state.hidden)
                                                .on_toggle(move |hidden| SimpleSettingsMessage::UpdateProfileHidden(
                                                    profile_name_hidden.clone(),
                                                    hidden,
                                                )),
                                        )
                                        .spacing(8),
                                )
                                .push(
                                    row()
                                        .push(Space::with_width(Length::Fill))
                                        .push(
                                            button::standard("Cancel")
                                                .on_press(SimpleSettingsMessage::CancelEditProfile(
                                                    profile_name_cancel.clone(),
                                                )),
                                        )
                                        .push(
                                            button::suggested("Save")
                                                .on_press(SimpleSettingsMessage::SaveProfile(
                                                    profile_name_save.clone(),
                                                )),
                                        )
                                        .spacing(8),
                                )
                                .spacing(8),
                        )
                        .padding(Padding::from([8, 0, 0, 24])),
                    );
                }
            } else {
                // Show read-only details - create owned strings
                let backend_text = format!("Backend: {}", profile.backend);
                let model_text = format!("Model: {}", profile.model);
                let endpoint_text = format!("Endpoint: {}", profile.endpoint);
                let temp_text = format!(
                    "Temperature: {}",
                    profile.temperature.map(|t| t.to_string()).unwrap_or_else(|| "N/A".to_string())
                );
                let tokens_text = format!(
                    "Max Tokens: {}",
                    profile.max_tokens.map(|t| t.to_string()).unwrap_or_else(|| "N/A".to_string())
                );
                let context_window_text = format!(
                    "Context Window Size: {}",
                    profile.context_window_size.map(|s| s.to_string()).unwrap_or_else(|| "Auto".to_string())
                );
                let summarize_threshold_text = format!(
                    "Summarize Threshold: {:.2}",
                    profile.summarize_threshold
                );
                let prompt_file_text = format!(
                    "Prompt File: {}",
                    profile.profile_prompt_file.as_ref().map(|s| s.as_str()).unwrap_or("None")
                );
                let enabled_mcp_text = if profile.enabled_mcp.is_empty() {
                    "Enabled MCP: None".to_string()
                } else {
                    format!("Enabled MCP: {}", profile.enabled_mcp.join(", "))
                };
                
                card_content = card_content.push(
                    container(
                        column()
                            .push(text(backend_text).size(12).class(
                                cosmic::style::Text::Color(
                                    theme::active().cosmic().palette.neutral_6.into(),
                                ),
                            ))
                            .push(text(model_text).size(12).class(
                                cosmic::style::Text::Color(
                                    theme::active().cosmic().palette.neutral_6.into(),
                                ),
                            ))
                            .push(text(endpoint_text).size(12).class(
                                cosmic::style::Text::Color(
                                    theme::active().cosmic().palette.neutral_6.into(),
                                ),
                            ))
                            .push(
                                text(temp_text)
                                .size(12)
                                .class(cosmic::style::Text::Color(
                                    theme::active().cosmic().palette.neutral_6.into(),
                                )),
                            )
                            .push(
                                text(tokens_text)
                                .size(12)
                                .class(cosmic::style::Text::Color(
                                    theme::active().cosmic().palette.neutral_6.into(),
                                )),
                            )
                            .push(
                                text(context_window_text)
                                .size(12)
                                .class(cosmic::style::Text::Color(
                                    theme::active().cosmic().palette.neutral_6.into(),
                                )),
                            )
                            .push(
                                text(summarize_threshold_text)
                                .size(12)
                                .class(cosmic::style::Text::Color(
                                    theme::active().cosmic().palette.neutral_6.into(),
                                )),
                            )
                            .push(
                                text(prompt_file_text)
                                .size(12)
                                .class(cosmic::style::Text::Color(
                                    theme::active().cosmic().palette.neutral_6.into(),
                                )),
                            )
                            .push(
                                text(enabled_mcp_text)
                                .size(12)
                                .class(cosmic::style::Text::Color(
                                    theme::active().cosmic().palette.neutral_6.into(),
                                )),
                            )
                            .spacing(4),
                    )
                    .padding(Padding::from([8, 0, 0, 24])),
                );
            }
        }

        // Build the final card with owned content
        let owned_card = card_content;
        container(owned_card)
            .padding(16)
            .class(cosmic::style::Container::Card)
            .into()
    }

    fn add_profile_section<'a>(&'a self) -> Element<'a, SimpleSettingsMessage> {
        container(
            column()
                .push(
                    text("Add New Profile")
                        .size(16)
                        .class(cosmic::style::Text::Color(
                            theme::active().cosmic().palette.neutral_9.into(),
                        )),
                )
                .push(
                    column()
                        .push(
                            row()
                                .push(
                                    text_input("Profile Name", &self.new_profile_name)
                                        .on_input(SimpleSettingsMessage::NewProfileNameChanged)
                                        .width(Length::Fill),
                                )
                                .push(Space::with_width(8))
                                .push(
                                    text_input("Backend", &self.new_profile_backend)
                                        .on_input(SimpleSettingsMessage::NewProfileBackendChanged)
                                        .width(Length::Fill),
                                ),
                        )
                        .push(
                            row()
                                .push(
                                    text_input("Model", &self.new_profile_model)
                                        .on_input(SimpleSettingsMessage::NewProfileModelChanged)
                                        .width(Length::Fill),
                                )
                                .push(Space::with_width(8))
                                .push(
                                    text_input("Endpoint", &self.new_profile_endpoint)
                                        .on_input(SimpleSettingsMessage::NewProfileEndpointChanged)
                                        .width(Length::Fill),
                                ),
                        )
                        .push(
                            text_input("API Key", &self.new_profile_api_key)
                                .on_input(SimpleSettingsMessage::NewProfileApiKeyChanged)
                                .width(Length::Fill),
                        )
                        .spacing(8),
                )
                .push(
                    row()
                        .push(Space::with_width(Length::Fill))
                        .push(
                            button::suggested("Add Profile")
                                .on_press(SimpleSettingsMessage::AddNewProfile),
                        ),
                )
                .spacing(12),
        )
        .padding(16)
        .into()
    }
}
