//! History page - conversation list with search
//!
//! Matches the original app's history page styling.

use cosmic::{
    iced::Length,
    widget::{self, button, column, container, icon, row, scrollable, text, text_input, Space},
    Element,
};

use crate::ui::app::{LunaThinApp, Message};

pub fn history_page(app: &LunaThinApp) -> Element<'static, Message> {
    let conv_count = app.conversations.len();
    
    let mut content = column().spacing(12);

    // Header card with icon and count
    content = content.push(
        container(
            row()
                .push(
                    row()
                        .push(icon::from_name("list-large-symbolic").size(20))
                        .push(text("Conversation History").size(20))
                        .spacing(8)
                        .align_y(cosmic::iced::Alignment::Center),
                )
                .push(Space::with_width(Length::Fill))
                .push(
                    text(format!("{} conversations", conv_count))
                        .size(12)
                        .class(cosmic::style::Text::Color(cosmic::iced::Color::from_rgb(
                            0.6, 0.6, 0.6,
                        ))),
                )
                .spacing(12)
                .align_y(cosmic::iced::Alignment::Center),
        )
        .padding(16)
        .width(Length::Fill)
        .class(cosmic::style::Container::Card),
    );

    // Search bar (placeholder for now - server doesn't support search yet)
    content = content.push(
        container(
            row()
                .push(icon::from_name("search-symbolic").size(16))
                .push(
                    text_input("Search conversations...", "")
                        .width(Length::Fill),
                )
                .spacing(8)
                .align_y(cosmic::iced::Alignment::Center),
        )
        .padding(12)
        .width(Length::Fill)
        .class(cosmic::style::Container::Card),
    );

    // Conversation list
    if app.conversations.is_empty() {
        // Empty state
        content = content.push(
            container(
                column()
                    .push(icon::from_name("chat-bubble-empty-symbolic").size(48))
                    .push(text("No conversations yet").size(16))
                    .push(
                        text("Start a new chat to create your first conversation!")
                            .size(12)
                            .class(cosmic::style::Text::Color(cosmic::iced::Color::from_rgb(
                                0.6, 0.6, 0.6,
                            ))),
                    )
                    .spacing(8)
                    .align_x(cosmic::iced::Alignment::Center),
            )
            .padding(32)
            .width(Length::Fill)
            .class(cosmic::style::Container::Card),
        );
    } else {
        let mut list = column().spacing(8);

        for conv in &app.conversations {
            let is_selected = Some(&conv.id) == app.current_conversation_id.as_ref();

            // Format timestamp
            let date_str = chrono::DateTime::from_timestamp(conv.updated_at, 0)
                .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
                .unwrap_or_default();

            // Preview text
            let preview = conv
                .last_message_preview
                .clone()
                .unwrap_or_else(|| "No messages".to_string());
            let truncated = if preview.len() > 100 {
                format!("{}...", &preview[..100])
            } else {
                preview
            };

            let conv_id = conv.id.clone();
            let delete_id = conv.id.clone();
            let title = conv.title.clone();

            let card = container(
                column()
                    .push(
                        // Title row
                        row()
                            .push(text(title).size(16))
                            .push(Space::with_width(Length::Fill))
                            .push(
                                text(date_str)
                                    .size(12)
                                    .class(cosmic::style::Text::Color(
                                        cosmic::iced::Color::from_rgb(0.6, 0.6, 0.6),
                                    )),
                            )
                            .align_y(cosmic::iced::Alignment::Center),
                    )
                    .push(
                        // Preview text
                        text(truncated)
                            .size(12)
                            .class(cosmic::style::Text::Color(cosmic::iced::Color::from_rgb(
                                0.5, 0.5, 0.5,
                            ))),
                    )
                    .push(
                        // Action buttons
                        row()
                            .push(Space::with_width(Length::Fill))
                            .push(
                                row()
                                    .push(
                                        button::icon(crate::ui::icons::get_handle(
                                            "chat-bubble-text-symbolic",
                                            16,
                                        ))
                                        .on_press(Message::SelectConversation(conv_id)),
                                    )
                                    .push(
                                        button::icon(crate::ui::icons::get_handle(
                                            "user-trash-full-symbolic",
                                            16,
                                        ))
                                        .class(widget::button::ButtonClass::Destructive)
                                        .on_press(Message::DeleteConversation(delete_id)),
                                    )
                                    .spacing(8),
                            )
                            .align_y(cosmic::iced::Alignment::Center),
                    )
                    .spacing(8),
            )
            .padding(16)
            .width(Length::Fill)
            .style(move |theme| {
                if is_selected {
                    cosmic::widget::container::Style {
                        background: Some(cosmic::iced::Background::Color(
                            theme.cosmic().primary.component.hover.into(),
                        )),
                        border: cosmic::iced::Border {
                            width: 2.0,
                            color: theme.cosmic().primary.base.into(),
                            radius: 8.0.into(),
                        },
                        ..Default::default()
                    }
                } else {
                    cosmic::widget::container::Style {
                        background: Some(cosmic::iced::Background::Color(
                            theme.cosmic().background.component.hover.into(),
                        )),
                        border: cosmic::iced::Border {
                            width: 0.0,
                            color: cosmic::iced::Color::TRANSPARENT,
                            radius: 8.0.into(),
                        },
                        ..Default::default()
                    }
                }
            });

            list = list.push(card);
        }

        content = content.push(
            scrollable(list)
                .height(Length::Fill)
                .width(Length::Fill),
        );
    }

    container(content)
        .padding(16)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}
