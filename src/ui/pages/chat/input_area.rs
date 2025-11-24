use cosmic::{
    iced::{Length, keyboard},
    widget::{self, text_editor},
    Element,
};
use crate::ui::app::{Message, CosmicLlmApp};

pub fn input_area(app: &CosmicLlmApp) -> Element<Message> {
    cosmic::widget::container(
        cosmic::widget::column::with_capacity(3)
            .push(
                // Attached files display
                if !app.attached_files.is_empty() {
                    cosmic::widget::column::with_children(
                        app.attached_files.iter().map(|file_path| {
                            let file_name = std::path::Path::new(file_path)
                                .file_name()
                                .and_then(|name| name.to_str())
                                .unwrap_or(file_path);
                            
                            cosmic::widget::row::with_children(vec![
                                cosmic::widget::text(format!("📎 {}", file_name)).size(12).into(),
                                cosmic::widget::Space::with_width(Length::Fill).into(),
                                cosmic::widget::button::standard("✕")
                                    .on_press(Message::RemoveFile(file_path.clone()))
                                    .padding([4, 8])
                                    .into(),
                            ])
                            .spacing(8)
                            .align_y(cosmic::iced::Alignment::Center)
                            .into()
                        }).collect()
                    )
                    .spacing(4)
                } else {
                    cosmic::widget::column::with_children(vec![
                        cosmic::widget::text("").size(12).into()
                    ])
                }
            )
            .push(
                // Input row with buttons inline
                cosmic::widget::row::with_capacity(3)
                    .push(
                        // Attach file button (left side)
                        widget::button::icon(crate::ui::icons::get_handle("mail-attachment-symbolic", 16))
                            .on_press(Message::AttachFile)
                    )
                    .push(
                        // Text editor for message (multi-line)
                        text_editor(&app.input_content)
                            .id(app.input_id.clone())
                            .on_action(Message::InputActionPerformed)
                            .height(Length::Shrink)
                            .padding(12)
                            .key_binding(|key_press| {
                                match key_press.key.as_ref() {
                                    keyboard::Key::Named(keyboard::key::Named::Enter)
                                        if key_press.modifiers.shift() => {
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
                    )
                    .push(
                        // Send/Stop button (right side)
                        if app.is_streaming {
                            // Stop button when streaming
                            widget::button::icon(crate::ui::icons::get_handle("process-stop-symbolic", 16))
                                .class(widget::button::ButtonClass::Destructive)
                                .on_press(Message::StopMessage)
                        } else {
                            // Send button when not streaming
                            widget::button::icon(crate::ui::icons::get_handle("send-symbolic", 16))
                                .class(widget::button::ButtonClass::Suggested)
                                .on_press(Message::SendMessage)
                        }
                    )
                    .spacing(8)
                    .align_y(cosmic::iced::Alignment::Center)
            )
            .spacing(8)
    )
    .padding(16)
    .width(Length::Fill)
    .class(cosmic::style::Container::Card)
    .into()
}
