use cosmic::widget;
use cosmic::Element;

use crate::config::MCPServerConfig;
use super::super::page::SettingsMessage;

#[derive(Debug, Clone)]
pub struct ServerCard {
    pub server: MCPServerConfig,
    pub index: usize,
    pub is_editing: bool,
    pub text_input_ids: ServerTextInputIds,
}

#[derive(Debug, Clone)]
pub struct ServerTextInputIds {
    pub name: cosmic::widget::Id,
    pub command: cosmic::widget::Id,
}

impl Default for ServerTextInputIds {
    fn default() -> Self {
        Self {
            name: cosmic::widget::Id::unique(),
            command: cosmic::widget::Id::unique(),
        }
    }
}

impl ServerCard {
    pub fn new(server: MCPServerConfig, index: usize) -> Self {
        Self {
            server,
            index,
            is_editing: false,
            text_input_ids: ServerTextInputIds::default(),
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
        widget::settings::item::item_row(
            &self.server.name,
            widget::row()
                .push(
                    widget::column()
                        .push(widget::text(format!("Type: {:?}", self.server.server_type)).size(12))
                        .push(widget::text(format!("Command: {}", self.server.command)).size(12))
                )
                .push(
                    widget::row()
                        .push(
                            widget::button::icon(cosmic::widget::icon::from_name("edit-symbolic"))
                                .on_press(SettingsMessage::EditMCPServer(self.index))
                        )
                        .push(
                            widget::button::icon(cosmic::widget::icon::from_name("user-trash-full-symbolic"))
                                .on_press(SettingsMessage::DeleteMCPServer(self.index))
                        )
                )
        ).into()
    }

    fn edit_view(&self) -> Element<SettingsMessage> {
        widget::settings::item::item_row(
            "Edit MCP Server",
            widget::column()
                .push(
                    widget::text_input("Server Name", &self.server.name)
                        .id(self.text_input_ids.name.clone())
                        .on_input(|name| SettingsMessage::UpdateMCPServer(
                            self.index,
                            name,
                            self.server.command.clone()
                        ))
                )
                .push(
                    widget::text_input("Command", &self.server.command)
                        .id(self.text_input_ids.command.clone())
                        .on_input(|command| SettingsMessage::UpdateMCPServer(
                            self.index,
                            self.server.name.clone(),
                            command
                        ))
                )
                .push(
                    widget::row()
                        .push(
                            widget::button::suggested("Save")
                                .on_press(SettingsMessage::SaveMCPServer)
                        )
                        .push(
                            widget::button::standard("Cancel")
                                .on_press(SettingsMessage::CancelEditMCPServer)
                        )
                )
        ).into()
    }
}
