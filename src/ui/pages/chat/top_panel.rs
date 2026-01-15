use crate::ui::app::{CosmicLlmApp, Message};
use crate::llm::{Message as LlmMessage, Role};
use crate::llm::tokenizer::TokenCounter;
use crate::storage::conversation_storage::Conversation as StoredConversation;
use crate::prompts::PromptManager;
use cosmic::{iced::Length, widget, Element};
use tracing;

pub fn top_panel(app: &CosmicLlmApp) -> Element<Message> {
    // Count enabled/disabled tools from cache
    let total_tools = app.mcp_cache.total_tools_count();
    let enabled_count = app.mcp_cache.enabled_tools_count();

    // Conversation info
    let (title, created_text, _msg_count, context_usage) = if let Some(id) = app.conversation_state.current_conversation_id {
        if let Ok(Some(conv)) = app.storage.get_conversation(&id) {
            let created = conv.created_at.format("%Y-%m-%d %H:%M").to_string();
            // Prefer the latest title from the on-disk index (updated by background tasks)
            let index = app
                .storage
                .list_conversations_from_index()
                .unwrap_or_else(|e| {
                    tracing::error!(
                        error = %e,
                        "Failed to list conversations from index"
                    );
                    Vec::new()
                });
            let latest_title = index
                .into_iter()
                .find(|ci| ci.id == id)
                .map(|ci| ci.title)
                .unwrap_or_else(|| conv.title.clone());
            
            // Use cached context usage to avoid blocking UI during rendering
            // Cache is updated when conversation is loaded/changed
            let usage_pct = app.conversation_state.context_usage_cache.get(&id).copied().flatten();
            (latest_title, Some(created), conv.messages.len(), usage_pct)
        } else {
            ("New Chat".to_string(), None, app.conversation_state.messages.len(), None)
        }
    } else {
        ("New Chat".to_string(), None, app.conversation_state.messages.len(), None)
    };

    let _created_label = created_text.unwrap_or_else(|| "".to_string());

    cosmic::widget::container(
        cosmic::widget::column::with_capacity(3)
            .push(
                // First row: Title <-> New chat button
                cosmic::widget::row::with_capacity(2)
                    .push(cosmic::widget::text(title).size(18))
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
                // Divider line
                cosmic::widget::container(cosmic::widget::Space::with_height(Length::Fixed(1.0)))
                    .width(Length::Fill)
                    .style(|_theme| cosmic::widget::container::Style {
                        background: Some(cosmic::iced::Background::Color(
                            cosmic::iced::Color::from_rgb(0.3, 0.3, 0.3),
                        )),
                        ..Default::default()
                    }),
            )
            .push(
                // Second row: Profile label, dropdown, context %, tools count <-> config icon
                cosmic::widget::container(
                    cosmic::widget::row::with_capacity(6)
                        .push(
                            cosmic::widget::text("Profile")
                                .size(12)
                                
                        )
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
                        .push(cosmic::widget::Space::with_width(Length::Fixed(12.0)))
                        .push(
                            if let Some(pct) = context_usage {
                                cosmic::widget::text(format!("{}% context", pct))
                                    .size(12)
                                    
                            } else {
                                cosmic::widget::text("")
                                    .size(12)
                            },
                        )
                        .push(cosmic::widget::Space::with_width(Length::Fixed(12.0)))
                        .push(
                            if total_tools == 0 {
                                cosmic::widget::text("No tools")
                                    .size(12)
                                    
                            } else {
                                cosmic::widget::text(format!("{}/{} tools", enabled_count, total_tools))
                                    .size(12)
                                    
                            },
                        )
                        .push(cosmic::widget::Space::with_width(Length::Fill))
                        .push(
                            // Config icon (toggles tools)
                            widget::button::icon(crate::ui::icons::get_handle(
                                "configure-symbolic",
                                16,
                            ))
                            .on_press(Message::ShowToolsContext),
                        )
                        .spacing(8)
                        .align_y(cosmic::iced::Alignment::Center),
                )
                .width(Length::Fill)
                .padding(cosmic::iced::Padding::from([8, 12]))

            )
            .spacing(8),
    )
    .padding(12)
    .class(cosmic::style::Container::Card)

    

    .into()
}

/// Calculate context usage percentage for a conversation
/// This matches the server-side calculation by including system prompts
pub(crate) fn calculate_context_usage(
    conv: &StoredConversation, 
    config: &crate::config::AppConfig,
    prompt_manager: &PromptManager,
) -> Option<u32> {
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
                // Combine content AND tool_result_json (both may contain data)
                let mut content = msg.content.clone();
                if let Some(ref result_json) = msg.tool_result_json {
                    if !content.is_empty() {
                        content.push_str("\n");
                    }
                    content.push_str(&result_json.to_string());
                }
                LlmMessage::new_tool_result(
                    tool_call_id,
                    content,
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
    
    // Add system prompts to match server-side calculation
    // Add system prompt if available
    if let Some(system_prompt) = prompt_manager.get_system_prompt() {
        llm_messages.insert(0, LlmMessage::new(Role::System, system_prompt.to_string()));
    }
    
    // Add profile prompt if configured
    if let Some(profile_prompt_file) = profile.profile_prompt_file.as_ref() {
        let resolved_path = crate::config::AppConfig::resolve_config_path(profile_prompt_file);
        if let Ok(profile_prompt) = prompt_manager.load_profile_prompt(&resolved_path.to_string_lossy()) {
            llm_messages.insert(0, LlmMessage::new(Role::System, profile_prompt));
        }
    }
    
    // Count tokens (including system prompts) - matches server calculation
    let token_counter = TokenCounter::new(profile);
    let total_tokens: usize = llm_messages.iter()
        .map(|msg| token_counter.count_message_tokens(msg))
        .sum();
    
    // Get context limit (now correctly detects DeepSeek Reasoner with 131k context)
    let context_limit = token_counter.get_context_limit(profile);
    
    // Calculate percentage and cap at 100% to avoid showing over 100%
    if context_limit > 0 {
        let percentage = (total_tokens as f32 / context_limit as f32) * 100.0;
        Some(percentage.min(100.0) as u32)
    } else {
        None
    }
}
