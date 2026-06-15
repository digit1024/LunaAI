//! TTS message handlers
//!
//! Handles TTS-related messages: StartTts, StopTts, TtsStatusChanged

use cosmic::app;
use crate::ui::app::{LunaThinApp, Message, TtsStatus};
use crate::utils::markdown_strip;

/// Handle TTS-related messages
pub fn handle_tts_messages(
    app: &mut LunaThinApp,
    message: Message,
) -> Option<app::Task<Message>> {
    match message {
        Message::StartTts(message_id) => {
            // Find message content
            let msg = app.messages.iter()
                .find(|m| m.id == message_id)?;
            
            // Strip markdown
            let plain_text = markdown_strip::strip_markdown(&msg.content);
            
            if plain_text.is_empty() {
                tracing::warn!("Message content is empty after markdown stripping");
                return None;
            }
            
            // Get language (default to "en-US" for now)
            let language = "en-US".to_string();
            
            // IMPORTANT: Set current_tts_message_id BEFORE starting TTS
            // This ensures the UI immediately shows stop button for this message
            app.current_tts_message_id = Some(message_id.clone());
            app.tts_status = TtsStatus::Speaking;
            
            // Start TTS
            if let Some(ref client) = app.tts_client {
                let client = client.clone();
                return Some(app::Task::perform(
                    async move {
                        match client.speak(plain_text, language).await {
                            Ok(_) => {
                                tracing::debug!("TTS speak request sent successfully");
                                // Status will be updated via DBus signal, but we set it optimistically
                                Message::TtsStatusChanged("speaking".to_string())
                            }
                            Err(e) => {
                                tracing::error!("TTS error: {}", e);
                                Message::TtsStatusChanged("idle".to_string())
                            }
                        }
                    },
                    cosmic::Action::App,
                ));
            } else {
                tracing::warn!("TTS client not available");
                // Reset state if client not available
                app.current_tts_message_id = None;
                app.tts_status = TtsStatus::Idle;
            }
            None
        }
        Message::StopTts => {
            if let Some(ref client) = app.tts_client {
                let client = client.clone();
                return Some(app::Task::perform(
                    async move {
                        match client.stop().await {
                            Ok(_) => {
                                tracing::debug!("TTS stop request sent successfully");
                                Message::TtsStatusChanged("idle".to_string())
                            }
                            Err(e) => {
                                tracing::error!("TTS stop error: {}", e);
                                Message::TtsStatusChanged("idle".to_string())
                            }
                        }
                    },
                    cosmic::Action::App,
                ));
            } else {
                tracing::warn!("TTS client not available for stop");
                // Reset state if client not available
                app.current_tts_message_id = None;
                app.tts_status = TtsStatus::Idle;
            }
            None
        }
        Message::TtsStatusChanged(status) => {
            app.tts_status = match status.as_str() {
                "speaking" => TtsStatus::Speaking,
                _ => TtsStatus::Idle,
            };
            // When TTS becomes idle, clear current message ID
            // This causes the stop button to change back to play button
            if app.tts_status == TtsStatus::Idle {
                app.current_tts_message_id = None;
            }
            None
        }
        _ => None, // Not a TTS message
    }
}

