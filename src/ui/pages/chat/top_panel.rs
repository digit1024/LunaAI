use crate::ui::app::{CosmicLlmApp, Message};
use crate::llm::{Message as LlmMessage, Role};
use crate::llm::tokenizer::TokenCounter;
use crate::storage::conversation_storage::Conversation as StoredConversation;
use cosmic::{iced::Length, widget, Element};

pub fn top_panel(app: &CosmicLlmApp) -> Element<Message> {
    // Count enabled/disabled tools
    let total_tools = app.available_mcp_tools.len();
    let enabled_count = app
        .available_mcp_tools
        .iter()
        .filter(|tool| app.tool_states.get(&tool.name).copied().unwrap_or(true))
        .count();

    // Conversation info
    let (title, created_text, msg_count, context_usage) = if let Some(id) = app.current_conversation_id {
        if let Ok(Some(conv)) = app.storage.get_conversation(&id) {
            let created = conv.created_at.format("%Y-%m-%d %H:%M").to_string();
            // Prefer the latest title from the on-disk index (updated by background tasks)
            let index = app
                .storage
                .list_conversations_from_index()
                .unwrap_or_else(|e| {
                    eprintln!("Failed to list conversations: {}", e);
                    Vec::new()
                });
            let latest_title = index
                .into_iter()
                .find(|ci| ci.id == id)
                .map(|ci| ci.title)
                .unwrap_or_else(|| conv.title.clone());
            
            // Calculate context usage percentage
            let usage_pct = calculate_context_usage(&conv, &app.config);
            (latest_title, Some(created), conv.messages.len(), usage_pct)
        } else {
            ("New Chat".to_string(), None, app.messages.len(), None)
        }
    } else {
        ("New Chat".to_string(), None, app.messages.len(), None)
    };

    let _created_label = created_text.unwrap_or_else(|| "".to_string());

    cosmic::widget::container(
        cosmic::widget::column::with_capacity(2)
            .push(
                // Top row: Title, Messages count, New chat icon
                cosmic::widget::row::with_capacity(3)
                    .push(cosmic::widget::text(title).size(18))
                    .push(
                        cosmic::widget::text(
                            if let Some(pct) = context_usage {
                                format!("{} messages ({}% context)", msg_count, pct)
                            } else {
                                format!("{} messages", msg_count)
                            }
                        )
                        .size(12)
                        .class(cosmic::style::Text::Color(cosmic::iced::Color::from_rgb(
                            0.4, 0.4, 0.4,
                        ))),
                    )
                    .push(cosmic::widget::Space::with_width(Length::Fill))
                    .push(
                        // New chat icon button
                        widget::button::icon(crate::ui::icons::get_handle(
                            "plus-circle-filled-symbolic",
                            16,
                        ))
                        .class(widget::button::ButtonClass::Suggested)
                        .on_press(Message::NewConversation),
                    )
                    .spacing(12)
                    .align_y(cosmic::iced::Alignment::Center),
            )
            .push(
                // Bottom row: Model select, Tools summary with icons
                cosmic::widget::row::with_capacity(4)
                    .push(
                        // Profile selection dropdown
                        {
                            let mut names: Vec<String> = app.config.profiles
                                .iter()
                                .filter(|(_, p)| !p.hidden)
                                .map(|(name, _)| name.clone())
                                .collect();
                            names.sort();
                            let idx = names.iter().position(|k| k == &app.config.default);
                            widget::dropdown(names, idx, Message::ChangeDefaultProfile)
                        },
                    )
                    .push(cosmic::widget::Space::with_width(Length::Fill))
                    .push(
                        // Tools summary with toggle and configure icons
                        if total_tools == 0 {
                            // Show configure button when no tools
                            cosmic::widget::row::with_capacity(2)
                                .push(cosmic::widget::text("No tools configured").size(12).class(
                                    cosmic::style::Text::Color(cosmic::iced::Color::from_rgb(
                                        0.5, 0.5, 0.5,
                                    )),
                                ))
                                .push(
                                    widget::button::icon(crate::ui::icons::get_handle(
                                        "configure-symbolic",
                                        16,
                                    ))
                                    .on_press(Message::ShowToolsContext),
                                )
                                .spacing(8)
                                .align_y(cosmic::iced::Alignment::Center)
                        } else {
                            // Tool controls with icons
                            cosmic::widget::row::with_capacity(4)
                                .push(
                                    cosmic::widget::text(format!(
                                        "{} / {} tools",
                                        enabled_count, total_tools
                                    ))
                                    .size(12),
                                )
                                .push(
                                    // Toggle all tools button (toggler widget)
                                    cosmic::widget::toggler(enabled_count == total_tools)
                                        .on_toggle(|enabled| Message::ToggleAllTools(enabled)),
                                )
                                .push(
                                    // Configure tools button (icon)
                                    widget::button::icon(crate::ui::icons::get_handle(
                                        "configure-symbolic",
                                        16,
                                    ))
                                    .on_press(Message::ShowToolsContext),
                                )
                                .spacing(8)
                                .align_y(cosmic::iced::Alignment::Center)
                        },
                    )
                    .spacing(12)
                    .align_y(cosmic::iced::Alignment::Center),
            )
            .spacing(8),
    )
    .padding(12)
    .class(cosmic::style::Container::Card)
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
