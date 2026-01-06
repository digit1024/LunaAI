//! D-Bus message handlers
//!
//! Handles D-Bus TTS/STT related messages.

use cosmic::app;
use cosmic::widget::text_editor;
use std::sync::Arc;

use crate::ui::app::{CosmicLlmApp, ConversationModeState, Message};
use crate::ui::helpers::utils::strip_markdown_for_tts;

#[cfg(feature = "ttsandstt")]
pub fn handle_dbus_messages(app: &mut CosmicLlmApp, message: &Message) -> Option<app::Task<Message>> {
    match message {
        Message::DbusServiceAvailable(available) => {
            let was_available = app.dbus_ttsstt_available;
            app.dbus_ttsstt_available = *available;
            if *available && !was_available {
                tracing::info!("D-Bus TTS/STT service is now available");
            } else if !*available && was_available {
                tracing::warn!("D-Bus TTS/STT service is no longer available");
                let mut guard = app.dbus_ttsstt_status.blocking_write();
                guard.clear();
                app.stt_listening_initiated = false;
            }
            Some(app::Task::none())
        }
        Message::DbusStatusChanged(status) => {
            let old_status = app.dbus_ttsstt_status_display.clone();
            if old_status != *status {
                tracing::debug!(
                    old_status = %old_status,
                    new_status = %status,
                    "D-Bus status changed, buttons will update"
                );
                app.dbus_ttsstt_status_display = status.clone();
                
                // Handle conversation mode state synchronization
                #[cfg(feature = "ttsandstt")]
                if app.is_conversation_mode_active {
                    match status.as_str() {
                        "listening" => {
                            app.conversation_mode_state = ConversationModeState::Listening;
                        }
                        "speaking" => {
                            app.conversation_mode_state = ConversationModeState::Speaking;
                        }
                        "processing" => {
                            app.conversation_mode_state = ConversationModeState::Processing;
                        }
                        "idle" => {
                            // If we were speaking and now idle, TTS completed
                            if app.conversation_mode_state == ConversationModeState::Speaking {
                                // TTS completed, trigger completion handler (which will add delay and resume listening)
                                // Check if STT is already listening to avoid duplicates
                                // But allow it if we're transitioning from speaking (TTS just finished)
                                if !app.stt_listening_initiated {
                                    return Some(cosmic::Task::perform(
                                        async move {
                                            cosmic::Action::App(Message::ConversationModeTtsComplete)
                                        },
                                        |msg| msg,
                                    ));
                                } else {
                                    tracing::debug!("TTS completed but STT already initiated, skipping duplicate trigger");
                                }
                            }
                            // If we were listening and now idle, it means STT finished (pause detected)
                            // Don't immediately resume - wait for the STT result to come through call_stt
                            // The call_stt async task will return the text, which will trigger ConversationModeSttResult
                            if app.conversation_mode_state == ConversationModeState::Listening {
                                // Just mark that we're no longer listening, but don't resume yet
                                // The STT result will come from the call_stt task
                                app.stt_listening_initiated = false;
                            }
                            app.playing_message_id = None;
                        }
                        _ => {}
                    }
                } else {
                    // Not in conversation mode, handle normally
                    if *status == "idle" {
                        app.stt_listening_initiated = false;
                        app.playing_message_id = None;
                    }
                }
                
                let mut guard = app.dbus_ttsstt_status.blocking_write();
                *guard = status.clone();
            }
            Some(app::Task::none())
        }
        Message::CheckDbusService => {
            let client = app.dbus_ttsstt_client.clone();
            Some(cosmic::Task::perform(
                async move {
                    let available = client.check_availability().await;
                    cosmic::Action::App(Message::DbusServiceAvailable(available))
                },
                |msg| msg,
            ))
        }
        Message::PlayMessageTts(message_idx) => {
            if let Some(msg) = app.conversation_state.messages.get(*message_idx) {
                app.playing_message_id = Some(*message_idx);
                
                // If in conversation mode, set state to Speaking
                #[cfg(feature = "ttsandstt")]
                if app.is_conversation_mode_active {
                    app.conversation_mode_state = ConversationModeState::Speaking;
                }
                
                let text = strip_markdown_for_tts(&msg.content);
                
                tracing::debug!(
                    original_length = msg.content.len(),
                    cleaned_length = text.len(),
                    original_preview = msg.content.chars().take(100).collect::<String>(),
                    cleaned_preview = text.chars().take(100).collect::<String>(),
                    "TTS text cleaning: original vs cleaned"
                );
                
                let client = app.dbus_ttsstt_client.clone();
                #[cfg(feature = "ttsandstt")]
                let is_conversation_mode = app.is_conversation_mode_active;
                #[cfg(not(feature = "ttsandstt"))]
                let is_conversation_mode = false;
                
                Some(cosmic::Task::perform(
                    async move {
                        if let Err(e) = client.call_tts(&text, "en-US").await {
                            tracing::error!(error = %e, "Failed to call TTS");
                            return cosmic::Action::App(Message::DismissError);
                        }
                        // If in conversation mode, TTS completion will be handled by D-Bus status signal
                        // The status will change to "idle" when TTS completes, which triggers ConversationModeTtsComplete
                        if is_conversation_mode {
                            // The status signal will handle the completion
                            cosmic::Action::App(Message::DismissError)
                        } else {
                            cosmic::Action::App(Message::DismissError)
                        }
                    },
                    |msg| msg,
                ))
            } else {
                Some(app::Task::none())
            }
        }
        Message::StopMessageTts => {
            app.playing_message_id = None;
            let client = app.dbus_ttsstt_client.clone();
            Some(cosmic::Task::perform(
                async move {
                    if let Err(e) = client.stop().await {
                        tracing::error!(error = %e, "Failed to stop TTS");
                    }
                    cosmic::Action::App(Message::DismissError)
                },
                |msg| msg,
            ))
        }
        Message::StartStt => {
            app.stt_listening_initiated = true;
            let client = app.dbus_ttsstt_client.clone();
            Some(cosmic::Task::perform(
                async move {
                    match client.call_stt("en-US", 2.0).await {
                        Ok(text) => cosmic::Action::App(Message::SttResult(text)),
                        Err(e) => {
                            tracing::error!(error = %e, "Failed to call STT");
                            cosmic::Action::App(Message::DismissError)
                        }
                    }
                },
                |msg| msg,
            ))
        }
        Message::StopStt => {
            app.stt_listening_initiated = false;
            let client = app.dbus_ttsstt_client.clone();
            Some(cosmic::Task::perform(
                async move {
                    if let Err(e) = client.stop().await {
                        tracing::error!(error = %e, "Failed to stop STT");
                    }
                    cosmic::Action::App(Message::DismissError)
                },
                |msg| msg,
            ))
        }
        Message::SttResult(text) => {
            app.stt_listening_initiated = false;
            app.chat_page.input_content.perform(text_editor::Action::Edit(
                text_editor::Edit::Paste(Arc::new(text.clone())),
            ));
            app.chat_page.input = app.chat_page.input_content.text();
            Some(app::Task::none())
        }
        _ => None,
    }
}

#[cfg(not(feature = "ttsandstt"))]
pub fn handle_dbus_messages(_app: &mut CosmicLlmApp, message: &Message) -> Option<app::Task<Message>> {
    match message {
        Message::DbusServiceAvailable(_) | 
        Message::DbusStatusChanged(_) | 
        Message::CheckDbusService | 
        Message::PlayMessageTts(_) | 
        Message::StopMessageTts | 
        Message::StartStt | 
        Message::StopStt | 
        Message::SttResult(_) => {
            Some(app::Task::none())
        }
        _ => None,
    }
}

