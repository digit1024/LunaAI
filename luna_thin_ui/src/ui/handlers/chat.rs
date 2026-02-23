//! Chat message handlers
//!
//! Handles chat-related messages: SendMessage, StopMessage, InputChanged, etc.

use cosmic::app;
use cosmic::widget::text_editor;
use crate::ui::app::{LunaThinApp, Message, ConnectionStatus, ChatMessage};

/// Handle chat-related messages
pub fn handle_chat_messages(
    app: &mut LunaThinApp,
    message: Message,
) -> Option<app::Task<Message>> {
    match message {
        Message::InputChanged(text) => {
            app.input_text = text;
            None
        }
        Message::InputActionPerformed(action) => {
            app.chat_page.input_content.perform(action);
            // Sync input_text with editor content
            app.input_text = app.chat_page.input_content.text();
            None
        }
        Message::SendMessage => handle_send_message(app),
        Message::StopMessage => {
            app.send_command(crate::server::dto::ClientCommand::StopStreaming {
                conversation_id: app.current_conversation_id.clone(),
            });
            None
        }
        Message::AttachFile => handle_attach_file(app),
        Message::FileSelected(path) => {
            if let Some(ref file_client) = app.file_client {
                let client = file_client.clone();
                let path_clone = path.clone();
                let conv_id = app.current_conversation_id.clone();
                Some(app::Task::perform(
                    async move {
                        match client
                            .upload_file(&path_clone, conv_id.as_deref())
                            .await
                        {
                            Ok(r) => Message::UploadSuccess {
                                uid: r.uid,
                                original_name: r.original_name,
                                stored_path: r.stored_path,
                            },
                            Err(e) => Message::FileUploadError(e.to_string()),
                        }
                    },
                    |msg| cosmic::Action::App(msg),
                ))
            } else {
                None
            }
        }
        Message::UploadSuccess {
            stored_path,
            original_name,
            ..
        } => {
            let template = format!(
                "User uploaded file {}. It has been stored under {}. You should obtain content of this file if possible.",
                original_name, stored_path
            );
            app.messages.push(ChatMessage::user(template.clone()));
            crate::ui::audio::AudioService::play_sound("sent.mp3");
            app.send_command(crate::server::dto::ClientCommand::SendMessage {
                conversation_id: app.current_conversation_id.clone(),
                content: template,
            });
            None
        }
        Message::FileUploadError(error) => {
            app.inline_error = Some(format!("File upload failed: {}", error));
            None
        }
        Message::CopyMessage(content) => {
            // Copy to clipboard
            match arboard::Clipboard::new() {
                Ok(mut clipboard) => {
                    if let Err(e) = clipboard.set_text(&content) {
                        tracing::error!("Failed to copy to clipboard: {}", e);
                    } else {
                        tracing::info!("Copied {} bytes to clipboard", content.len());
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to initialize clipboard: {}", e);
                }
            }
            None
        }
        Message::RegenerateMessage(message_id) => {
            // Log regenerate click - will be used in future to regenerate answer
            tracing::info!("Regenerate message clicked: {}", message_id);
            None
        }
        Message::RetryMessage(message_id) => {
            tracing::info!("Retry message clicked: {}", message_id);
            // Find the message and put its content in input, then truncate conversation
            if let Some(msg) = app.messages.iter().find(|m| m.id == message_id) {
                // Store input text to restore after conversation reload
                app.pending_retry_input = Some(msg.content.clone());
                tracing::debug!("Stored retry input text: {} chars", msg.content.len());
                
                // Get conversation ID
                if let Some(conv_id) = &app.current_conversation_id {
                    tracing::info!("Sending TruncateConversation command for conversation {} at message {}", conv_id, message_id);
                    // Send truncate command
                    app.send_command(crate::server::dto::ClientCommand::TruncateConversation {
                        conversation_id: conv_id.clone(),
                        message_id: message_id.clone(),
                    });
                } else {
                    tracing::warn!("No current conversation ID, cannot truncate");
                }
            } else {
                tracing::warn!("Message {} not found for retry", message_id);
            }
            None
        }
        Message::Tick(_) => {
            // Update typing indicator animation
            const TYPING_PROGRESS_STEP: f32 = 0.1;
            app.chat_page.typing_indicator_progress = 
                (app.chat_page.typing_indicator_progress + TYPING_PROGRESS_STEP) % 1.0;
            None
        }
        Message::DismissError => {
            app.inline_error = None;
            None
        }
        Message::DismissInfo => {
            app.inline_info = None;
            None
        }
        _ => None, // Not a chat message
    }
}

/// Handle SendMessage - the main chat handler
fn handle_send_message(app: &mut LunaThinApp) -> Option<app::Task<Message>> {
    if app.input_text.trim().is_empty() || app.connection_status != ConnectionStatus::Connected {
        return None;
    }

    // Save content before clearing
    let message_content = app.input_text.clone();
    
    // Clear input immediately when message is sent
    app.input_text.clear();
    app.chat_page.input_content = text_editor::Content::new();

    // Reset assistant bubble tracking for new conversation turn
    app.current_assistant_bubble_id = None;
    
    // Add user message immediately (only if not empty)
    if !message_content.trim().is_empty() {
        app.messages.push(ChatMessage::user(message_content.clone()));
    }

    crate::ui::audio::AudioService::play_sound("sent.mp3");

    app.send_command(crate::server::dto::ClientCommand::SendMessage {
        conversation_id: app.current_conversation_id.clone(),
        content: message_content,
    });

    None
}

/// Handle AttachFile - returns a Task for file chooser
fn handle_attach_file(_app: &mut LunaThinApp) -> Option<app::Task<Message>> {
    use cosmic::dialog::file_chooser::{self, FileFilter};
    
    tracing::debug!("AttachFile message received");
    // Use libcosmic's file chooser
    Some(app::Task::perform(
        async move {
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
                        Message::FileUploadError("Failed to convert URL to file path".to_string())
                    }
                }
                Err(file_chooser::Error::Cancelled) => Message::FileUploadError("File selection cancelled".to_string()),
                Err(why) => Message::FileUploadError(format!("File chooser error: {}", why)),
            }
        },
        |msg| cosmic::Action::App(msg),
    ))
}

