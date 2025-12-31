//! Conversation mode message handlers
//!
//! Handles conversation mode messages: StartConversationMode, StopConversationMode, etc.

use cosmic::app;
use crate::ui::app::{CosmicLlmApp, ConversationModeState, Message};

#[cfg(feature = "ttsandstt")]
pub fn handle_conversation_mode_messages(
    app: &mut CosmicLlmApp,
    message: &Message,
) -> Option<app::Task<Message>> {
    match message {
        Message::StartConversationMode => {
            handle_start_conversation_mode(app)
        }
        Message::StopConversationMode => {
            handle_stop_conversation_mode(app)
        }
        Message::SetConversationModeState(new_state) => {
            app.conversation_mode_state = *new_state;
            Some(app::Task::none())
        }
        Message::ConversationModeSttResult(text) => {
            handle_conversation_mode_stt_result(app, &text)
        }
        Message::ConversationModeTtsComplete => {
            handle_conversation_mode_tts_complete(app)
        }
        _ => None,
    }
}

#[cfg(feature = "ttsandstt")]
fn handle_start_conversation_mode(app: &mut CosmicLlmApp) -> Option<app::Task<Message>> {
    // Check if D-Bus service is available
    if !app.dbus_ttsstt_available {
        tracing::warn!("Cannot start conversation mode: D-Bus TTS/STT service not available");
        app.chat_page.current_error = Some("D-Bus TTS/STT service not available".to_string());
        return Some(app::Task::none());
    }

    // Don't start if already active
    if app.is_conversation_mode_active {
        return Some(app::Task::none());
    }

    // Don't start if streaming
    if app.is_streaming {
        tracing::warn!("Cannot start conversation mode: Already streaming");
        return Some(app::Task::none());
    }

    tracing::info!("Starting conversation mode");
    app.is_conversation_mode_active = true;
    app.conversation_mode_state = ConversationModeState::Listening;
    app.conversation_mode_transcribed_text = None;

    // Start STT listening with pause detection
    // Set flag before starting to prevent duplicates
    app.stt_listening_initiated = true;
    let client = app.dbus_ttsstt_client.clone();
    
    Some(cosmic::Task::perform(
        async move {
            match client.call_stt("en-US", 2.0).await {
                Ok(text) => cosmic::Action::App(Message::ConversationModeSttResult(text)),
                Err(e) => {
                    tracing::error!(error = %e, "Failed to start STT in conversation mode");
                    cosmic::Action::App(Message::StopConversationMode)
                }
            }
        },
        |msg| msg,
    ))
}

#[cfg(feature = "ttsandstt")]
fn handle_stop_conversation_mode(app: &mut CosmicLlmApp) -> Option<app::Task<Message>> {
    if !app.is_conversation_mode_active {
        return Some(app::Task::none());
    }

    tracing::info!("Stopping conversation mode");
    app.is_conversation_mode_active = false;
    app.conversation_mode_state = ConversationModeState::Listening;
    app.conversation_mode_transcribed_text = None;
    app.stt_listening_initiated = false;

    // Stop both STT and TTS
    let client = app.dbus_ttsstt_client.clone();
    Some(cosmic::Task::perform(
        async move {
            let _ = client.stop().await; // Ignore errors
            cosmic::Action::App(Message::DismissError)
        },
        |msg| msg,
    ))
}

#[cfg(feature = "ttsandstt")]
fn handle_conversation_mode_stt_result(
    app: &mut CosmicLlmApp,
    text: &str,
) -> Option<app::Task<Message>> {
    if !app.is_conversation_mode_active {
        return Some(app::Task::none());
    }

    let trimmed_text = text.trim();
    if trimmed_text.is_empty() {
        // No text, resume listening
        return resume_listening(app);
    }

    tracing::info!(text = %trimmed_text, "Conversation mode STT result received, sending to LLM");
    
    // Reset STT listening flag since we got the result
    app.stt_listening_initiated = false;
    
    // Update transcribed text
    app.conversation_mode_transcribed_text = Some(trimmed_text.to_string());
    
    // Switch to processing state
    app.conversation_mode_state = ConversationModeState::Processing;
    
    // Auto-send the message
    // Set input and trigger send
    app.chat_page.input = trimmed_text.to_string();
    app.chat_page.input_content = cosmic::widget::text_editor::Content::with_text(trimmed_text.clone());
    
    tracing::debug!(
        input_set = app.chat_page.input.len(),
        input_preview = app.chat_page.input.chars().take(50).collect::<String>(),
        "Input set, triggering SendMessage"
    );
    
    // The input is already set above, so we can directly trigger SendMessage
    // Use a simple task that will be processed in the next update cycle
    Some(app::Task::perform(
        async move {
            // The input should already be set in app.chat_page.input
            // This task just triggers SendMessage in the next update cycle
            cosmic::Action::App(Message::SendMessage)
        },
        |msg| msg,
    ))
}

#[cfg(feature = "ttsandstt")]
fn handle_conversation_mode_tts_complete(app: &mut CosmicLlmApp) -> Option<app::Task<Message>> {
    if !app.is_conversation_mode_active {
        return Some(app::Task::none());
    }

    // Check if we're still streaming - if so, wait
    if app.is_streaming {
        tracing::debug!("Still streaming, will resume listening after streaming and TTS complete");
        return Some(app::Task::none());
    }

    // Check if STT is already listening - if so, don't start another one
    if app.stt_listening_initiated {
        tracing::debug!("STT already initiated, skipping duplicate STT request");
        return Some(app::Task::none());
    }

    tracing::info!("Conversation mode TTS complete, waiting before resuming listening");
    
    // Update state to Listening (will be shown after delay)
    app.conversation_mode_state = ConversationModeState::Listening;
    
    // Set flag to prevent duplicate STT requests while we wait for the delay
    app.stt_listening_initiated = true;
    
    // Add a delay after TTS completes before resuming listening
    // This ensures the last message finishes playing completely and gives a brief pause
    let client = app.dbus_ttsstt_client.clone();
    Some(app::Task::perform(
        async move {
            // Wait 500ms after TTS completes before resuming listening
            // This ensures audio has fully finished and gives user a moment
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            
            // Now resume listening
            match client.call_stt("en-US", 2.0).await {
                Ok(text) => cosmic::Action::App(Message::ConversationModeSttResult(text)),
                Err(e) => {
                    tracing::error!(error = %e, "Failed to resume STT after TTS delay");
                    cosmic::Action::App(Message::StopConversationMode)
                }
            }
        },
        |msg| msg,
    ))
}

#[cfg(feature = "ttsandstt")]
fn resume_listening(app: &mut CosmicLlmApp) -> Option<app::Task<Message>> {
    if !app.is_conversation_mode_active {
        return Some(app::Task::none());
    }

    // Check if we're still streaming - if so, wait
    if app.is_streaming {
        tracing::debug!("Still streaming, will resume listening after completion");
        return Some(app::Task::none());
    }

    // Check if STT is already listening - if so, don't start another one
    if app.stt_listening_initiated {
        tracing::debug!("STT already initiated in resume_listening, skipping duplicate STT request");
        return Some(app::Task::none());
    }

    tracing::debug!("Resuming listening in conversation mode");
    
    app.conversation_mode_state = ConversationModeState::Listening;
    // Set flag before starting to prevent duplicates
    app.stt_listening_initiated = true;
    
    let client = app.dbus_ttsstt_client.clone();
    Some(cosmic::Task::perform(
        async move {
            match client.call_stt("en-US", 2.0).await {
                Ok(text) => cosmic::Action::App(Message::ConversationModeSttResult(text)),
                Err(e) => {
                    tracing::error!(error = %e, "Failed to resume STT in conversation mode");
                    cosmic::Action::App(Message::StopConversationMode)
                }
            }
        },
        |msg| msg,
    ))
}

#[cfg(not(feature = "ttsandstt"))]
pub fn handle_conversation_mode_messages(
    _app: &mut CosmicLlmApp,
    message: &Message,
) -> Option<app::Task<Message>> {
    match message {
        Message::StartConversationMode |
        Message::StopConversationMode |
        Message::SetConversationModeState(_) |
        Message::ConversationModeSttResult(_) |
        Message::ConversationModeTtsComplete => {
            Some(app::Task::none())
        }
        _ => None,
    }
}

