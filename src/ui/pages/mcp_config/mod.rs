use crate::ui::app::{CosmicLlmApp, Message};
use cosmic::{
    iced::Length,
    widget::{self, scrollable},
    Element,
};

pub fn mcp_config_view(app: &CosmicLlmApp) -> Element<Message> {
    // Load the actual MCP config (same as startup)
    let mcp_config =
        crate::config::MCPConfig::load_from_json().unwrap_or_else(|_| app.config.mcp.clone());

    let server_count = mcp_config.servers.len();
    let enabled_count = app.available_mcp_tools.len();

    // Build server list with owned data
    let mut server_column = cosmic::widget::column::with_capacity(mcp_config.servers.len());
    for (server_name, server_config) in mcp_config.servers {
        let command_text = format!("{} {}", server_config.command, server_config.args.join(" "));

        let server_widget = cosmic::widget::column::with_capacity(4)
            .push(
                cosmic::widget::row::with_capacity(2)
                    .push(cosmic::widget::text(server_name.clone()).size(16))
                    .push(cosmic::widget::Space::with_width(Length::Fill))
                    .push(cosmic::widget::text("Connected").size(12).class(
                        cosmic::style::Text::Color(cosmic::iced::Color::from_rgb(0.2, 0.8, 0.2)),
                    ))
                    .align_y(cosmic::iced::Alignment::Center),
            )
            .push(cosmic::widget::text(format!("Type: stdio")).size(12).class(
                cosmic::style::Text::Color(cosmic::iced::Color::from_rgb(0.6, 0.6, 0.6)),
            ))
            .push(
                cosmic::widget::text(format!("Command: {}", command_text))
                    .size(12)
                    .class(cosmic::style::Text::Color(cosmic::iced::Color::from_rgb(
                        0.6, 0.6, 0.6,
                    ))),
            )
            .spacing(4);

        server_column = server_column.push(server_widget);
    }

    cosmic::widget::column::with_capacity(4)
        .push(
            // Simple header
            cosmic::widget::row::with_capacity(3)
                .push(
                    cosmic::widget::row::with_capacity(2)
                        .push(widget::icon::from_name("configure-symbolic").size(20))
                        .push(cosmic::widget::text("MCP Configuration").size(20))
                        .spacing(8)
                        .align_y(cosmic::iced::Alignment::Center),
                )
                .push(cosmic::widget::Space::with_width(Length::Fill))
                .push(
                    cosmic::widget::text(format!(
                        "{} servers, {} tools",
                        server_count, enabled_count
                    ))
                    .size(12)
                    .class(cosmic::style::Text::Color(
                        cosmic::iced::Color::from_rgb(0.6, 0.6, 0.6),
                    )),
                )
                .spacing(12)
                .align_y(cosmic::iced::Alignment::Center),
        )
        .push(
            // Servers section
            cosmic::widget::column::with_capacity(2)
                .push(cosmic::widget::text(format!("MCP Servers ({})", server_count)).size(16))
                .push(if server_count == 0 {
                    Element::from(
                        cosmic::widget::column::with_capacity(3)
                            .push(widget::icon::from_name("network-server-symbolic").size(48))
                            .push(cosmic::widget::text("No MCP servers configured").size(16))
                            .push(
                                cosmic::widget::text(
                                    "Add MCP servers to enable tools and capabilities",
                                )
                                .size(12)
                                .class(cosmic::style::Text::Color(cosmic::iced::Color::from_rgb(
                                    0.6, 0.6, 0.6,
                                ))),
                            )
                            .spacing(8)
                            .align_x(cosmic::iced::Alignment::Center),
                    )
                } else {
                    Element::from(scrollable(server_column))
                })
                .spacing(12),
        )
        .push(
            // Tools section
            cosmic::widget::column::with_capacity(2)
                .push(cosmic::widget::text(format!("Available Tools ({})", enabled_count)).size(16))
                .push(tools_list_view(app))
                .spacing(12),
        )
        .spacing(16)
        .into()
}

pub fn tools_list_view(app: &CosmicLlmApp) -> Element<Message> {
    let tools = &app.available_mcp_tools;

    if tools.is_empty() {
        return cosmic::widget::column::with_capacity(3)
            .push(widget::icon::from_name("tool-symbolic").size(48))
            .push(cosmic::widget::text("No tools discovered yet").size(16))
            .push(
                cosmic::widget::text("Tools will appear here once MCP servers are connected")
                    .size(12)
                    .class(cosmic::style::Text::Color(cosmic::iced::Color::from_rgb(
                        0.6, 0.6, 0.6,
                    ))),
            )
            .spacing(8)
            .align_x(cosmic::iced::Alignment::Center)
            .into();
    }

    let mut column = cosmic::widget::column::with_capacity(tools.len());
    for tool in tools.iter() {
        // Build input schema text
        let input_text = if let Some(properties) = tool.parameters.get("properties") {
            if let Some(props_obj) = properties.as_object() {
                let params: Vec<String> = props_obj.keys().map(|k| k.to_string()).collect();
                if params.is_empty() {
                    "No parameters".to_string()
                } else {
                    format!("Parameters: {}", params.join(", "))
                }
            } else {
                "Parameters: (schema)".to_string()
            }
        } else {
            "No parameters defined".to_string()
        };

        let tool_item = cosmic::widget::column::with_capacity(3)
            .push(
                cosmic::widget::row::with_capacity(2)
                    .push(
                        cosmic::widget::text(&tool.name)
                            .size(14)
                            .font(cosmic::font::Font::MONOSPACE),
                    )
                    .push(cosmic::widget::Space::with_width(Length::Fill))
                    .push(cosmic::widget::text("Available").size(10).class(
                        cosmic::style::Text::Color(cosmic::iced::Color::from_rgb(0.2, 0.8, 0.2)),
                    ))
                    .align_y(cosmic::iced::Alignment::Center),
            )
            .push(cosmic::widget::text(&tool.description).size(12))
            .push(
                cosmic::widget::text(input_text)
                    .size(10)
                    .class(cosmic::style::Text::Color(cosmic::iced::Color::from_rgb(
                        0.5, 0.5, 0.5,
                    ))),
            )
            .spacing(4);

        column = column.push(tool_item);
    }

    scrollable(column).into()
}
