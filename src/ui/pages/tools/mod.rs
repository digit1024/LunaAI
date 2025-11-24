use cosmic::{Element, widget::scrollable};
use crate::ui::app::{Message, CosmicLlmApp};

pub fn tools_context_view(app: &CosmicLlmApp) -> Element<Message> {
    let total_tools = app.available_mcp_tools.len();
    let enabled_count = app.available_mcp_tools.iter()
        .filter(|tool| app.tool_states.get(&tool.name).copied().unwrap_or(true))
        .count();
    
    cosmic::widget::column::with_capacity(3)
        .push(
            // Header with summary and controls
            cosmic::widget::container(
                cosmic::widget::column::with_capacity(2)
                    .push(
                        cosmic::widget::text(format!("🔧 Tools: {} / {} enabled", enabled_count, total_tools))
                            .size(16)
                    )
                    .push(
                        cosmic::widget::row::with_capacity(2)
                            .push(
                                cosmic::widget::button::text("Enable All")
                                    .on_press(Message::ToggleAllTools(true))
                                    .padding(6)
                                    .class(cosmic::style::Button::Text)
                            )
                            .push(
                                cosmic::widget::button::text("Disable All")
                                    .on_press(Message::ToggleAllTools(false))
                                    .padding(6)
                                    .class(cosmic::style::Button::Text)
                            )
                            .spacing(8)
                    )
                    .spacing(8)
            )
            .padding(16)
            .class(cosmic::style::Container::Card)
        )
        .push(
            // Tool list
            if app.available_mcp_tools.is_empty() {
                Element::from(
                    cosmic::widget::container(
                        cosmic::widget::column::with_capacity(2)
                            .push(
                                cosmic::widget::text("No tools available")
                                    .size(14)
                            )
                            .push(
                                cosmic::widget::text("Configure MCP servers to see tools here")
                                    .size(12)
                                    .class(cosmic::style::Text::Color(cosmic::iced::Color::from_rgb(0.6, 0.6, 0.6)))
                            )
                            .spacing(4)
                    )
                    .padding(16)
                    .class(cosmic::style::Container::Card)
                )
            } else {
                let mut tool_list = cosmic::widget::column::with_capacity(app.available_mcp_tools.len())
                    .spacing(4);
                
                for tool in &app.available_mcp_tools {
                    let is_enabled = app.tool_states.get(&tool.name).copied().unwrap_or(true);
                    let tool_row = cosmic::widget::container(
                        cosmic::widget::column::with_capacity(3)
                            .push(
                                cosmic::widget::row::with_capacity(2)
                                    .push(
                                        cosmic::widget::toggler(is_enabled)
                                            .on_toggle(|enabled| Message::ToggleTool(tool.name.clone(), enabled))
                                    )
                                    .push(
                                        cosmic::widget::text(&tool.name)
                                            .size(14)
                                            .class(if is_enabled {
                                                cosmic::style::Text::Default
                                            } else {
                                                cosmic::style::Text::Color(cosmic::iced::Color::from_rgb(0.5, 0.5, 0.5))
                                            })
                                    )
                                    .spacing(8)
                                    .align_y(cosmic::iced::Alignment::Center)
                            )
                            .push(
                                cosmic::widget::text(&tool.description)
                                    .size(12)
                                    .class(cosmic::style::Text::Color(cosmic::iced::Color::from_rgb(0.6, 0.6, 0.6)))
                            )
                            .spacing(4)
                    )
                    .padding(12)
                    .class(cosmic::style::Container::Card);
                    
                    tool_list = tool_list.push(tool_row);
                }
                
                cosmic::widget::scrollable(tool_list)
                    .into()
            }
        )
        .spacing(8)
        .into()
}
