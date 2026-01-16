//! MCP Servers page - displays all available MCP servers and their status

use cosmic::{
    iced::Length,
    widget::{self, column, container, row, text},
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
                let (status_text, status_desc, status_color) = match &server.status {
                    MCPServerStatus::Connected => (
                        "Connected",
                        format!("{} tools available", server.tool_count),
                        cosmic::iced::Color::from_rgb(0.0, 0.8, 0.0), // Green
                    ),
                    MCPServerStatus::Failed { error } => (
                        "Failed",
                        error.clone(),
                        cosmic::iced::Color::from_rgb(0.8, 0.0, 0.0), // Red
                    ),
                };

                content = content.push(
                    container(
                        column()
                            .push(
                                row()
                                    .push(text(&server.name).size(16).width(Length::Fill))
                                    .push(
                                        text(status_text)
                                            .size(14)
                                            .class(cosmic::style::Text::Color(status_color)),
                                    )
                                    .spacing(8)
                                    .align_y(cosmic::iced::Alignment::Center)
                                    .width(Length::Fill),
                            )
                            .push(
                                text(status_desc)
                                    .size(12)
                                    .class(cosmic::style::Text::Color(
                                        cosmic::iced::Color::from_rgb(0.6, 0.6, 0.6),
                                    )),
                            )
                            .spacing(4)
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

