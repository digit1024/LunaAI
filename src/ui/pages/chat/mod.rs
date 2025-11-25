pub mod message_list;
pub mod input_area;
pub mod top_panel;

use cosmic::{Element, iced::Length};
use crate::ui::app::{Message, CosmicLlmApp};

pub fn chat_view(app: &CosmicLlmApp) -> Element<Message> {
    let mut layout = cosmic::widget::column::with_capacity(6)
        .push(
            // Combined top panel with tools
            top_panel::top_panel(app)
        )
        .push(
            // Spacing between top panel and messages
            cosmic::widget::Space::with_height(Length::Fixed(16.0))
        );

    if let Some(error_banner) = app.error_banner() {
        layout = layout
            .push(error_banner)
            .push(cosmic::widget::Space::with_height(Length::Fixed(12.0)));
    }

    layout
        .push(
            // Messages area with better styling
            cosmic::widget::container(message_list::message_list(app))
                .height(Length::Fill)
                .width(Length::Fill)
        )
        .push(
            // Spacing between messages and input area
            cosmic::widget::Space::with_height(Length::Fixed(16.0))
        )
        .push(
            // Input area with better styling
            input_area::input_area(app)
        )
        .into()
}
