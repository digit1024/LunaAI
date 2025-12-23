use crate::ui::app::{CosmicLlmApp, Message};
use cosmic::{
    iced::{Length, Padding},
    widget::{button, container, scrollable},
    Element,
};

pub fn tools_context_view(app: &CosmicLlmApp) -> Element<Message> {
    // Load the actual MCP config (same as startup)
    let mcp_config =
        crate::config::MCPConfig::load_from_json().unwrap_or_else(|_| app.config.mcp.clone());

    // Get registry data
    let (tools_by_server, all_server_names) = {
        if let Ok(registry) = app.mcp_registry.try_read() {
            let tools_by_server = registry.get_tools_by_server();
            let all_server_names = registry.get_all_server_names(&mcp_config);
            (tools_by_server, all_server_names)
        } else {
            (
                std::collections::HashMap::new(),
                mcp_config.servers.keys().cloned().collect::<Vec<_>>(),
            )
        }
    };

    let server_count = all_server_names.len();
    let total_tools: usize = tools_by_server.values().map(|tools| tools.len()).sum();
    let enabled_tools_count: usize = app
        .available_mcp_tools
        .iter()
        .filter(|tool| app.tool_states.get(&tool.name).copied().unwrap_or(true))
        .count();

    // Build server list
    let mut server_column = cosmic::widget::column::with_capacity(all_server_names.len());
    for server_name in &all_server_names {
        let server_name_clone = server_name.clone();
        let tools = tools_by_server.get(server_name).cloned().unwrap_or_default();
        let tool_count = tools.len();
        
        // Count enabled tools for this server - determine server enabled state from actual tool states
        let enabled_count = tools
            .iter()
            .filter(|tool| app.tool_states.get(&tool.name).copied().unwrap_or(true))
            .count();
        
        // Server is enabled if it has at least one enabled tool
        // This way we always reflect the actual state, not just the profile config
        let is_server_enabled = enabled_count > 0;
        
        // Check if this server is expanded
        let is_expanded = app.expanded_mcp_servers.contains(server_name);
        let expand_icon = if is_expanded { "▼" } else { "▶" };

        // Server header row with toggle and expand/collapse
        let header_row = cosmic::widget::row::with_capacity(5)
            .push(
                cosmic::widget::toggler(is_server_enabled).on_toggle(
                    move |enabled| Message::ToggleMCPServerEnabled(server_name_clone.clone(), enabled),
                )
            )
            .push(
                button::text(expand_icon)
                    .on_press(Message::ToggleMCPServer(server_name.clone()))
                    .class(cosmic::style::Button::Text),
            )
            .push(cosmic::widget::text(server_name.clone()).size(16))
            .push(cosmic::widget::Space::with_width(Length::Fill))
            .push(
                cosmic::widget::text(format!("{} / {} tools", enabled_count, tool_count))
                    .size(12)
                    .class(cosmic::style::Text::Color(
                        cosmic::iced::Color::from_rgb(0.6, 0.6, 0.6),
                    )),
            )
            .align_y(cosmic::iced::Alignment::Center)
            .spacing(8);

        let mut server_widget = cosmic::widget::column::with_capacity(2)
            .push(header_row);

        // Show tools only when expanded
        if is_expanded && !tools.is_empty() {
            let tool_widgets: Vec<Element<Message>> = tools
                .iter()
                .map(|tool| {
                    let tool_name = tool.name.clone();
                    let tool_description = tool.description.clone();
                    let is_enabled = app.tool_states.get(&tool.name).copied().unwrap_or(true);
                    container(
                        cosmic::widget::column::with_capacity(2)
                            .push(
                                cosmic::widget::row::with_capacity(2)
                                    .push(
                                        cosmic::widget::text(tool_name)
                                            .size(12)
                                            .class(if is_enabled {
                                                cosmic::style::Text::Default
                                            } else {
                                                cosmic::style::Text::Color(
                                                    cosmic::iced::Color::from_rgb(0.5, 0.5, 0.5),
                                                )
                                            }),
                                    )
                                    .push(cosmic::widget::Space::with_width(Length::Fill))
                                    .align_y(cosmic::iced::Alignment::Center),
                            )
                            .push(
                                cosmic::widget::text(tool_description)
                                    .size(11)
                                    .class(cosmic::style::Text::Color(
                                        cosmic::iced::Color::from_rgb(0.6, 0.6, 0.6),
                                    )),
                            )
                            .spacing(2),
                    )
                    .padding(Padding::from([4, 8, 4, 24]))
                    .into()
                })
                .collect();
            
            let mut tools_column = cosmic::widget::column::with_capacity(tool_widgets.len());
            for tool_widget in tool_widgets {
                tools_column = tools_column.push(tool_widget);
            }
            server_widget = server_widget.push(
                container(tools_column.spacing(2))
                    .padding(Padding::from([4, 0, 0, 0])),
            );
        }

        server_column = server_column.push(
            container(server_widget)
                .padding(12)
                .class(cosmic::style::Container::Card),
        );
    }

    cosmic::widget::column::with_capacity(3)
        .push(
            // Header with summary
            cosmic::widget::container(
                cosmic::widget::column::with_capacity(2)
                    .push(
                        cosmic::widget::text(format!(
                            "🔧 Tools: {} / {} enabled",
                            enabled_tools_count, total_tools
                        ))
                        .size(16),
                    )
                    .push(
                        cosmic::widget::text(format!(
                            "{} servers configured",
                            server_count
                        ))
                        .size(12)
                        .class(cosmic::style::Text::Color(
                            cosmic::iced::Color::from_rgb(0.6, 0.6, 0.6),
                        )),
                    )
                    .spacing(4),
            )
            .padding(16)
            .class(cosmic::style::Container::Card),
        )
        .push(
            // Server list
            if server_count == 0 {
                Element::from(
                    cosmic::widget::container(
                        cosmic::widget::column::with_capacity(2)
                            .push(cosmic::widget::text("No tools available").size(14))
                            .push(
                                cosmic::widget::text("Configure MCP servers to see tools here")
                                    .size(12)
                                    .class(cosmic::style::Text::Color(
                                        cosmic::iced::Color::from_rgb(0.6, 0.6, 0.6),
                                    )),
                            )
                            .spacing(4),
                    )
                    .padding(16)
                    .class(cosmic::style::Container::Card),
                )
            } else {
                Element::from(scrollable(server_column).spacing(8))
            },
        )
        .spacing(8)
        .into()
}
