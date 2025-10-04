use cosmic::widget;
use cosmic::Element;

use crate::config::MCPServerConfig;
use super::page::SettingsMessage;

pub struct MCPConfigSection;

impl MCPConfigSection {
    pub fn new() -> Self {
        Self
    }

    pub fn view(&self, servers: &[MCPServerConfig]) -> Vec<Element<SettingsMessage>> {
        let mut items = Vec::new();

        // Add server button
        items.push(
            widget::settings::item::item(
                "Add MCP Server",
                widget::button::suggested("Add Server")
                    .on_press(SettingsMessage::AddMCPServer)
            )
        );

        // Server list
        if !servers.is_empty() {
            items.push(
                widget::settings::item::item(
                    "Configured MCP Servers",
                    self.server_list(servers)
                )
            );
        }

        items
    }

    fn server_list(&self, servers: &[MCPServerConfig]) -> Element<SettingsMessage> {
        let mut server_widgets = Vec::new();
        
        for (index, server) in servers.iter().enumerate() {
            server_widgets.push(
                widget::container(
                    widget::column()
                        .push(
                            widget::row()
                                .push(
                                    widget::column()
                                        .push(widget::text(&server.name).size(14))
                                        .push(widget::text(format!("Type: {}", server.r#type)).size(12))
                                        .push(widget::text(format!("Command: {}", server.command.as_deref().unwrap_or(""))).size(12))
                                )
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
                )
                .padding(12)
                .class(cosmic::style::Container::Card)
                .into()
            );
        }

        widget::column().push(widget::Space::with_height(8.0))
            .push(widget::column::with_children(server_widgets))
            .into()
    }

    pub fn validate_server(&self, server: &MCPServerConfig) -> Vec<String> {
        let mut errors = Vec::new();
        
        if server.name.is_empty() {
            errors.push("Server name is required".to_string());
        }
        
        if server.command.as_ref().map_or(true, |c| c.is_empty()) {
            errors.push("Command is required".to_string());
        }

        // Validate command format
        if let Some(command) = &server.command {
            if command.contains(' ') && !command.starts_with('"') {
                // Suggest quoting commands with spaces
                errors.push("Consider quoting commands with spaces".to_string());
            }
        }

        errors
    }

    pub fn create_new_server() -> MCPServerConfig {
        MCPServerConfig {
            name: "New Server".to_string(),
            r#type: "stdio".to_string(),
            url: None,
            command: Some("".to_string()),
            args: None,
        }
    }
}
