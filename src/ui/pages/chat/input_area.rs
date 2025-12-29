use crate::ui::app::{CosmicLlmApp, Message};
use cosmic::{
    Element, iced::{Background, Color, Length, keyboard}, widget::{self, text_editor}
};

pub fn input_area(app: &CosmicLlmApp) -> Element<Message> {
    cosmic::widget::container(
        cosmic::widget::column::with_capacity(2)
            .push(
                // Text editor for message (multi-line) - full width on top
                cosmic::widget::container(
                    text_editor(&app.chat_page.input_content)
                    .class(cosmic::theme::iced::TextEditor::Custom(Box::new(
                        |_theme, _status| text_editor::Style {
                            background: Background::Color(Color::TRANSPARENT),
                            border: cosmic::iced::Border {
                                width: 0.0,
                                radius: 0.0.into(),
                                color: Color::TRANSPARENT,
                            },
                            icon: cosmic::theme::active().cosmic().on_bg_color().into(),
                            placeholder: cosmic::theme::active().cosmic().on_bg_color().into(),
                            value: cosmic::theme::active().cosmic().on_bg_color().into(),
                            selection: Color::from_rgba(1.0, 1.0, 1.0, 0.3),
                        }
                    )))
                        .id(app.chat_page.input_id.clone())
                        .on_action(Message::InputActionPerformed)
                        .height(Length::Shrink)
                        .padding(12)
                        .placeholder("Send message to Luna AI...")
                        .key_binding(|key_press| {
                            match key_press.key.as_ref() {
                                keyboard::Key::Named(keyboard::key::Named::Enter)
                                    if key_press.modifiers.shift() =>
                                {
                                    // Shift+Enter for new line
                                    text_editor::Binding::from_key_press(key_press)
                                }
                                keyboard::Key::Named(keyboard::key::Named::Enter) => {
                                    // Enter to send message
                                    Some(text_editor::Binding::Custom(Message::SendMessage))
                                }
                                _ => text_editor::Binding::from_key_press(key_press),
                            }
                        })


                        ,
                )
                .width(Length::Fill),
            )
            .push(
                // Bottom row: Attachments <----> Send button
                {
                    let mut row = cosmic::widget::row::with_capacity(4);
                    // Attach file button
                    row = row.push(
                        widget::button::icon(crate::ui::icons::get_handle(
                            "mail-attachment-symbolic",
                            16,
                        ))
                        .on_press(Message::AttachFile),
                    );
                    // Microphone/Stop button for STT (only if feature enabled and D-Bus service is available)
                    #[cfg(feature = "ttsandstt")]
                    {
                        let service_available = app.dbus_ttsstt_available;
                        let status = &app.dbus_ttsstt_status_display;
                        let we_initiated = app.stt_listening_initiated;
                        let is_listening = status == "listening";
                        let is_processing = status == "processing";
                        
                        // Disable button if:
                        // - Service not available
                        // - Streaming
                        // - Service is listening/processing but we didn't initiate it (another app is using it)
                        let button_enabled = service_available 
                            && !app.is_streaming 
                            && !((is_listening || is_processing) && !we_initiated);
                        
                        if button_enabled {
                            if is_listening && we_initiated {
                                // Show stop button when we're listening
                                row = row.push(
                                    widget::button::icon(crate::ui::icons::get_handle(
                                        "process-stop-symbolic",
                                        16,
                                    ))
                                    .on_press(Message::StopStt),
                                );
                            } else {
                                // Show microphone button when not listening or when we can start
                                let mic_button = widget::button::icon(crate::ui::icons::get_handle(
                                    "audio-input-microphone-symbolic",
                                    16,
                                ))
                                .on_press(Message::StartStt);
                                
                                row = row.push(mic_button);
                            }
                        }
                        // Note: When service is busy for another app, we simply don't show the button
                        // This is cleaner than showing a disabled button
                    }
                    row
                }
                    .push(
                        // Attached files display (left side, expands to fill)
                        if !app.attachment_state.attached_files.is_empty() {
                            cosmic::widget::column::with_children(
                                app.attachment_state.attached_files
                                    .iter()
                                    .map(|file_path| {
                                        let file_name = std::path::Path::new(file_path)
                                            .file_name()
                                            .and_then(|name| name.to_str())
                                            .unwrap_or(file_path);

                                        cosmic::widget::row::with_children(vec![
                                            cosmic::widget::text(format!("📎 {}", file_name))
                                                .size(12)
                                                .into(),
                                            cosmic::widget::Space::with_width(Length::Fill).into(),
                                            cosmic::widget::button::standard("✕")
                                                .on_press(Message::RemoveFile(file_path.to_string()))
                                                .padding([4, 8])
                                                .into(),
                                        ])
                                        .spacing(8)
                                        .align_y(cosmic::iced::Alignment::Center)
                                        .into()
                                    })
                                    .collect(),
                            )
                            .spacing(4)
                        } else {
                            cosmic::widget::column::with_capacity(0)
                        }
                        .width(Length::Fill),
                    )
                    .push(
                        // Send/Stop button (right side)
                        if app.is_streaming {
                            // Stop button when streaming
                            widget::button::icon(crate::ui::icons::get_handle(
                                "process-stop-symbolic",
                                16,
                            ))
                            .class(widget::button::ButtonClass::Destructive)
                            .on_press(Message::StopMessage)
                        } else {
                            // Send button when not streaming
                            widget::button::icon(crate::ui::icons::get_handle("send-symbolic", 16))
                                .class(widget::button::ButtonClass::Suggested)
                                .on_press(Message::SendMessage)
                        },
                    )
                    .spacing(8)
                    .align_y(cosmic::iced::Alignment::Center),
            )
            .spacing(8),
    )
    .padding(16)
    .width(Length::Fill)
    .class(cosmic::style::Container::Card)
    .into()
}
