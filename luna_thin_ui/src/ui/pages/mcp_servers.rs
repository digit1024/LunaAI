//! MCP Servers page - displays all available MCP servers and their status

use cosmic::{
    iced::Length,
    widget::{self, button, container, text, Column, Row},
    Element,
};

use crate::server::dto::MCPServerStatus;
use crate::ui::app::{ConnectionStatus, LunaThinApp, Message};
use crate::ui::widgets::page_header;

pub fn mcp_servers_page(app: &LunaThinApp) -> Element<'_, Message> {
    let mut content = Column::new().spacing(16).padding(16);

    let trailing = if app.connection_status == ConnectionStatus::Connected {
        format!("{} servers", app.mcp_servers.len())
    } else {
        "Connect to view servers".to_string()
    };

    content = content.push(page_header::subpage_header(
        "MCP Servers",
        "network-server-symbolic",
        Some(trailing),
    ));

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

                let is_expanded = app.mcp_expanded_servers.contains(&server.name);
                let toggle_label = if is_expanded {
                    format!("▼ {} tools", server.tool_count)
                } else {
                    format!("▶ {} tools", server.tool_count)
                };

                let mut server_column = Column::new()
                    .push(
                        Row::new()
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
                    .width(Length::Fill);

                if server.tool_count > 0 {
                    server_column = server_column.push(
                        button::text(toggle_label)
                            .on_press(Message::ToggleMCPServerExpand(server.name.clone())),
                    );
                }

                if is_expanded && !server.tools.is_empty() {
                    let mut tools_column = Column::new().spacing(2).padding([4, 0, 0, 12]);
                    for tool_name in &server.tools {
                        tools_column = tools_column.push(
                            text(format!("• {tool_name}"))
                                .size(12)
                                .class(cosmic::style::Text::Color(
                                    cosmic::iced::Color::from_rgb(0.75, 0.75, 0.75),
                                )),
                        );
                    }
                    server_column = server_column.push(tools_column);
                }

                content = content.push(
                    container(server_column)
                        .padding(16)
                        .width(Length::Fill)
                        .class(cosmic::style::Container::Card),
                );
            }
        }
    }

    widget::scrollable(content).into()
}
