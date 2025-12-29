pub mod page;

pub use page::{Message, Page};

use crate::mcp::registry::ServerStatus;
use crate::ui::app::CosmicLlmApp;
use cosmic::{
    iced::{Length, Padding},
    widget::{self, button, container, scrollable},
    Element,
};

/// View function
pub fn mcp_config_view(app: &CosmicLlmApp) -> Element<crate::ui::app::Message> {
    // Load the actual MCP config (same as startup)
    let mcp_config =
        crate::config::MCPConfig::load_from_json().unwrap_or_else(|_| app.config.mcp.clone());

    // Get registry data
    let (server_statuses, tools_by_server, all_server_names) = {
        if let Ok(registry) = app.mcp_registry.try_read() {
            let mut statuses = std::collections::HashMap::new();
            let tools_by_server = registry.get_tools_by_server();
            let all_server_names = registry.get_all_server_names(&mcp_config);
            
            for server_name in &all_server_names {
                statuses.insert(server_name.clone(), registry.get_server_status(server_name));
            }
            
            (statuses, tools_by_server, all_server_names)
        } else {
            (
                std::collections::HashMap::new(),
                std::collections::HashMap::new(),
                mcp_config.servers.keys().cloned().collect::<Vec<_>>(),
            )
        }
    };

    let server_count = all_server_names.len();
    let total_tools: usize = tools_by_server.values().map(|tools| tools.len()).sum();

    // Build expandable server list
    let mut server_column = cosmic::widget::column::with_capacity(all_server_names.len());
    for server_name in &all_server_names {
        let status = server_statuses.get(server_name).cloned().unwrap_or(ServerStatus::Failed("Unknown".to_string()));
        let is_expanded = app.mcp_config_page.expanded_servers.contains(server_name);
        let tools = tools_by_server.get(server_name).cloned().unwrap_or_default();

        // Status badge
        let (status_text, status_color) = match &status {
            ServerStatus::Connected => ("Connected", cosmic::iced::Color::from_rgb(0.2, 0.8, 0.2)),
            ServerStatus::Failed(_) => ("Failed", cosmic::iced::Color::from_rgb(0.8, 0.2, 0.2)),
        };

        // Expand/collapse icon
        let expand_icon = if is_expanded { "▼" } else { "▶" };

        // Server header row
        let header_row = cosmic::widget::row::with_capacity(4)
            .push(
                button::text(expand_icon)
                    .on_press(crate::ui::app::Message::MCPConfigPage(page::Message::ToggleServer(server_name.clone())))
                    .class(cosmic::style::Button::Text),
            )
            .push(cosmic::widget::text(server_name.clone()).size(16))
            .push(cosmic::widget::Space::with_width(Length::Fill))
            .push(
                cosmic::widget::text(status_text)
                    .size(12)
                    .class(cosmic::style::Text::Color(status_color)),
            )
            .align_y(cosmic::iced::Alignment::Center)
            .spacing(8);

        let mut server_widget = cosmic::widget::column::with_capacity(3)
            .push(header_row);

        // Show tools when expanded
        if is_expanded {
            if tools.is_empty() {
                server_widget = server_widget.push(
                    container(
                        cosmic::widget::text("No tools available")
                            .size(12)
                            .class(cosmic::style::Text::Color(cosmic::iced::Color::from_rgb(0.6, 0.6, 0.6)))
                    )
                    .padding(Padding::from([4, 0, 0, 24])),
                );
            } else {
                // Build tools column - collect widgets first to avoid lifetime issues
                let tool_widgets: Vec<Element<crate::ui::app::Message>> = tools.into_iter().map(render_tool_item).collect();
                let mut tools_column = cosmic::widget::column::with_capacity(tool_widgets.len());
                for tool_widget in tool_widgets {
                    tools_column = tools_column.push(tool_widget);
                }
                server_widget = server_widget.push(
                    container(tools_column.spacing(8))
                        .padding(Padding::from([4, 0, 0, 24])),
                );
            }

            // Show error message if failed
            if let ServerStatus::Failed(error_msg) = &status {
                let error_text = format!("Error: {}", error_msg);
                server_widget = server_widget.push(
                    container(
                        cosmic::widget::text(error_text)
                            .size(11)
                            .class(cosmic::style::Text::Color(cosmic::iced::Color::from_rgb(0.8, 0.4, 0.4)))
                    )
                    .padding(Padding::from([4, 0, 0, 24])),
                );
            }
        }

        server_column = server_column.push(
            container(server_widget)
                .padding(12)
                .class(cosmic::style::Container::Card),
        );
    }

    cosmic::widget::column::with_capacity(3)
        .push(
            // Header
            cosmic::widget::row::with_capacity(4)
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
                        server_count, total_tools
                    ))
                    .size(12)
                    .class(cosmic::style::Text::Color(
                        cosmic::iced::Color::from_rgb(0.6, 0.6, 0.6),
                    )),
                )
                .push(
                    button::text("Edit Config")
                        .on_press(crate::ui::app::Message::OpenMCPConfig)
                        .class(cosmic::style::Button::Text),
                )
                .spacing(12)
                .align_y(cosmic::iced::Alignment::Center),
        )
        .push(
            // Servers section
            if server_count == 0 {
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
                Element::from(scrollable(server_column).spacing(8))
            },
        )
        .spacing(16)
        .into()
}

fn render_tool_item(tool: crate::llm::ToolDefinition) -> Element<'static, crate::ui::app::Message> {
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

    container(
        cosmic::widget::column::with_capacity(3)
            .push(
                cosmic::widget::row::with_capacity(2)
            .push(
                cosmic::widget::text(tool.name)
                    .size(14)
                    .font(cosmic::font::Font::MONOSPACE),
            )
                    .push(cosmic::widget::Space::with_width(Length::Fill))
                    .align_y(cosmic::iced::Alignment::Center),
            )
            .push(cosmic::widget::text(tool.description).size(12))
            .push(
                cosmic::widget::text(input_text)
                    .size(10)
                    .class(cosmic::style::Text::Color(cosmic::iced::Color::from_rgb(
                        0.5, 0.5, 0.5,
                    ))),
            )
            .spacing(4)
    )
    .padding(8)
    .class(cosmic::style::Container::Card)
    .into()
}

