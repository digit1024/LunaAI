//! Input area - multi-line text editor, attachments, send/stop button

use cosmic::{
    iced::{keyboard, Background, Color, Length},
    widget::{self, button, column, container, row, text, text_editor, Space},
    Element,
};

use crate::ui::app::{LunaThinApp, Message};

pub fn input_area(app: &LunaThinApp) -> Element<Message> {
    container(
        column()
            .push(
                // Text editor (multi-line)
                container(
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
                            },
                        )))
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
                        }),
                )
                .width(Length::Fill),
            )
            .push(
                // Bottom row: Attach button, attachments, send/stop button
                {
                    let mut bottom_row = row()
                        .push(
                            // Attach file button with icon
                            button::icon(crate::ui::icons::get_handle("mail-attachment-symbolic", 16))
                                .on_press(Message::AttachFile),
                        )
                        .push(Space::with_width(8));

                    // Pending attachments
                    if !app.pending_attachments.is_empty() {
                        let mut attachments_col = column().spacing(4);
                        for attachment in &app.pending_attachments {
                            let file_name = std::path::Path::new(&attachment.file_path)
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("file");

                            let status_icon = if attachment.uploading {
                                "⏳"
                            } else if attachment.file_id.is_some() {
                                "✓"
                            } else if attachment.error.is_some() {
                                "❌"
                            } else {
                                "📎"
                            };

                            attachments_col = attachments_col.push(
                                row()
                                    .push(text(format!("{} {}", status_icon, file_name)).size(12))
                                    .push(Space::with_width(Length::Fill))
                                    .push(
                                        button::text("✕")
                                            .on_press(Message::RemoveFile(
                                                attachment.file_path.clone(),
                                            ))
                                            .class(cosmic::style::Button::Text)
                                            .padding([2, 6]),
                                    )
                                    .spacing(8)
                                    .align_y(cosmic::iced::Alignment::Center),
                            );
                        }
                        bottom_row = bottom_row.push(attachments_col);
                    }

                    bottom_row = bottom_row.push(Space::with_width(Length::Fill));

                    // Send/Stop button with icons
                    if app.is_streaming {
                        bottom_row = bottom_row.push(
                            button::icon(crate::ui::icons::get_handle("process-stop-symbolic", 16))
                                .on_press(Message::StopMessage)
                                .class(widget::button::ButtonClass::Destructive),
                        );
                    } else {
                        bottom_row = bottom_row.push(
                            button::icon(crate::ui::icons::get_handle("send-symbolic", 16))
                                .on_press(Message::SendMessage)
                                .class(widget::button::ButtonClass::Suggested),
                        );
                    }

                    bottom_row.spacing(8).align_y(cosmic::iced::Alignment::Center)
                },
            )
            .spacing(8),
    )
    .padding(16)
    .width(Length::Fill)
    .class(cosmic::style::Container::Card)
    .into()
}

