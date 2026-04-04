//! Chat page module
//!
//! Matches the original app's chat page layout.

pub mod input_area;
pub mod message_list;
pub mod top_panel;

use cosmic::Element;
use cosmic::widget::{self, text_editor};
use cosmic::iced::Length;

use crate::ui::app::{LunaThinApp, Message, ConnectionStatus};

/// State for chat page UI elements
#[derive(Debug)]
pub struct ChatPageState {
    pub scrollable_id: widget::Id,
    pub input_id: widget::Id,
    pub input_content: text_editor::Content,
    pub typing_indicator_progress: f32,
}

impl Default for ChatPageState {
    fn default() -> Self {
        Self {
            scrollable_id: widget::Id::unique(),
            input_id: widget::Id::unique(),
            input_content: text_editor::Content::new(),
            typing_indicator_progress: 0.0,
        }
    }
}

impl Clone for ChatPageState {
    fn clone(&self) -> Self {
        Self {
            scrollable_id: self.scrollable_id.clone(), // Preserve widget ID
            input_id: self.input_id.clone(), // Preserve widget ID
            input_content: text_editor::Content::with_text(&self.input_content.text()),
            typing_indicator_progress: self.typing_indicator_progress,
        }
    }
}

/// Main chat page view (matches original app layout)
pub fn chat_page<'a>(app: &'a LunaThinApp) -> Element<'a, Message> {
    let mut layout = widget::column::with_capacity(6);

    // Top panel (with combined tools)
    layout = layout.push(top_panel::top_panel(app));

    // Error banner if present
    if let Some(ref error) = app.inline_error {
        layout = layout
            .push(widget::Space::new().height(Length::Fixed(8.0)))
            .push(crate::ui::widgets::error_banner(error, Message::DismissError));
    }

    // Info banner if present (e.g. summarization started/finished)
    if let Some(ref info) = app.inline_info {
        layout = layout
            .push(widget::Space::new().height(Length::Fixed(8.0)))
            .push(crate::ui::widgets::info_banner(info, Message::DismissInfo));
    }

    // Messages area - takes all remaining space
    layout = layout.push(
        widget::container(message_list::message_list(app))
            .height(Length::Fill)
            .width(Length::Fill)
            .padding([8, 0, 8, 0]), // Top/bottom padding
    );

    // Input area (only if connected)
    if app.connection_status == ConnectionStatus::Connected {
        layout = layout.push(input_area::input_area(app));
    }

    // Wrap in container with horizontal padding
    widget::container(layout)
        .height(Length::Fill)
        .width(Length::Fill)
        .padding([0, 12, 12, 12]) // No top (header handles it), sides, bottom
        .into()
}

