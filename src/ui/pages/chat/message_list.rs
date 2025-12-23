use crate::ui::app::{CosmicLlmApp, Message, ToolCallStatus};
use crate::ui::widgets::ToolCallWidget;
use cosmic::{
    iced::{Font, Length, Padding},
    widget::{self, markdown, scrollable},
    Element,
};

pub fn message_list(app: &CosmicLlmApp) -> Element<Message> {
    // Show empty state placeholder when there are no messages and not streaming
    if app.messages.is_empty() && !app.is_streaming {
        return cosmic::widget::container(
            cosmic::widget::column()
                .spacing(8)
                .push(
                    cosmic::widget::text("🪄")
                        .size(48)
                )
                .push(
                    cosmic::widget::text("Ready to help")
                        .size(18)
                )
                .push(
                    cosmic::widget::text("Start typing below to begin the agentic loop.")
                        .size(14)
                        .class(cosmic::style::Text::Color(cosmic::iced::Color::from_rgb(0.6, 0.6, 0.6)))
                )
                .align_x(cosmic::iced::Alignment::Center)
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(cosmic::iced::Alignment::Center)
        .align_y(cosmic::iced::Alignment::Center)
        .into();
    }

    let mut column = cosmic::widget::column::with_capacity(app.messages.len()).spacing(0);

    // Add regular chat messages
    for (i, msg) in app.messages.iter().enumerate() {
        // Check if this is a summary message - render it differently
        if msg.is_summary {
            let expanded = app.expanded_summaries.contains(&i);
            let summary_count = msg.summarized_count.unwrap_or(0);
            
            let toggle_button = cosmic::widget::button::text(format!(
                "{} 📄 Summary ({} messages)",
                if expanded { "▼" } else { "▶" },
                summary_count
            ))
            .on_press(Message::ToggleSummary(i))
            .class(cosmic::style::Button::Text)
            .width(Length::Fill);
            
            let mut summary_column = cosmic::widget::column()
                .push(toggle_button);
            
            if expanded {
                // Show summary content when expanded
                summary_column = summary_column.push(
                    widget::container(widget::lazy(&msg.content, |_| {
                        let items = markdown::parse(&msg.content).collect::<Vec<_>>();
                        let style = widget::markdown::Style {
                            inline_code_padding: cosmic::iced::Padding::from([1, 2]),
                            inline_code_highlight: widget::markdown::Highlight {
                                background: cosmic::iced::Background::Color(
                                    cosmic::iced::Color::from_rgb(0.1, 0.1, 0.1),
                                ),
                                border: cosmic::iced::Border::default().rounded(2),
                            },
                            inline_code_color: cosmic::iced::Color::WHITE,
                            link_color: cosmic::iced::Color::from_rgb(0.3, 0.6, 1.0),
                        };
                        widget::markdown(&items, widget::markdown::Settings::default(), style)
                            .map(Message::MarkdownLinkClicked)
                    }))
                    .width(Length::Fill)
                    .padding(Padding::from([8, 12]))
                );
            }
            
            let summary_widget = cosmic::widget::container(summary_column)
                .padding(Padding::from([12, 16]))
                .class(cosmic::style::Container::Card)
                .width(Length::Fill); // 100% width for summary messages
            
            // Summary messages span full width, centered
            let summary_row = cosmic::widget::row::with_capacity(1)
                .push(summary_widget);
            
            column = column.push(summary_row);
            continue; // Skip regular message rendering for summaries
        }
        
        let content = msg.content.clone();
        let mut tool_summaries: Vec<(String, String, String)> = Vec::new();
        if !msg.is_user {
            for anchored in app
                .archived_tool_calls
                .iter()
                .filter(|t| t.anchor_index == i)
            {
                let summary_id = anchored
                    .tool_call
                    .id
                    .clone()
                    .unwrap_or_else(|| format!("archived-{}-{}", i, tool_summaries.len()));
                tool_summaries.push((
                    summary_id,
                    anchored.tool_call.tool_name.clone(),
                    anchored.tool_call.parameters.clone(),
                ));
            }
            if let Some(current_anchor) = app.current_ai_message_index {
                if current_anchor == i {
                    for active in &app.active_tool_calls {
                        let summary_id = active
                            .id
                            .clone()
                            .unwrap_or_else(|| format!("active-{}-{}", i, tool_summaries.len()));
                        tool_summaries.push((
                            summary_id,
                            active.tool_name.clone(),
                            active.parameters.clone(),
                        ));
                    }
                }
            }
        }

        // Determine adjacent messages for styling
        let prev_msg = if i > 0 { app.messages.get(i - 1) } else { None };
        let next_msg = app.messages.get(i + 1);
        
        // Calculate border radius and margins for AI messages
        let (top_radius, bottom_left_radius, bottom_right_radius, top_margin, bottom_margin) = if msg.is_user {
            // User messages: bottom-right radius is 0
            (20.0, 20.0, 0.0, 12.0, 12.0) // top-left, top-right, bottom-left rounded; bottom-right = 0
        } else {
            // AI messages: conditional radius and margins
            let prev_is_user = prev_msg.map(|m| m.is_user).unwrap_or(false);
            let next_is_user = next_msg.map(|m| m.is_user).unwrap_or(true); // true if no next message
            let has_next = next_msg.is_some();
            
            let top_r = if prev_is_user { 20.0 } else { 0.0 };
            let bottom_left_r = 0.0; // Always 0 for AI messages
            let bottom_right_r = if !has_next || next_is_user { 20.0 } else { 0.0 };
            let top_m = if prev_is_user { 12.0 } else { 0.0 };
            let bottom_m = if !has_next || next_is_user { 12.0 } else { 0.0 };
            
            (top_r, bottom_left_r, bottom_right_r, top_m, bottom_m)
        };

        let message_widget = cosmic::widget::container({
            let content_widget: Element<Message> = if msg.is_user {
                widget::container(
                    cosmic::widget::text(&msg.content)
                        .size(14)
                        .class(cosmic::style::Text::Color(cosmic::theme::active().cosmic().on_bg_color().into())),
                )
                .width(Length::Fill)
                .into()
            } else if msg.is_error {
                widget::container(cosmic::widget::text(&msg.content).size(14).class(
                    cosmic::style::Text::Color(cosmic::iced::Color::from_rgb(0.8, 0.2, 0.2)),
                ))
                .width(Length::Fill)
                .into()
            } else {
                widget::container(widget::lazy(&msg.content, |_| {
                    let items = markdown::parse(&msg.content).collect::<Vec<_>>();
                    let style = widget::markdown::Style {
                        inline_code_padding: cosmic::iced::Padding::from([1, 2]),
                        inline_code_highlight: widget::markdown::Highlight {
                            background: cosmic::iced::Background::Color(
                                cosmic::iced::Color::from_rgb(0.1, 0.1, 0.1),
                            ),
                            border: cosmic::iced::Border::default().rounded(2),
                        },
                        inline_code_color: cosmic::iced::Color::WHITE,
                        link_color: cosmic::iced::Color::from_rgb(0.3, 0.6, 1.0),
                    };
                    widget::markdown(&items, widget::markdown::Settings::default(), style)
                        .map(Message::MarkdownLinkClicked)
                }))
                .width(Length::Fill)
                .into()
            };

            let mut column = cosmic::widget::column::with_capacity(1).push(content_widget);
            for (summary_id, label, params) in tool_summaries {
                let key = (i, summary_id.clone());
                let expanded = app.expanded_tool_summaries.contains(&key);
                let toggle = cosmic::widget::button::text(format!(
                    "{} {}",
                    if expanded { "▼" } else { "▶" },
                    label
                ))
                .on_press(Message::ToggleToolSummary(i, summary_id.clone()))
                .class(cosmic::style::Button::Text)
                .width(Length::Fill);

                column = column.push(toggle);

                if expanded {
                    column = column.push(
                        cosmic::widget::text(params.clone())
                            .size(12)
                            .font(Font::MONOSPACE)
                            .class(cosmic::style::Text::Color(cosmic::iced::Color::from_rgb(
                                0.7, 0.7, 0.7,
                            )))
                            .width(Length::Fill),
                    );
                }
            }

            // Add reasoning content display for AI messages (show during streaming too)
            if !msg.is_user && !msg.is_error {
                if let Some(ref reasoning) = msg.reasoning_content {
                    if !reasoning.is_empty() {
                        let expanded = app.expanded_reasoning.contains(&i);
                        // Auto-expand during streaming if not manually collapsed
                        let should_show = expanded || (app.is_streaming && app.current_ai_message_index == Some(i));
                        let toggle = cosmic::widget::button::text(format!(
                            "{} 💭 Thinking",
                            if should_show { "▼" } else { "▶" }
                        ))
                        .on_press(Message::ToggleReasoning(i))
                        .class(cosmic::style::Button::Text)
                        .width(Length::Fill);

                        column = column.push(toggle);

                        if should_show {
                            column = column.push(
                                cosmic::widget::container(
                                    cosmic::widget::text(reasoning.clone())
                                        .size(12)
                                        .font(Font::MONOSPACE)
                                        .class(cosmic::style::Text::Color(cosmic::iced::Color::from_rgb(
                                            0.6, 0.7, 0.9,
                                        )))
                                        .width(Length::Fill),
                                )
                                .padding(Padding::from([8, 12]))
                                .class(cosmic::style::Container::Card)
                                .width(Length::Fill),
                            );
                        }
                    }
                }
            }

            cosmic::widget::row::with_capacity(2).push(column).push(
                cosmic::widget::button::text("📋")
                    .on_press(Message::ShowMessageDialog(content))
                    .padding(4)
                    .class(cosmic::style::Button::Text),
            )
        })
        .padding(Padding::from([12, 16]))
        .style(move |theme| {
            if msg.is_user {
                // User message: custom background and border radius
                cosmic::widget::container::Style {
                    background: Some(cosmic::iced::Background::Color(
                        theme.cosmic().primary.component.hover.into()
                    )),
                    border: cosmic::iced::Border {
                        width: 0.0,
                        color: cosmic::iced::Color::TRANSPARENT,
                        radius: cosmic::iced::border::Radius {
                            top_left: 20.0,
                            top_right: 20.0,
                            bottom_left: 20.0,
                            bottom_right: 0.0, // Not rounded on right bottom
                        },
                    },
                    ..Default::default()
                }
            } else {
                // AI message: conditional border radius with background
                cosmic::widget::container::Style {
                    background: Some(cosmic::iced::Background::Color(
                        theme.cosmic().background.component.hover.into()
                    )),
                    border: cosmic::iced::Border {
                        width: 0.0,
                        color: cosmic::iced::Color::TRANSPARENT,
                        radius: cosmic::iced::border::Radius {
                            top_left: top_radius,
                            top_right: top_radius,
                            bottom_left: bottom_left_radius,
                            bottom_right: bottom_right_radius,
                        },
                    },
                    ..Default::default()
                }
            }
        })
        .width(Length::FillPortion(7)); // 70% width

        // Wrap message widget with margins
        let message_with_margins = if msg.is_user {
            cosmic::widget::container(message_widget)
                .width(Length::FillPortion(7)) // Same width as tool results
                .padding(cosmic::iced::Padding {
                    top: 12.0,
                    bottom: 12.0,
                    left: 0.0,
                    right: 0.0,
                })
        } else {
            cosmic::widget::container(message_widget)
                .width(Length::FillPortion(7)) // Same width as tool results
                .padding(cosmic::iced::Padding {
                    top: top_margin,
                    bottom: bottom_margin,
                    left: 0.0,
                    right: 0.0,
                })
        };

        let message_row = if msg.is_user {
            // User messages: right-aligned
            cosmic::widget::row::with_capacity(2)
                .push(cosmic::widget::Space::with_width(Length::FillPortion(3)))
                .push(message_with_margins)
        } else {
            // AI messages: left-aligned
            cosmic::widget::row::with_capacity(2)
                .push(message_with_margins)
                .push(cosmic::widget::Space::with_width(Length::FillPortion(3)))
        };
        // Push the message first
        column = column.push(message_row);
        // If there are archived tool calls anchored to this message, render them right after
        for (idx, anchored) in app.archived_tool_calls.iter().enumerate() {
            if anchored.anchor_index == i {
                let is_expanded = app.expanded_tool_calls.contains(&idx);
                let tool_call = &anchored.tool_call;
                let tool_name = tool_call.tool_name.clone();
                let parameters = tool_call.parameters.clone();
                let status = match tool_call.status {
                    ToolCallStatus::Started => crate::ui::widgets::ToolCallStatus::Started,
                    ToolCallStatus::Completed => crate::ui::widgets::ToolCallStatus::Completed,
                    ToolCallStatus::Error => crate::ui::widgets::ToolCallStatus::Error,
                };
                let result = tool_call.result.clone();
                let error = tool_call.error.clone();
                let widget = Box::leak(Box::new(ToolCallWidget {
                    tool_name,
                    parameters,
                    status,
                    result,
                    error,
                    is_expanded,
                }));
                let widget_element = widget
                    .content()
                    .map(move |msg| Message::ToolCallWidgetMessage(idx, msg));
                // Tool results: same width as messages, left-aligned, no margins, radius 0
                let tool_widget = cosmic::widget::container(widget_element)
                    .width(Length::FillPortion(7))
                    .style(move |theme| {
                        cosmic::widget::container::Style {
                            background: Some(cosmic::iced::Background::Color(
                                theme.cosmic().background.component.hover.into()
                            )),
                            border: cosmic::iced::Border {
                                width: 0.0,
                                color: cosmic::iced::Color::TRANSPARENT,
                                radius: cosmic::iced::border::Radius {
                                    top_left: 0.0,
                                    top_right: 0.0,
                                    bottom_left: 0.0,
                                    bottom_right: 0.0,
                                },
                            },
                            ..Default::default()
                        }
                    });
                let tool_call_row = cosmic::widget::row::with_capacity(2)
                    .push(tool_widget)
                    .push(cosmic::widget::Space::with_width(Length::FillPortion(3)));
                column = column.push(tool_call_row);
            }
        }
        // If we're on the currently streaming AI message, also render active tool calls inline
        if let Some(anchor) = app.current_ai_message_index {
            if anchor == i {
                let offset = app.archived_tool_calls.len();
                for (j, tool_call) in app.active_tool_calls.iter().enumerate() {
                    let idx = offset + j;
                    let is_expanded = app.expanded_tool_calls.contains(&idx);
                    let tool_name = tool_call.tool_name.clone();
                    let parameters = tool_call.parameters.clone();
                    let status = match tool_call.status {
                        ToolCallStatus::Started => crate::ui::widgets::ToolCallStatus::Started,
                        ToolCallStatus::Completed => crate::ui::widgets::ToolCallStatus::Completed,
                        ToolCallStatus::Error => crate::ui::widgets::ToolCallStatus::Error,
                    };
                    let result = tool_call.result.clone();
                    let error = tool_call.error.clone();
                    let widget = Box::leak(Box::new(ToolCallWidget {
                        tool_name,
                        parameters,
                        status,
                        result,
                        error,
                        is_expanded,
                    }));
                    let widget_element = widget
                        .content()
                        .map(move |msg| Message::ToolCallWidgetMessage(idx, msg));
                    // Tool results: same width as messages, left-aligned, no margins, radius 0
                    let tool_widget = cosmic::widget::container(widget_element)
                        .width(Length::FillPortion(7))
                        .style(move |theme| {
                            cosmic::widget::container::Style {
                                background: Some(cosmic::iced::Background::Color(
                                    theme.cosmic().background.component.hover.into()
                                )),
                                border: cosmic::iced::Border {
                                    width: 0.0,
                                    color: cosmic::iced::Color::TRANSPARENT,
                                    radius: cosmic::iced::border::Radius {
                                        top_left: 0.0,
                                        top_right: 0.0,
                                        bottom_left: 0.0,
                                        bottom_right: 0.0,
                                    },
                                },
                                ..Default::default()
                            }
                        });
                    let tool_call_row = cosmic::widget::row::with_capacity(2)
                        .push(tool_widget)
                        .push(cosmic::widget::Space::with_width(Length::FillPortion(3)));
                    column = column.push(tool_call_row);
                }
                // If there are no active tool calls yet, but we're streaming, show typing indicator
                if app.active_tool_calls.is_empty() && app.is_streaming {
                    use crate::ui::widgets::typing_indicator;
                    let indicator_widget = cosmic::widget::container(
                        typing_indicator(app.typing_indicator_progress)
                            .map(|_| Message::ScrollToBottom)
                    )
                    .width(Length::FillPortion(7)); // 70% width like AI messages
                    // AI messages: left-aligned
                    let row = cosmic::widget::row::with_capacity(2)
                        .push(indicator_widget)
                        .push(cosmic::widget::Space::with_width(Length::FillPortion(3)));
                    column = column.push(row);
                }
            }
        }
    }

    // Show typing indicator at bottom if streaming and no messages yet, or after all messages
    if app.is_streaming && app.active_tool_calls.is_empty() {
        // Check if we already added it inside the loop
        let already_shown = app.current_ai_message_index.is_some() && 
            app.current_ai_message_index.map(|idx| idx < app.messages.len()).unwrap_or(false);
        
        if !already_shown {
            use crate::ui::widgets::typing_indicator;
            let indicator_widget = cosmic::widget::container(
                typing_indicator(app.typing_indicator_progress)
                    .map(|_| Message::ScrollToBottom)
            )
            .width(Length::FillPortion(7)); // 70% width like AI messages
            // AI messages: left-aligned
            let row = cosmic::widget::row::with_capacity(2)
                .push(indicator_widget)
                .push(cosmic::widget::Space::with_width(Length::FillPortion(3)));
            column = column.push(row);
        }
    }

    // Add spacer at bottom to force scroll to bottom
    column =
        column.push(cosmic::widget::Space::with_height(Length::Fixed(1.0)).width(Length::Fill));

    scrollable(column)
        .scrollbar_width(8)
        .scrollbar_padding(4)
        .id(app.scrollable_id.clone())
        .anchor_bottom()
        .into()
}
