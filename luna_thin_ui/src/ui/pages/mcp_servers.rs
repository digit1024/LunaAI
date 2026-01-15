//! MCP Servers page - displays all available MCP servers and their status

use cosmic::{
    iced::Length,
    widget::{self, button, column, container, row, text, Space},
    Element,
};

use crate::server::dto::MCPServerStatus;
use crate::ui::app::{LunaThinApp, Message, ConnectionStatus};

pub fn mcp_servers_page(app: &LunaThinApp) -> Element<Message> {
    let mut content = column().spacing(16).padding(16);

    // Header
    content = content.push(text("🔌 MCP Servers").size(24));

    // Info message
    if app.connection_status != ConnectionStatus::Connected {
        content = content.push(
            container(
                text("Connect to a server to view MCP servers")
                    .size(14)
                    .class(cosmic::style::Text::Color(cosmic::iced::Color::from_rgb(
                        0.7, 0.7, 0.7,
                    ))),
            )
            .padding(12)
            .width(Length::Fill)
            .class(cosmic::style::Container::Card),
        );
    } else {
        // Refresh button
        content = content.push(
            row()
                .push(
                    button::text("🔄 Refresh")
                        .on_press(Message::LoadMCPServers)
                        .class(cosmic::style::Button::Standard),
                ),
        );

        // Servers list
        if app.mcp_servers.is_empty() {
            content = content.push(
                container(
                    text("No MCP servers found")
                        .size(14)
                        .class(cosmic::style::Text::Color(cosmic::iced::Color::from_rgb(
                            0.7, 0.7, 0.7,
                        ))),
                )
                .padding(16)
                .width(Length::Fill)
                .class(cosmic::style::Container::Card),
            );
        } else {
            for server in &app.mcp_servers {
                let (status_icon, status_text, status_desc) = match &server.status {
                    MCPServerStatus::Connected => (
                        "🟢",
                        "Connected",
                        "Server is connected and available",
                    ),
                    MCPServerStatus::Failed { error } => (
                        "🔴",
                        "Failed",
                        error.as_str(),
                    ),
                };

                content = content.push(
                    container(
                        row()
                            .push(text(status_icon).size(24))
                            .push(
                                column()
                                    .push(text(&server.name).size(16).width(Length::Fill))
                                    .push(
                                        row()
                                            .push(text(status_text).size(14))
                                            .push(Space::with_width(8))
                                            .push(
                                                text(status_desc)
                                                    .size(12)
                                                    .class(cosmic::style::Text::Color(
                                                        cosmic::iced::Color::from_rgb(0.6, 0.6, 0.6),
                                                    )),
                                            )
                                            .spacing(4),
                                    )
                                    .spacing(4)
                                    .width(Length::Fill),
                            )
                            .spacing(12)
                            .align_y(cosmic::iced::Alignment::Center)
                            .width(Length::Fill),
                    )
                    .padding(16)
                    .width(Length::Fill)
                    .class(cosmic::style::Container::Card),
                );
            }
        }
    }

    widget::scrollable(content).into()
}

