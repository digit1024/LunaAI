//! Popover content listing memories recalled for a user message turn.

use chrono::{TimeZone, Utc};
use cosmic::{
    iced::Length,
    widget::{container, scrollable, text, Column, Row},
    Element,
};

use crate::server::dto::MemoryView;
use crate::ui::app::Message;

pub fn popup(memories: &[MemoryView]) -> Element<'static, Message> {
    let mut list = Column::new().spacing(8).width(Length::Fill);

    list = list.push(
        container(text("Recalled memories").size(12))
            .padding([0, 4])
            .width(Length::Fill),
    );

    for memory in memories {
        list = list.push(memory_card(memory));
    }

    container(
        scrollable(list.padding(4))
            .height(Length::Fixed(280.0))
            .width(Length::Fixed(320.0)),
    )
    .padding(4)
    .class(cosmic::style::Container::Card)
    .into()
}

fn memory_card(memory: &MemoryView) -> Element<'static, Message> {
    let category_label = memory
        .category
        .as_deref()
        .filter(|c| !c.is_empty())
        .map(|c| format!("[{c}] "))
        .unwrap_or_default();

    let date_str = Utc
        .timestamp_opt(memory.updated_at, 0)
        .single()
        .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_default();

    container(
        Column::new()
            .push(
                Row::new()
                    .push(text(format!("{category_label}Memory")).size(13).width(Length::Fill))
                    .push(
                        text(date_str).size(11).class(cosmic::style::Text::Color(
                            cosmic::iced::Color::from_rgb(0.6, 0.6, 0.6),
                        )),
                    )
                    .spacing(8),
            )
            .push(
                text(format!("Importance: {}", memory.importance)).size(11).class(
                    cosmic::style::Text::Color(cosmic::iced::Color::from_rgb(0.5, 0.5, 0.5)),
                ),
            )
            .push(text(memory.content.clone()).size(13))
            .spacing(6),
    )
    .padding(10)
    .width(Length::Fill)
    .style(|theme| cosmic::widget::container::Style {
        background: Some(cosmic::iced::Background::Color(
            theme.cosmic().background.component.hover.into(),
        )),
        border: cosmic::iced::Border {
            color: cosmic::iced::Color::from_rgb(0.35, 0.35, 0.35),
            width: 1.0,
            radius: 6.0.into(),
        },
        ..Default::default()
    })
    .into()
}
