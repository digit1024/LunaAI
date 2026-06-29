//! History page - conversation list with search and rename

use std::collections::HashMap;

use cosmic::{
    iced::Length,
    widget::{self, button, container, icon, scrollable, text, text_input, Column, Row, Space},
    Element,
};

use crate::server::dto::SearchResult;
use crate::ui::app::{LunaThinApp, Message};
use crate::ui::widgets::page_header;

/// Group search hits by conversation, keeping the best-ranked snippet per conversation.
fn grouped_search_results(results: &[SearchResult]) -> Vec<SearchResult> {
    let mut best: HashMap<String, SearchResult> = HashMap::new();
    for r in results {
        best.entry(r.conversation_id.clone())
            .and_modify(|existing| {
                if r.rank > existing.rank {
                    *existing = r.clone();
                }
            })
            .or_insert_with(|| r.clone());
    }
    let mut grouped: Vec<SearchResult> = best.into_values().collect();
    grouped.sort_by(|a, b| {
        b.rank
            .partial_cmp(&a.rank)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    grouped
}

fn truncate_text(text: &str, max_chars: usize) -> String {
    if text.chars().count() > max_chars {
        let head: String = text.chars().take(max_chars).collect();
        format!("{head}...")
    } else {
        text.to_string()
    }
}

pub fn history_page(app: &LunaThinApp) -> Element<'_, Message> {
    let searching = !app.history_search.trim().is_empty();
    let search_results = grouped_search_results(&app.history_search_results);

    let mut content = Column::new().spacing(12);

    let trailing = if searching {
        format!("{} results", search_results.len())
    } else {
        format!("{} conversations", app.conversations.len())
    };

    content = content.push(page_header::subpage_header(
        "Conversation History",
        "list-large-symbolic",
        Some(trailing),
    ));

    content = content.push(
        container(
            Row::new()
                .push(icon::from_name("search-symbolic").size(16))
                .push(
                    text_input("Search conversations...", &app.history_search)
                        .on_input(Message::HistorySearchChanged)
                        .width(Length::Fill),
                )
                .spacing(8)
                .align_y(cosmic::iced::Alignment::Center),
        )
        .padding(12)
        .width(Length::Fill)
        .class(cosmic::style::Container::Card),
    );

    let show_internal_label = if app.show_internal {
        "Hide transient"
    } else {
        "Show transient"
    };
    content = content.push(
        button::text(show_internal_label)
            .on_press(Message::ToggleShowInternal)
            .class(widget::button::ButtonClass::Standard),
    );

    if searching {
        if search_results.is_empty() {
            content = content.push(empty_state(
                "search-symbolic",
                "No matching conversations",
                "Try a different search term",
            ));
        } else {
            let mut list = Column::new().spacing(8);
            for result in search_results {
                let title = if result.conversation_title.is_empty() {
                    "Untitled".to_string()
                } else {
                    result.conversation_title.clone()
                };
                let preview = truncate_text(&result.snippet, 120);
                list = list.push(conversation_card(
                    app,
                    result.conversation_id.clone(),
                    title,
                    preview,
                    result.timestamp,
                    false,
                ));
            }
            content = content.push(scrollable(list).height(Length::Fill).width(Length::Fill));
        }
    } else if app.conversations.is_empty() {
        content = content.push(empty_state(
            "chat-bubble-empty-symbolic",
            "No conversations yet",
            "Start a new chat to create your first conversation!",
        ));
    } else {
        let mut list = Column::new().spacing(8);
        for conv in &app.conversations {
            let preview = conv
                .last_message_preview
                .clone()
                .unwrap_or_else(|| "No messages".to_string());
            list = list.push(conversation_card(
                app,
                conv.id.clone(),
                conv.title.clone(),
                truncate_text(&preview, 100),
                conv.updated_at,
                conv.internal,
            ));
        }
        content = content.push(scrollable(list).height(Length::Fill).width(Length::Fill));
    }

    container(content)
        .padding(16)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn empty_state<'a>(icon_name: &str, title: &'a str, subtitle: &'a str) -> Element<'a, Message> {
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
    .class(cosmic::style::Container::Card)
    .into()
}

fn conversation_card<'a>(
    app: &'a LunaThinApp,
    conv_id: String,
    title: String,
    preview: String,
    updated_at: i64,
    internal: bool,
) -> Element<'a, Message> {
    let is_selected = app.current_conversation_id.as_deref() == Some(conv_id.as_str());
    let is_renaming = app
        .renaming_conversation
        .as_ref()
        .map(|(id, _)| id == &conv_id)
        .unwrap_or(false);
    let rename_draft = app
        .renaming_conversation
        .as_ref()
        .filter(|(id, _)| id == &conv_id)
        .map(|(_, draft)| draft.as_str())
        .unwrap_or("");

    let date_str = chrono::DateTime::from_timestamp(updated_at, 0)
        .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_default();

    let delete_id = conv_id.clone();
    let rename_id = conv_id.clone();
    let open_id = conv_id.clone();
    let transient_id = conv_id.clone();
    let transient_next = !internal;

    let title_row = if is_renaming {
        Row::new()
            .push(
                button::icon(crate::ui::icons::get_handle("arrow1-left-symbolic", 16))
                    .on_press(Message::CancelRenameConversation)
                    .class(widget::button::ButtonClass::Text)
                    .padding(4),
            )
            .push(Space::new().width(8))
            .push(
                text_input("Title...", rename_draft)
                    .on_input(Message::RenameDraftChanged)
                    .on_submit(|_| Message::ConfirmRenameConversation)
                    .width(Length::Fill),
            )
            .push(
                button::icon(crate::ui::icons::get_handle(
                    "object-select-symbolic",
                    16,
                ))
                .on_press(Message::ConfirmRenameConversation),
            )
            .push(
                button::icon(crate::ui::icons::get_handle("process-stop-symbolic", 16))
                    .on_press(Message::CancelRenameConversation),
            )
            .spacing(8)
            .align_y(cosmic::iced::Alignment::Center)
    } else {
        let mut title_row_inner = Row::new().push(text(title).size(16));
        if internal {
            title_row_inner = title_row_inner.push(
                text("transient")
                    .size(10)
                    .class(cosmic::style::Text::Color(
                        cosmic::iced::Color::from_rgb(0.55, 0.55, 0.55),
                    )),
            );
        }
        title_row_inner
            .push(Space::new().width(Length::Fill))
            .push(
                text(date_str).size(12).class(cosmic::style::Text::Color(
                    cosmic::iced::Color::from_rgb(0.6, 0.6, 0.6),
                )),
            )
            .align_y(cosmic::iced::Alignment::Center)
    };

    container(
        Column::new()
            .push(title_row)
            .push(
                text(preview).size(12).class(cosmic::style::Text::Color(
                    cosmic::iced::Color::from_rgb(0.5, 0.5, 0.5),
                )),
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
                                .on_press(Message::BeginRenameConversation(rename_id)),
                            )
                            .push(
                                button::icon(crate::ui::icons::get_handle(
                                    if internal {
                                        "view-visible-symbolic"
                                    } else {
                                        "eye-not-visible-symbolic"
                                    },
                                    16,
                                ))
                                .on_press(Message::SetConversationInternal {
                                    conversation_id: transient_id,
                                    internal: transient_next,
                                }),
                            )
                            .push(
                                button::icon(crate::ui::icons::get_handle(
                                    "chat-bubble-text-symbolic",
                                    16,
                                ))
                                .on_press(Message::SelectConversation(open_id)),
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
                    color: theme.cosmic().primary.base.into(),
                    width: 2.0,
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
                    color: cosmic::iced::Color::TRANSPARENT,
                    width: 0.0,
                    radius: 8.0.into(),
                },
                ..Default::default()
            }
        }
    })
    .into()
}
