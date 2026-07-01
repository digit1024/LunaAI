//! Chat message handlers
//!
//! Handles chat-related messages: SendMessage, StopMessage, InputChanged, etc.

use cosmic::app;
use cosmic::widget::text_editor;
use crate::ui::app::{LunaThinApp, Message, ConnectionStatus, ChatMessage, ImageState};

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
            if app.is_current_streaming() {
                if let Some(cid) = app.current_conversation_id.clone() {
                    app.send_command(crate::server::dto::ClientCommand::StopStreaming {
                        conversation_id: Some(cid),
                    });
                }
            }
            None
        }
        Message::ToggleChatMenu => {
            app.chat_menu_open = !app.chat_menu_open;
            None
        }
        Message::CloseRecalledMemories => {
            app.recalled_memories_popup = None;
            None
        }
        Message::ShowRecalledMemories(message_id) => {
            app.recalled_memories_popup = Some(message_id);
            None
        }
        Message::CloseChatMenu => {
            app.chat_menu_open = false;
            None
        }
        Message::SummarizeConversation => {
            app.chat_menu_open = false;
            match app.current_conversation_id.clone() {
                None => app.inline_info = Some("No active conversation.".into()),
                Some(id) => {
                    app.send_command(crate::server::dto::ClientCommand::SummarizeConversation {
                        conversation_id: id,
                    });
                    app.inline_info = Some("Compact requested.".into());
                }
            }
            None
        }
        Message::ResumeAgent => {
            app.chat_menu_open = false;
            match app.current_conversation_id.clone() {
                None => app.inline_info = Some("No active conversation.".into()),
                Some(id) => {
                    if app.is_current_streaming() {
                        app.inline_info = Some("Stop streaming before resuming.".into());
                    } else {
                        app.send_command(crate::server::dto::ClientCommand::ResumeAgent {
                            conversation_id: id,
                        });
                        app.inline_info = Some("Resume agent requested.".into());
                    }
                }
            }
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
                    cosmic::Action::App,
                ))
            } else {
                None
            }
        }
        Message::UploadSuccess { uid, original_name, .. } => {
            app.pending_attachment_ids.push(uid);
            app.messages.push(ChatMessage::user(format!(
                "📎 Attached: {} (send a message to include it)",
                original_name
            )));
            None
        }
        Message::FileUploadError(error) => {
            app.inline_error = Some(format!("File upload failed: {}", error));
            None
        }
        Message::CopyMessage(content) => {
            tracing::info!("Copying {} bytes to clipboard", content.len());
            Some(cosmic::iced::clipboard::write(content))
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
        Message::DismissConnectionWarning => {
            app.connection_warning = None;
            None
        }
        Message::DownloadImage { url, title } => handle_download_image(app, url, title),
        Message::ImageSaved(path) => {
            app.inline_info = Some(format!("Image saved to {path}"));
            None
        }
        Message::ImageSaveError(error) => {
            if error != "Save cancelled" {
                app.inline_error = Some(error);
            }
            None
        }
        _ => None, // Not a chat message
    }
}

/// Handle SendMessage - the main chat handler
fn handle_send_message(app: &mut LunaThinApp) -> Option<app::Task<Message>> {
    let has_pending = !app.pending_attachment_ids.is_empty();
    if app.connection_status != ConnectionStatus::Connected {
        return None;
    }
    if app.input_text.trim().is_empty() && !has_pending {
        return None;
    }

    // Save content before clearing
    let message_content = app.input_text.clone();
    let attachment_ids = if app.pending_attachment_ids.is_empty() {
        None
    } else {
        Some(std::mem::take(&mut app.pending_attachment_ids))
    };

    // Clear input immediately when message is sent
    app.input_text.clear();
    app.chat_page.input_content = text_editor::Content::new();

    // Reset assistant bubble tracking for new conversation turn
    app.current_assistant_bubble_id = None;

    let bubble_text = if message_content.trim().is_empty() {
        "(attachment)".to_string()
    } else {
        message_content.clone()
    };
    app.messages.push(ChatMessage::user(bubble_text));

    crate::ui::audio::AudioService::play_sound("sent.mp3");

    app.send_command(crate::server::dto::ClientCommand::SendMessage {
        conversation_id: app.current_conversation_id.clone(),
        content: message_content,
        attachment_ids,
        internal: if app.current_conversation_id.is_none() && app.new_chat_internal {
            Some(true)
        } else {
            None
        },
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
        cosmic::Action::App,
    ))
}

fn handle_download_image(
    app: &mut LunaThinApp,
    url: String,
    title: String,
) -> Option<app::Task<Message>> {
    let Some(state) = app.image_cache.get(&url) else {
        app.inline_error = Some("Image is not ready to download yet".to_string());
        return None;
    };
    let Some(bytes) = state.download_bytes() else {
        app.inline_error = Some("Image is not ready to download yet".to_string());
        return None;
    };

    let is_svg = matches!(state, ImageState::Svg(_));
    let default_name = suggest_image_save_name(&url, &title, bytes, is_svg);
    let bytes = bytes.to_vec();

    Some(app::Task::perform(
        async move {
            use cosmic::dialog::file_chooser::{self, FileFilter};

            let mut filter = FileFilter::new("Image files");
            for ext in ["png", "jpg", "jpeg", "gif", "webp", "svg", "bmp"] {
                filter = filter.extension(ext);
            }

            let dialog = file_chooser::save::Dialog::new()
                .title("Save Image".to_string())
                .file_name(default_name)
                .filter(filter);

            match dialog.save_file().await {
                Ok(response) => match response.url() {
                    Some(file_url) => match file_url.to_file_path() {
                        Ok(path) => match std::fs::write(&path, &bytes) {
                            Ok(()) => Message::ImageSaved(path.to_string_lossy().to_string()),
                            Err(e) => Message::ImageSaveError(format!("Failed to write file: {e}")),
                        },
                        Err(_) => Message::ImageSaveError("Invalid save path".to_string()),
                    },
                    None => Message::ImageSaveError("Save cancelled".to_string()),
                },
                Err(file_chooser::Error::Cancelled) => {
                    Message::ImageSaveError("Save cancelled".to_string())
                }
                Err(why) => Message::ImageSaveError(format!("Save dialog error: {why}")),
            }
        },
        cosmic::Action::App,
    ))
}

fn suggest_image_save_name(url: &str, title: &str, bytes: &[u8], is_svg: bool) -> String {
    if let Some(name) = title
        .trim()
        .split(['/', '\\'])
        .next_back()
        .filter(|name| !name.is_empty() && name.contains('.'))
    {
        return sanitize_filename(name);
    }

    if let Some(name) = url
        .strip_prefix("luna-static:")
        .or_else(|| url.strip_prefix("file://"))
        .and_then(|rest| rest.split(['/', '\\']).next_back())
        .filter(|name| !name.is_empty())
    {
        return sanitize_filename(name);
    }

    if let Ok(parsed) = url::Url::parse(url) {
        if let Some(segments) = parsed.path_segments() {
            if let Some(name) = segments.filter(|s| !s.is_empty()).next_back() {
                if name.contains('.') {
                    return sanitize_filename(name);
                }
            }
        }
    }

    let ext = guess_image_extension(bytes, is_svg);
    format!("image.{ext}")
}

fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn guess_image_extension(bytes: &[u8], is_svg: bool) -> &'static str {
    if is_svg {
        return "svg";
    }
    if bytes.starts_with(b"\x89PNG") {
        "png"
    } else if bytes.len() >= 3 && bytes[0..3] == [0xFF, 0xD8, 0xFF] {
        "jpg"
    } else if bytes.starts_with(b"GIF8") {
        "gif"
    } else if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        "webp"
    } else if bytes.starts_with(b"BM") {
        "bmp"
    } else {
        "png"
    }
}

