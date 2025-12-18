use crate::ui::app::{CosmicLlmApp, Message};
use cosmic::{iced::Length, Element};
use uuid::Uuid;

pub fn history_view(app: &CosmicLlmApp) -> Element<Message> {
    let conversations = app
        .storage
        .list_conversations_from_index()
        .unwrap_or_else(|e| {
            eprintln!("Failed to list conversations: {}", e);
            Vec::new()
        });

    cosmic::widget::column::with_capacity(3)
        .push(
            // Enhanced header with icon and stats
            cosmic::widget::container(
                cosmic::widget::row::with_capacity(3)
                    .push(
                        cosmic::widget::row::with_capacity(2)
                            .push(
                                cosmic::widget::icon::from_name("list-large-symbolic")
                                    .size(20)
                            )
                            .push(
                                cosmic::widget::text("Conversation History")
                                    .size(20)
                            )
                            .spacing(8)
                            .align_y(cosmic::iced::Alignment::Center)
                    )
                    .push(cosmic::widget::Space::with_width(Length::Fill))
                    .push(
                        cosmic::widget::text(format!("{} conversations", conversations.len()))
                            .size(12)
                            .class(cosmic::style::Text::Color(cosmic::iced::Color::from_rgb(0.6, 0.6, 0.6)))
                    )
                    .spacing(12)
                    .align_y(cosmic::iced::Alignment::Center)
            )
            .padding(16)
            .class(cosmic::style::Container::Card)
        )
        .push(
            // Search bar
            cosmic::widget::container(
                cosmic::widget::row::with_capacity(2)
                    .push(
                        cosmic::widget::icon::from_name("search-symbolic")
                            .size(16)
                    )
                    .push(
                        cosmic::widget::text_input("Search conversations...", &app.search_query)
                            .on_input(Message::SearchChanged)
                            .width(Length::Fill)
                    )
                    .spacing(8)
                    .align_y(cosmic::iced::Alignment::Center)
            )
            .padding(12)
            .class(cosmic::style::Container::Card)
        )
        .push(
            // Enhanced conversations list or search results
            {
                // Show search results if search is active
                if !app.search_query.trim().is_empty() {
                    let mut column = cosmic::widget::column::with_capacity(app.search_results.len().max(1));
                    
                    if app.search_results.is_empty() {
                        column = column.push(
                            cosmic::widget::container(
                                cosmic::widget::column::with_capacity(3)
                                    .push(
                                        cosmic::widget::icon::from_name("search-symbolic")
                                            .size(48)
                                    )
                                    .push(
                                        cosmic::widget::text("No results found")
                                            .size(16)
                                    )
                                    .push(
                                        cosmic::widget::text(format!("No messages found matching '{}'", app.search_query))
                                            .size(12)
                                            .class(cosmic::style::Text::Color(cosmic::iced::Color::from_rgb(0.6, 0.6, 0.6)))
                                    )
                                    .spacing(8)
                                    .align_x(cosmic::iced::Alignment::Center)
                            )
                            .padding(32)
                            .class(cosmic::style::Container::Card)
                        );
                    } else {
                        for result in &app.search_results {
                            // Parse conversation ID to UUID
                            if let Ok(conv_id) = Uuid::parse_str(&result.conversation_id) {
                                // Get conversation title
                                let title = if let Ok(Some(conv)) = app.storage.get_conversation(&conv_id) {
                                    conv.title
                                } else {
                                    "Unknown Conversation".to_string()
                                };
                                
                                // Format timestamp
                                let date_str = chrono::DateTime::from_timestamp(result.timestamp, 0)
                                    .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
                                    .unwrap_or_else(|| "Unknown date".to_string());
                                
                                // Truncate content for preview
                                let preview = if result.content.chars().count() > 200 {
                                    result.content.chars().take(200).collect::<String>() + "..."
                                } else {
                                    result.content.clone()
                                };
                                
                                let search_result_card = cosmic::widget::container(
                                    cosmic::widget::column::with_capacity(4)
                                        .push(
                                            cosmic::widget::row::with_capacity(3)
                                                .push(
                                                    cosmic::widget::text(title.clone())
                                                        .size(16)
                                                )
                                                .push(cosmic::widget::Space::with_width(Length::Fill))
                                                .push(
                                                    cosmic::widget::text(date_str)
                                                        .size(12)
                                                        .class(cosmic::style::Text::Color(cosmic::iced::Color::from_rgb(0.6, 0.6, 0.6)))
                                                )
                                                .align_y(cosmic::iced::Alignment::Center)
                                        )
                                        .push(
                                            cosmic::widget::text(preview)
                                                .size(12)
                                                .class(cosmic::style::Text::Color(cosmic::iced::Color::from_rgb(0.5, 0.5, 0.5)))
                                        )
                                        .push(
                                            cosmic::widget::row::with_capacity(2)
                                                .push(cosmic::widget::Space::with_width(Length::Fill))
                                                .push(
                                                    cosmic::widget::button::icon(crate::ui::icons::get_handle("chat-bubble-text-symbolic", 16))
                                                        .on_press(Message::SelectConversation(conv_id))
                                                )
                                                .spacing(8)
                                                .align_y(cosmic::iced::Alignment::Center)
                                        )
                                        .spacing(8)
                                )
                                .padding(16)
                                .class(cosmic::style::Container::Card);
                                
                                column = column.push(search_result_card);
                            }
                        }
                    }
                    
                    cosmic::widget::scrollable(column.spacing(8))
                } else {
                    // Show normal conversation list
                    let mut column = cosmic::widget::column::with_capacity(conversations.len().max(1));
                    if conversations.is_empty() {
                        column = column.push(
                            cosmic::widget::container(
                                cosmic::widget::column::with_capacity(3)
                                    .push(
                                        cosmic::widget::icon::from_name("chat-bubble-empty-symbolic")
                                            .size(48)
                                    )
                                    .push(
                                        cosmic::widget::text("No conversations yet")
                                            .size(16)
                                    )
                                    .push(
                                        cosmic::widget::text("Start a new chat to create your first conversation!")
                                            .size(12)
                                            .class(cosmic::style::Text::Color(cosmic::iced::Color::from_rgb(0.6, 0.6, 0.6)))
                                    )
                                    .spacing(8)
                                    .align_x(cosmic::iced::Alignment::Center)
                            )
                            .padding(32)
                            .class(cosmic::style::Container::Card)
                        );
                    } else {
                        for conv_index in conversations {
                            let title = conv_index.title.clone();
                            let date_str = conv_index.updated_at.format("%Y-%m-%d %H:%M").to_string();
                            
                            // Get message count (lightweight, no token counting)
                            let message_count = if let Ok(Some(conv)) = app.storage.get_conversation(&conv_index.id) {
                                conv.messages.len()
                            } else {
                                0
                            };
                            
                            let conversation_card = cosmic::widget::container(
                                cosmic::widget::column::with_capacity(3)
                                    .push(
                                        cosmic::widget::row::with_capacity(3)
                                            .push(
                                                cosmic::widget::text(title.clone())
                                                    .size(16)
                                            )
                                            .push(cosmic::widget::Space::with_width(Length::Fill))
                                            .push(
                                                cosmic::widget::text(date_str)
                                                    .size(12)
                                                    .class(cosmic::style::Text::Color(cosmic::iced::Color::from_rgb(0.6, 0.6, 0.6)))
                                            )
                                            .align_y(cosmic::iced::Alignment::Center)
                                    )
                                    .push(
                                        cosmic::widget::row::with_capacity(2)
                                            .push(
                                                cosmic::widget::text(format!("{} messages", message_count))
                                                    .size(12)
                                                    .class(cosmic::style::Text::Color(cosmic::iced::Color::from_rgb(0.5, 0.5, 0.5)))
                                            )
                                            .push(cosmic::widget::Space::with_width(Length::Fill))
                                            .push(
                                                cosmic::widget::row::with_capacity(2)
                                                    .push(
                                                        cosmic::widget::button::icon(crate::ui::icons::get_handle("chat-bubble-text-symbolic", 16))
                                                            .on_press(Message::SelectConversation(conv_index.id))
                                                    )
                                                    .push(
                                                        cosmic::widget::button::icon(crate::ui::icons::get_handle("user-trash-full-symbolic", 16))
                                                            .class(cosmic::widget::button::ButtonClass::Destructive)
                                                            .on_press(Message::DeleteConversation(conv_index.id))
                                                    )
                                                    .spacing(8)
                                            )
                                            .align_y(cosmic::iced::Alignment::Center)
                                    )
                                    .spacing(8)
                            )
                            .padding(16)
                            .class(cosmic::style::Container::Card);
                            
                            column = column.push(conversation_card);
                        }
                    }
                    cosmic::widget::scrollable(column.spacing(8))
                }
            }
            .height(Length::Fill)
            .width(Length::Fill)
        )
        .spacing(12)
        .into()
}
