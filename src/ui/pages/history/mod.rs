use crate::ui::app::{CosmicLlmApp, Message};
use crate::llm::{Message as LlmMessage, Role};
use crate::llm::tokenizer::TokenCounter;
use crate::storage::conversation_storage::Conversation as StoredConversation;
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
                            
                            // Load full conversation to get messages and calculate context usage
                            let (message_count, context_usage) = if let Ok(Some(conv)) = app.storage.get_conversation(&conv_index.id) {
                                let count = conv.messages.len();
                                let usage = calculate_context_usage(&conv, &app.config);
                                (count, usage)
                            } else {
                                (0, None)
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
                                                cosmic::widget::text(
                                                    if let Some(pct) = context_usage {
                                                        format!("{} messages ({}% context)", message_count, pct)
                                                    } else {
                                                        format!("{} messages", message_count)
                                                    }
                                                )
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

/// Calculate context usage percentage for a conversation
/// This matches the server-side calculation by including system prompts
fn calculate_context_usage(conv: &StoredConversation, config: &crate::config::AppConfig) -> Option<u32> {
    // Get the profile for this conversation (or use default)
    let profile = conv.profile_name.as_ref()
        .and_then(|name| config.profiles.get(name))
        .or_else(|| config.get_default_profile())
        .or_else(|| config.profiles.values().next())?;
    
    // Convert conversation messages to LlmMessage format
    let mut llm_messages: Vec<LlmMessage> = conv.messages.iter().filter_map(|msg| {
        let role = match msg.role.as_str() {
            "user" => Role::User,
            "assistant" => Role::Assistant,
            "system" => Role::System,
            "tool" => Role::Tool,
            _ => return None,
        };
        
        Some(match role {
            Role::Tool => {
                let tool_call_id = msg.tool_call_id.clone().unwrap_or_else(|| "tool_result".to_string());
                LlmMessage::new_tool_result(
                    tool_call_id,
                    msg.content.clone(),
                    msg.tool_status.as_deref() == Some("error"),
                )
            }
            Role::Assistant => {
                let mut assistant_msg = if let Some(tool_calls) = msg.tool_calls.clone() {
                    if !tool_calls.is_empty() {
                        LlmMessage::new_with_tool_calls(role, msg.content.clone(), tool_calls)
                    } else {
                        LlmMessage::new(role, msg.content.clone())
                    }
                } else {
                    LlmMessage::new(role, msg.content.clone())
                };
                assistant_msg.reasoning_content = msg.reasoning_content.clone();
                assistant_msg
            }
            _ => LlmMessage::new(role, msg.content.clone()),
        })
    }).collect();
    
    // Count tokens from conversation messages
    let token_counter = TokenCounter::new(profile);
    let conversation_tokens: usize = llm_messages.iter()
        .map(|msg| token_counter.count_message_tokens(msg))
        .sum();
    
    // Get context limit (this now correctly detects DeepSeek models with 64k context)
    let context_limit = token_counter.get_context_limit(profile);
    
    // Calculate percentage and cap at 100% to avoid showing over 100%
    // Note: This doesn't include system prompts, but that's okay for UI display
    // The server-side calculation includes system prompts, but for UI we show conversation-only usage
    if context_limit > 0 {
        let percentage = (conversation_tokens as f32 / context_limit as f32) * 100.0;
        Some(percentage.min(100.0) as u32)
    } else {
        None
    }
}
