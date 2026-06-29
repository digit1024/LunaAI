//! Memories page - list, search, edit, and delete long-term memories

use cosmic::{
    iced::{Background, Color, Length},
    widget::{self, button, container, icon, scrollable, text, text_editor, text_input, Column, Row, Space},
    Element,
};

use crate::ui::app::{LunaThinApp, Message};
use crate::ui::widgets::page_header;

fn truncate_text(text: &str, max_chars: usize) -> String {
    if text.chars().count() > max_chars {
        let head: String = text.chars().take(max_chars).collect();
        format!("{head}...")
    } else {
        text.to_string()
    }
}

pub fn memories_page(app: &LunaThinApp) -> Element<'_, Message> {
    let searching = !app.memories_search.trim().is_empty();

    let mut content = Column::new().spacing(12);

    let memory_count = format!("{} memories", app.memories.len());
    content = content.push(page_header::subpage_header(
        "Memories",
        "emblem-favorite-symbolic",
        Some(memory_count),
    ));

    content = content.push(
        container(
            Row::new()
                .push(icon::from_name("search-symbolic").size(16))
                .push(
                    text_input("Search memories...", &app.memories_search)
                        .on_input(Message::MemoriesSearchChanged)
                        .width(Length::Fill),
                )
                .spacing(8)
                .align_y(cosmic::iced::Alignment::Center),
        )
        .padding(12)
        .width(Length::Fill)
        .class(cosmic::style::Container::Card),
    );

    if app.memories.is_empty() {
        let (icon_name, title, subtitle) = if searching {
            (
                "search-symbolic",
                "No matching memories",
                "Try a different search term",
            )
        } else {
            (
                "emblem-favorite-symbolic",
                "No memories yet",
                "Memories are created when the assistant stores facts across conversations.",
            )
        };
        content = content.push(
            container(
                Column::new()
                    .push(icon::from_name(icon_name).size(48))
                    .push(text(title).size(16))
                    .push(
                        text(subtitle).size(12).class(cosmic::style::Text::Color(
                            cosmic::iced::Color::from_rgb(0.6, 0.6, 0.6),
                        )),
                    )
                    .spacing(8)
                    .align_x(cosmic::iced::Alignment::Center),
            )
            .padding(32)
            .width(Length::Fill)
            .class(cosmic::style::Container::Card),
        );
    } else {
        let mut list = Column::new().spacing(8);
        for memory in &app.memories {
            list = list.push(memory_card(app, memory));
        }
        if app.memories_has_more {
            list = list.push(
                container(
                    button::standard("Load more")
                        .width(Length::Fill)
                        .on_press(Message::LoadMoreMemories),
                )
                .padding([8, 0])
                .width(Length::Fill),
            );
        }
        content = content.push(scrollable(list).height(Length::Fill).width(Length::Fill));
    }

    container(content)
        .padding(16)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn memory_card<'a>(
    app: &'a LunaThinApp,
    memory: &'a crate::server::dto::MemoryView,
) -> Element<'a, Message> {
    let is_editing = app
        .editing_memory
        .as_ref()
        .map(|d| d.id == memory.id)
        .unwrap_or(false);

    if is_editing {
        return edit_memory_card(app);
    }

    let memory_id = memory.id;
    let category_label = memory
        .category
        .as_deref()
        .filter(|c| !c.is_empty())
        .map(|c| format!("[{c}] "))
        .unwrap_or_default();
    let preview = truncate_text(&memory.content, 200);
    let date_str = chrono::DateTime::from_timestamp(memory.updated_at, 0)
        .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_default();

    container(
        Column::new()
            .push(
                Row::new()
                    .push(
                        text(format!("{category_label}{preview}")).size(14).width(Length::Fill),
                    )
                    .push(
                        text(date_str).size(12).class(cosmic::style::Text::Color(
                            cosmic::iced::Color::from_rgb(0.6, 0.6, 0.6),
                        )),
                    )
                    .spacing(8)
                    .align_y(cosmic::iced::Alignment::Start),
            )
            .push(
                text(format!("Importance: {}", memory.importance)).size(12).class(
                    cosmic::style::Text::Color(cosmic::iced::Color::from_rgb(0.5, 0.5, 0.5)),
                ),
            )
            .push(
                Row::new()
                    .push(Space::new().width(Length::Fill))
                    .push(
                        Row::new()
                            .push(
                                button::icon(crate::ui::icons::get_handle(
                                    "document-edit-symbolic",
                                    16,
                                ))
                                .on_press(Message::BeginEditMemory(memory_id)),
                            )
                            .push(
                                button::icon(crate::ui::icons::get_handle(
                                    "user-trash-full-symbolic",
                                    16,
                                ))
                                .class(widget::button::ButtonClass::Destructive)
                                .on_press(Message::DeleteMemory(memory_id)),
                            )
                            .spacing(8),
                    )
                    .align_y(cosmic::iced::Alignment::Center),
            )
            .spacing(8),
    )
    .padding(16)
    .width(Length::Fill)
    .style(|theme| cosmic::widget::container::Style {
        background: Some(cosmic::iced::Background::Color(
            theme.cosmic().background.component.hover.into(),
        )),
        border: cosmic::iced::Border {
            color: cosmic::iced::Color::TRANSPARENT,
            width: 0.0,
            radius: 8.0.into(),
        },
        ..Default::default()
    })
    .into()
}

fn edit_memory_card<'a>(app: &'a LunaThinApp) -> Element<'a, Message> {
    let draft = app.editing_memory.as_ref().expect("edit card without draft");

    container(
        Column::new()
            .push(
                Row::new()
                    .push(
                        button::icon(crate::ui::icons::get_handle("arrow1-left-symbolic", 16))
                            .on_press(Message::CancelEditMemory)
                            .class(widget::button::ButtonClass::Text)
                            .padding(4),
                    )
                    .push(Space::new().width(8))
                    .push(text("Edit memory").size(14))
                    .align_y(cosmic::iced::Alignment::Center),
            )
            .push(
                container(
                    text_editor(&draft.content)
                        .class(cosmic::theme::iced::TextEditor::Custom(Box::new(
                            |_theme, _status| text_editor::Style {
                                background: Background::Color(Color::TRANSPARENT),
                                border: cosmic::iced::Border {
                                    color: Color::TRANSPARENT,
                                    width: 0.0,
                                    radius: 0.0.into(),
                                },
                                placeholder: cosmic::theme::active().cosmic().on_bg_color().into(),
                                value: cosmic::theme::active().cosmic().on_bg_color().into(),
                                selection: Color::from_rgba(1.0, 1.0, 1.0, 0.3),
                            },
                        )))
                        .on_action(Message::MemoryDraftContentAction)
                        .height(Length::Fixed(120.0))
                        .padding(8)
                        .placeholder("Content..."),
                )
                .width(Length::Fill)
                .class(cosmic::style::Container::Card),
            )
            .push(
                text_input("Category (optional)", &draft.category)
                    .on_input(Message::MemoryDraftCategoryChanged)
                    .width(Length::Fill),
            )
            .push(
                Row::new()
                    .push(text("Importance (1-10):").size(12))
                    .push(
                        text_input("5", &draft.importance)
                            .on_input(Message::MemoryDraftImportanceChanged)
                            .width(Length::Fixed(48.0)),
                    )
                    .spacing(8)
                    .align_y(cosmic::iced::Alignment::Center),
            )
            .push(
                Row::new()
                    .push(Space::new().width(Length::Fill))
                    .push(
                        button::icon(crate::ui::icons::get_handle(
                            "object-select-symbolic",
                            16,
                        ))
                        .on_press(Message::ConfirmEditMemory)
                        .padding(4),
                    )
                    .push(
                        button::icon(crate::ui::icons::get_handle("process-stop-symbolic", 16))
                            .on_press(Message::CancelEditMemory)
                            .padding(4),
                    )
                    .spacing(8)
                    .align_y(cosmic::iced::Alignment::Center),
            )
            .spacing(8),
    )
    .padding(16)
    .width(Length::Fill)
    .class(cosmic::style::Container::Card)
    .into()
}
