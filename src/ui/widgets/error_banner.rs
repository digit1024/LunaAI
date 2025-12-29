//! Error banner widget
//!
//! Extracted from app.rs for better modularity.

use crate::ui::app::Message;
use cosmic::{Element, widget};

/// Create an error banner widget
pub fn error_banner(error: &str) -> Element<Message> {
    let content = widget::row::with_children(vec![
        crate::ui::icons::get_icon("dialog-warning-symbolic", 16).into(),
        widget::text(error).size(14).into(),
        widget::Space::with_width(cosmic::iced::Length::Fill).into(),
        widget::button::standard("Dismiss")
            .on_press(Message::DismissError)
            .padding([4, 12])
            .into(),
    ])
    .spacing(12)
    .align_y(cosmic::iced::Alignment::Center);

    widget::container(content)
        .padding(12)
        .width(cosmic::iced::Length::Fill)
        .class(cosmic::style::Container::Card)
        .into()
}

