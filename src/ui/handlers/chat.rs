//! Chat message handlers
//!
//! Handles chat-related messages: SendMessage, StopMessage, RetryMessage, AttachFile, etc.

use cosmic::{app, widget::text_editor};
use uuid::Uuid;

use crate::services::{ContextService, MessageConverter};
use crate::ui::app::{ChatMessage, CosmicLlmApp, Message};

/// Handle chat-related messages
pub fn handle_chat_messages(
    app: &mut CosmicLlmApp,
    message: Message,
) -> Option<app::Task<Message>> {
    match message {
        Message::InputChanged(input) => {
            app.chat_page.input = input;
            None
        }
        Message::InputActionPerformed(action) => {
            app.chat_page.input_content.perform(action);
            app.chat_page.input = app.chat_page.input_content.text();
            None
        }
        Message::SendMessage => handle_send_message(app),
        Message::StopMessage => {
            handle_stop_message(app);
            None
        }
        Message::RetryMessage => handle_retry_message(app),
        Message::AttachFile => handle_attach_file(app),
        Message::FileSelected(file_path) => {
            let _ = app.attachment_state.add_file(file_path);
            None
        }
        Message::RemoveFile(file_path) => {
            app.attachment_state.remove_file(&file_path);
            None
        }
        Message::FileChooserCancelled => None,
        Message::FileChooserError(error) => {
            app.chat_page.current_error = Some(format!("File selection error: {}", error));
            None
        }
        Message::ScrollToBottom => None, // Handled by UI
        Message::InlineError(error) => {
            app.chat_page.current_error = Some(error);
            Some(app::Task::none())
        }
        Message::DismissError => {
            app.chat_page.current_error = None;
            Some(app::Task::none())
        }
        Message::TypingIndicatorTick(instant) => {
            if let Some(start_time) = app.chat_page.typing_indicator_start_time {
                let elapsed = instant.duration_since(start_time);
                // Update animation progress (cycles every 1.2 seconds)
                app.chat_page.typing_indicator_progress = (elapsed.as_secs_f32() / 1.2) % 1.0;
            }
            Some(app::Task::none())
        }
        _ => None, // Not a chat message
    }
}

/// Handle SendMessage - the largest and most complex handler
fn handle_send_message(app: &mut CosmicLlmApp) -> Option<app::Task<Message>> {
    #[cfg(feature = "ttsandstt")]
    let is_conv_mode = app.is_conversation_mode_active;
    #[cfg(not(feature = "ttsandstt"))]
    let is_conv_mode = false;
    
    tracing::info!(
        input_length = app.chat_page.input.len(),
        input_preview = app.chat_page.input.chars().take(50).collect::<String>(),
        attachment_count = app.attachment_state.attached_files.len(),
        is_conversation_mode = is_conv_mode,
        "SendMessage received"
    );
    
    // Allow sending if there's text OR if there are attachments
    if app.chat_page.input.trim().is_empty() && app.attachment_state.attached_files.is_empty() {
        return None;
    }

    // Create new conversation if none exists
    if app.conversation_state.current_conversation_id.is_none() {
        let current_profile_name = Some(app.config.default.as_str());
        let conv_id = app
            .storage
            .create_conversation_with_profile("Generating title...".to_string(), current_profile_name)
            .unwrap_or_else(|e| {
                tracing::error!(error = %e, "Failed to create conversation");
                Uuid::new_v4()
            });
        app.conversation_state.current_conversation_id = Some(conv_id);
        // Update nav model to reflect new conversation
        app.load_recent_conversations();
        app.update_nav_model();

        // Generate title synchronously
        tracing::debug!(conversation_id = %conv_id, "Starting title generation");
        let message_text = app.chat_page.input.clone();

        // Create a simple title based on first few words
        let fallback_title = if message_text.len() > 50 {
            format!("{}...", &message_text[..47])
        } else {
            message_text
        };

        tracing::debug!(
            conversation_id = %conv_id,
            title = %fallback_title,
            "Generated title"
        );
        if let Err(e) = app
            .storage
            .update_conversation_title(&conv_id, fallback_title.clone())
        {
            tracing::error!(
                conversation_id = %conv_id,
                error = %e,
                "Failed to update conversation title"
            );
        }
    }

    // Create user message content
    let message_content = app.chat_page.input.clone();

    // Add user message
    let user_msg = ChatMessage {
        content: message_content,
        is_user: true,
        is_error: false,
        reasoning_content: None,
        is_summary: false,
        is_summarized: false,
        summarized_count: None,
    };
    app.conversation_state.messages.push(user_msg.clone());

    // Play sent sound
    crate::ui::audio::AudioService::play_sound("sent.mp3");

    // Add to storage
    if let Some(conv_id) = app.conversation_state.current_conversation_id {
        if let Err(e) = app.storage.add_message_to_conversation(
            &conv_id,
            "user".to_string(),
            app.chat_page.input.clone(),
        ) {
            tracing::error!(error = %e, "Failed to add message to conversation");
        } else {
            // Update context usage cache after adding message
            app.update_context_usage_cache(conv_id);
        }
    }

    // Send to LLM and get response
    let input_text = app.chat_page.input.clone();
    app.chat_page.input.clear();
    app.chat_page.input_content = text_editor::Content::new();

    // Assistant bubble will be created when streaming starts
    app.tool_call_state.set_current_ai_message_index(None);

    // Create attachments for the current message FIRST
    let mut attachments = Vec::new();
    tracing::debug!(
        file_count = app.attachment_state.attached_files.len(),
        "Processing attached files"
    );
    for file_path in &app.attachment_state.attached_files {
        tracing::debug!(file_path = %file_path, "Processing file");
        match crate::llm::file_utils::create_attachment(file_path) {
            Ok(attachment) => {
                tracing::debug!(
                    file_name = %attachment.file_name,
                    mime_type = %attachment.mime_type,
                    "Created attachment"
                );
                // Validate file for LLM
                if let Err(e) =
                    crate::llm::file_utils::validate_file_for_llm(&attachment)
                {
                    tracing::error!(
                        file_path = %file_path,
                        error = %e,
                        "File validation failed"
                    );
                    app.chat_page.current_error = Some(format!(
                        "File validation error for {}: {}",
                        file_path, e
                    ));
                    return None;
                }
                tracing::debug!(file_path = %file_path, "File validation passed");
                attachments.push(attachment);
            }
            Err(e) => {
                tracing::error!(
                    file_path = %file_path,
                    error = %e,
                    "Failed to create attachment"
                );
                app.chat_page.current_error =
                    Some(format!("Failed to process file {}: {}", file_path, e));
                return None;
            }
        }
    }
    tracing::debug!(attachment_count = attachments.len(), "Final attachments count");

    // Convert messages to LLM format using services
    let mut llm_messages = Vec::new();

    // Load messages from database and convert using MessageConverter service
    if let Some(conv_id) = app.conversation_state.current_conversation_id {
        match app.storage.load_conversation_messages(&conv_id.to_string()) {
            Ok(db_messages) => {
                // Use MessageConverter service (single source of truth)
                llm_messages = MessageConverter::db_to_llm(&db_messages, true);
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "Failed to load messages from DB, falling back to UI messages"
                );
                // Fallback to UI messages
                for msg in &app.conversation_state.messages {
                    let role = if msg.is_user {
                        crate::llm::Role::User
                    } else {
                        crate::llm::Role::Assistant
                    };
                    llm_messages.push(crate::llm::Message::new(role, msg.content.clone()));
                }
            }
        }
    } else {
        // No conversation yet, use UI messages
        for msg in &app.conversation_state.messages {
            let role = if msg.is_user {
                crate::llm::Role::User
            } else {
                crate::llm::Role::Assistant
            };
            llm_messages.push(crate::llm::Message::new(role, msg.content.clone()));
        }
    }

    // Inject prompts using ContextService
    if let Some(profile) = app.config.get_default_profile() {
        llm_messages = match ContextService::inject_prompts(
            llm_messages,
            &app.prompt_manager,
            profile,
        ) {
            Ok(messages_with_prompts) => messages_with_prompts,
            Err(e) => {
                tracing::error!(error = %e, "Failed to inject prompts, using messages without prompts");
                // Rebuild messages without prompts (inject_prompts consumed the original)
                let mut fallback_messages = Vec::new();
                if let Some(conv_id) = app.conversation_state.current_conversation_id {
                    if let Ok(db_messages) = app.storage.load_conversation_messages(&conv_id.to_string()) {
                        fallback_messages = MessageConverter::db_to_llm(&db_messages, true);
                    }
                }
                fallback_messages
            }
        }
    } else {
        tracing::warn!("No default profile configured, skipping prompt injection");
    }

    // Create the current user message with attachments
    let current_user_message = if attachments.is_empty() {
        crate::llm::Message::new(crate::llm::Role::User, input_text.clone())
    } else {
        crate::llm::Message::new_with_attachments(
            crate::llm::Role::User,
            input_text.clone(),
            attachments,
        )
    };

    tracing::debug!(
        message_role = ?current_user_message.role,
        content_length = current_user_message.content.len(),
        attachment_count = current_user_message.attachments.as_ref().map(|a| a.len()).unwrap_or(0),
        "Final LLM message with attachments"
    );

    llm_messages.push(current_user_message.clone());

    // Clear attached files after processing
    app.attachment_state.attached_files.clear();

    // === DESKTOP CONTEXT MANAGEMENT ===
    // Check token count and trigger summarization if needed
    handle_context_management(app, &mut llm_messages, &current_user_message)?;

    // Store the prepared messages for the subscription to use
    app.attachment_state.pending_llm_messages = Some(llm_messages);

    // Start streaming LLM response
    let streaming_id = uuid::Uuid::new_v4();
    app.current_streaming_id = Some(streaming_id);
    app.is_streaming = true;
    // Initialize typing indicator animation
    app.chat_page.typing_indicator_start_time = Some(cosmic::iced::time::Instant::now());
    app.chat_page.typing_indicator_progress = 0.0;
    
    // Play typing sound
    crate::ui::audio::AudioService::play_sound("typing.mp3");

    // Store the last user message for retry functionality
    app.chat_page.last_user_message = Some(input_text.clone());

    None
}

/// Handle context management and summarization
fn handle_context_management(
    app: &mut CosmicLlmApp,
    llm_messages: &mut Vec<crate::llm::Message>,
    current_user_message: &crate::llm::Message,
) -> Option<()> {
    let profile = app.config.get_default_profile()?;
    let conv_id = app.conversation_state.current_conversation_id?;
    let llm_client = app.llm_client.clone();
    let storage = &app.storage;
    
    // Use ContextService to check and trigger summarization
    // Desktop needs to block on async operation
    let result = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async {
            crate::services::ContextService::check_and_trigger_summarization_desktop(
                llm_messages,
                conv_id,
                storage,
                &llm_client,
                profile,
            ).await
        })
    });
    
    match result {
        Ok(()) => {
            tracing::debug!("Context management check completed");
            // If summarization happened, rebuild messages from DB
            if let Ok(updated_msgs) = app.storage.load_conversation_messages(&conv_id.to_string()) {
                // Use MessageConverter service (single source of truth)
                *llm_messages = MessageConverter::db_to_llm(&updated_msgs, true);
                
                // Inject prompts using ContextService
                *llm_messages = match ContextService::inject_prompts(
                    llm_messages.clone(),
                    &app.prompt_manager,
                    profile,
                ) {
                    Ok(messages_with_prompts) => messages_with_prompts,
                    Err(e) => {
                        tracing::error!(error = %e, "Failed to inject prompts after summarization");
                        // Fallback: rebuild without prompts
                        MessageConverter::db_to_llm(&updated_msgs, true)
                    }
                };
                
                // Re-add current user message
                llm_messages.push(current_user_message.clone());
                
                // Rebuild UI messages from DB
                if let Ok(Some(conv)) = app.storage.get_conversation(&conv_id) {
                    app.rebuild_conversation_view(conv);
                }
            }
            Some(())
        }
        Err(e) => {
            tracing::error!(error = %e, "Failed to check/trigger summarization");
            Some(()) // Continue anyway
        }
    }
}

/// Handle StopMessage
fn handle_stop_message(app: &mut CosmicLlmApp) {
    if app.is_streaming {
        // Stop the current streaming
        app.is_streaming = false;
        app.current_streaming_id = None;
        app.attachment_state.pending_llm_messages = None; // Clear prepared messages
        app.chat_page.typing_indicator_start_time = None;
        app.chat_page.typing_indicator_progress = 0.0;

        // Remove any incomplete assistant message
        if let Some(index) = app.tool_call_state.current_ai_message_index {
            if index < app.conversation_state.messages.len() && !app.conversation_state.messages[index].is_user {
                app.conversation_state.messages.remove(index);
            }
        }
        app.tool_call_state.set_current_ai_message_index(None);
    }
}

/// Handle RetryMessage
fn handle_retry_message(app: &mut CosmicLlmApp) -> Option<app::Task<Message>> {
    if let Some(last_msg) = &app.chat_page.last_user_message {
        app.chat_page.input = last_msg.clone();
        // Trigger SendMessage
        return Some(cosmic::task::future(async move { Message::SendMessage }));
    }
    None
}

/// Handle AttachFile - returns a Task for file chooser
fn handle_attach_file(_app: &mut CosmicLlmApp) -> Option<cosmic::app::Task<Message>> {
    use cosmic::dialog::file_chooser::{self, FileFilter};
    use std::sync::Arc;
    
    tracing::debug!("AttachFile message received");
    // Use libcosmic's file chooser
    Some(cosmic::task::future(async move {
        // Create file filters for supported file types
        let text_filter = FileFilter::new("Text files")
            .extension("txt")
            .extension("md")
            .extension("json")
            .extension("xml")
            .extension("csv")
            .extension("log")
            .extension("yaml")
            .extension("yml")
            .extension("rs")
            .extension("py")
            .extension("js")
            .extension("ts")
            .extension("html")
            .extension("css");

        let image_filter = FileFilter::new("Image files")
            .extension("jpg")
            .extension("jpeg")
            .extension("png")
            .extension("gif")
            .extension("bmp")
            .extension("webp")
            .extension("svg");

        let document_filter = FileFilter::new("Document files")
            .extension("pdf")
            .extension("doc")
            .extension("docx")
            .extension("xls")
            .extension("xlsx")
            .extension("ppt")
            .extension("pptx");

        let dialog = file_chooser::open::Dialog::new()
            .title("Select File to Attach")
            .filter(text_filter)
            .filter(image_filter)
            .filter(document_filter);

        match dialog.open_file().await {
            Ok(response) => {
                let url = response.url();
                if let Ok(path) = url.to_file_path() {
                    Message::FileSelected(path.to_string_lossy().to_string())
                } else {
                    Message::FileChooserError(Arc::new(
                        file_chooser::Error::UrlAbsolute,
                    ))
                }
            }
            Err(file_chooser::Error::Cancelled) => Message::FileChooserCancelled,
            Err(why) => Message::FileChooserError(Arc::new(why)),
        }
    }))
}

