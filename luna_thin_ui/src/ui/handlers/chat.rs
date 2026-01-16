//! Chat message handlers
//!
//! Handles chat-related messages: SendMessage, StopMessage, InputChanged, etc.

use cosmic::app;
use cosmic::widget::text_editor;
use crate::ui::app::{LunaThinApp, Message, ConnectionStatus, PendingAttachment, ChatMessage};

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
            app.pending_attachments.push(PendingAttachment {
                file_path: path.clone(),
                file_id: None,
                uploading: true,
                error: None,
            });

            if let Some(ref file_client) = app.file_client {
                let client = file_client.clone();
                let path_clone = path.clone();
                Some(app::Task::perform(
                    async move {
                        match client.upload_file(&path_clone).await {
                            Ok(attachment) => Message::FileUploaded(path_clone, attachment.file_id),
                            Err(e) => Message::FileUploadError(e.to_string()),
                        }
                    },
                    |msg| cosmic::Action::App(msg),
                ))
            } else {
                None
            }
        }
        Message::FileUploaded(path, file_id) => {
            if let Some(attachment) = app.pending_attachments.iter_mut().find(|a| a.file_path == path) {
                attachment.file_id = Some(file_id);
                attachment.uploading = false;
            }
            None
        }
        Message::FileUploadError(error) => {
            app.inline_error = Some(format!("File upload failed: {}", error));
            None
        }
        Message::RemoveFile(path) => {
            app.pending_attachments.retain(|a| a.file_path != path);
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
                // Set input to message content
                app.input_text = msg.content.clone();
                tracing::debug!("Set input text to: {} chars", msg.content.len());
                
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

    // Play sent sound
    crate::ui::audio::AudioService::play_sound("sent.mp3");

    let attachment_ids: Vec<String> = app
        .pending_attachments
        .iter()
        .filter_map(|a| a.file_id.clone())
        .collect();

    app.send_command(crate::server::dto::ClientCommand::SendMessage {
        conversation_id: app.current_conversation_id.clone(),
        content: message_content,
        attachment_ids: if attachment_ids.is_empty() { None } else { Some(attachment_ids) },
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

