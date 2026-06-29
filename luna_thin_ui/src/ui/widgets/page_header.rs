//! Shared header for sidebar sub-pages (History, Memories, MCP, Settings).

use cosmic::{
    iced::{alignment, Length},
    widget::{button, container, icon, text, Row, Space},
    Element,
};

use crate::ui::app::Message;
use crate::ui::icons;

/// Title row with back-to-chat control and optional trailing summary text.
pub fn subpage_header(
    title: &str,
    icon_name: &str,
    trailing: Option<String>,
) -> Element<'static, Message> {
    let mut header = Row::new()
        .push(
            button::icon(icons::get_handle("arrow1-left-symbolic", 16))
                .on_press(Message::BackToChat)
                .class(cosmic::style::Button::Text)
                .padding(4),
        )
        .push(Space::new().width(8))
        .push(icon::from_name(icon_name).size(20))
        .push(Space::new().width(8))
        .push(text(title.to_string()).size(20))
        .push(Space::new().width(Length::Fill))
        .spacing(0)
        .align_y(alignment::Vertical::Center)
        .width(Length::Fill);

    if let Some(summary) = trailing {
        header = header.push(
            text(summary)
                .size(12)
                .class(cosmic::style::Text::Color(cosmic::iced::Color::from_rgb(
                    0.6, 0.6, 0.6,
                ))),
        );
    }

    container(header)
        .padding(16)
        .width(Length::Fill)
        .class(cosmic::style::Container::Card)
        .into()
}
