//! Message list - scrollable list of chat messages
//!
//! Matches the mobile app's approach - tool calls are separate bubbles.

use cosmic::{
    iced::{Length, Padding},
    widget::{column, container, row, scrollable, text, Space},
    Element,
};

use crate::ui::app::{BubbleType, ChatMessage, LunaThinApp, Message};
use crate::ui::widgets::{message_bubble, typing_indicator, BubbleContext, ToolCallWidget, ToolCallStatus};

/// Check if a message is "assistant-like" for styling purposes (not user)
fn is_assistant_like(msg: &ChatMessage) -> bool {
    matches!(msg.bubble_type, BubbleType::Assistant | BubbleType::ToolRequest | BubbleType::ToolResult | BubbleType::Summary)
}

/// Build the message list view
pub fn message_list(app: &LunaThinApp) -> Element<Message> {
    // Empty state
    if app.messages.is_empty() && !app.is_streaming {
        return container(
            column()
                .push(text("🪄").size(48))
                .push(text("Ready to help").size(18))
                .push(
                    text("Start typing below to begin the agentic loop.")
                        .size(14)
                        .class(cosmic::style::Text::Color(cosmic::iced::Color::from_rgb(
                            0.6, 0.6, 0.6,
                        ))),
                )
                .spacing(8)
                .align_x(cosmic::iced::Alignment::Center),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(cosmic::iced::Alignment::Center)
        .align_y(cosmic::iced::Alignment::Center)
        .into();
    }

    let mut col = column().spacing(0);

    // Filter out empty messages and render based on bubble type
    // Use actual message indices for context calculation
    for (i, msg) in app.messages.iter().enumerate() {
        // Skip empty messages (except tool calls which have their own content)
        if matches!(msg.bubble_type, BubbleType::User | BubbleType::Assistant | BubbleType::Summary) {
            if msg.content.trim().is_empty() {
                continue; // Skip empty messages
            }
        }
        
        match msg.bubble_type {
            BubbleType::User | BubbleType::Assistant | BubbleType::Summary => {
                col = col.push(render_message_bubble(app, msg, i));
            }
            BubbleType::ToolRequest => {
                // Single tool bubble - shows params and result when available
                col = col.push(render_tool_bubble(app, msg, i));
            }
            BubbleType::ToolResult => {
                // ToolResult type is no longer used (single bubble approach)
                // but handle it for backwards compatibility
                col = col.push(render_tool_bubble(app, msg, i));
            }
        }
    }

    // Typing indicator when streaming but no content yet
    if app.is_streaming {
        let has_running_tools = app.messages.iter().any(|m| {
            m.bubble_type == BubbleType::ToolRequest &&
            m.tool_status.as_deref() == Some("running")
        });
        
        // Show typing indicator only if:
        // 1. No streaming assistant bubble exists (messages with is_streaming=true)
        // 2. No tools are currently running
        let has_streaming_bubble = app.messages.iter().any(|m| m.is_streaming);
        
        if !has_streaming_bubble && !has_running_tools {
            let indicator = container(
                typing_indicator(app.chat_page.typing_indicator_progress),
            )
            .width(Length::FillPortion(7));

            col = col.push(
                row()
                    .push(indicator)
                    .push(Space::with_width(Length::FillPortion(3))),
            );
        }
    }

    // Bottom spacer for scroll anchor
    col = col.push(Space::with_height(Length::Fixed(1.0)).width(Length::Fill));

    // Use anchor_bottom() for auto-scroll like original app
    scrollable(col)
        .id(app.chat_page.scrollable_id.clone())
        .scrollbar_width(8)
        .scrollbar_padding(4)
        .anchor_bottom()
        .height(Length::Fill)
        .into()
}

/// Render a user/assistant/summary message bubble
fn render_message_bubble(app: &LunaThinApp, msg: &ChatMessage, idx: usize) -> Element<'static, Message> {
    // Calculate context for smart corners
    let prev_msg = if idx > 0 { app.messages.get(idx - 1) } else { None };
    let next_msg = app.messages.get(idx + 1);

    let is_user_msg = msg.bubble_type == BubbleType::User;
    let is_summary_msg = msg.bubble_type == BubbleType::Summary;
    
    // For context, consider tool bubbles as "assistant-like" for styling
    let prev_is_user_msg = prev_msg.map(|m| m.bubble_type == BubbleType::User).unwrap_or(false);
    let prev_is_assistant_msg = prev_msg.map(|m| is_assistant_like(m)).unwrap_or(false);
    let next_is_assistant_msg = next_msg.map(|m| is_assistant_like(m)).unwrap_or(false);
    
    let ctx = BubbleContext {
        is_prev_user: prev_is_user_msg,
        is_prev_assistant: prev_is_assistant_msg,
        is_next_assistant: next_is_assistant_msg,
        has_next: next_msg.is_some(),
    };

    let is_reasoning_expanded = app.expanded_reasoning.contains(&idx);
    let is_summary_expanded = app.expanded_summaries.contains(&idx);

    message_bubble(
        &msg.content,
        is_user_msg,
        is_summary_msg,
        msg.summarized_count,
        msg.reasoning_content.as_deref(),
        is_reasoning_expanded,
        is_summary_expanded,
        ctx,
        Message::CopyMessage(msg.content.clone()),
        Some(Message::ToggleReasoning(idx)),
        Some(Message::ToggleSummary(idx)),
        // Regenerate and playback only for assistant messages (not user, not summary)
        if !is_user_msg && !is_summary_msg {
            Some(Message::RegenerateMessage(msg.id.clone()))
        } else {
            None
        },
        if !is_user_msg && !is_summary_msg {
            Some(Message::PlaybackMessage(msg.id.clone()))
        } else {
            None
        },
    )
}

/// Render a single tool bubble (shows params, status, and result when available)
/// Tool bubbles are "assistant-like" and need proper rounding like assistant messages
fn render_tool_bubble(app: &LunaThinApp, msg: &ChatMessage, idx: usize) -> Element<'static, Message> {
    let tool_call_id = msg.tool_call_id.clone().unwrap_or_default();
    let tool_name = msg.tool_name.clone().unwrap_or_else(|| "tool".to_string());
    let params = msg.tool_params.clone().unwrap_or_default();
    let status_str = msg.tool_status.clone().unwrap_or_else(|| "planned".to_string());
    let result = msg.tool_result.clone();
    let is_error = msg.is_error;
    
    // Calculate context for smart corners (tool bubbles are assistant-like)
    let prev_msg = if idx > 0 { app.messages.get(idx - 1) } else { None };
    let next_msg = app.messages.get(idx + 1);
    
    // Tool bubbles are assistant-like, so check if prev/next are user or assistant-like
    let prev_is_user = prev_msg.map(|m| m.bubble_type == BubbleType::User).unwrap_or(false);
    let next_is_user = next_msg.map(|m| m.bubble_type == BubbleType::User).unwrap_or(true); // true if no next
    let has_next = next_msg.is_some();
    
    // Calculate rounding like assistant bubbles:
    // - top_radius: 20 if prev is user, else 0
    // - bottom_left: always 0
    // - bottom_right: 20 if last or next is user, else 0
    let top_radius = if prev_is_user { 20.0 } else { 0.0 };
    let bottom_left_radius = 0.0; // Always 0 for assistant-like
    let bottom_right_radius = if !has_next || next_is_user { 20.0 } else { 0.0 };
    let top_margin = if prev_is_user { 12.0 } else { 0.0 };
    let bottom_margin = if !has_next || next_is_user { 12.0 } else { 0.0 };
    
    let is_expanded = app.expanded_tools.contains(&tool_call_id);
    let status = match status_str.as_str() {
        "planned" => ToolCallStatus::Planned,
        "running" => ToolCallStatus::Running,
        "error" => ToolCallStatus::Error,
        _ => ToolCallStatus::Completed,
    };

    let widget = ToolCallWidget {
        tool_name,
        parameters: params,
        status,
        result: if is_error { None } else { result.clone() },
        error: if is_error { result } else { None },
        is_expanded,
    };

    let tool_element = widget.content(Message::ToggleToolDetails(tool_call_id));

    // Tool bubbles: same width as messages, left-aligned, with smart corners
    let tool_widget = container(tool_element)
        .width(Length::FillPortion(7))
        .padding(Padding::from([12, 16]))
        .style(move |theme| cosmic::widget::container::Style {
            background: Some(cosmic::iced::Background::Color(
                theme.cosmic().background.component.hover.into(),
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
        });

    // Left-aligned row with margins
    row()
        .push(
            container(tool_widget)
                .width(Length::FillPortion(7))
                .padding(cosmic::iced::Padding {
                    top: top_margin,
                    bottom: bottom_margin,
                    left: 0.0,
                    right: 0.0,
                }),
        )
        .push(Space::with_width(Length::FillPortion(3)))
        .into()
}
