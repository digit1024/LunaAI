//! Dialog message handlers
//!
//! Handles dialog-related messages.

use cosmic::app;
use cosmic::widget::text_editor;

use crate::ui::app::{CosmicLlmApp, Message};
use crate::ui::dialogs::{DialogAction, DialogPage};

pub fn handle_dialog_messages(app: &mut CosmicLlmApp, message: &Message) -> Option<app::Task<Message>> {
    match message {
        Message::DialogAction(action) => {
            match action {
                DialogAction::Open(page) => {
                    match &page {
                        DialogPage::MessageText(text) => {
                            app.dialog_text_content = Some(text_editor::Content::with_text(text));
                        }
                    }
                    app.dialog = Some(page.clone());
                }
                DialogAction::Update(page) => {
                    match &page {
                        DialogPage::MessageText(text) => {
                            app.dialog_text_content = Some(text_editor::Content::with_text(text));
                        }
                    }
                    app.dialog = Some(page.clone());
                }
                DialogAction::Close => {
                    app.dialog = None;
                    app.dialog_text_content = None;
                }
                DialogAction::Complete => {
                    app.dialog = None;
                    app.dialog_text_content = None;
                }
                DialogAction::CopyText => {
                    if let Some(DialogPage::MessageText(text)) = &app.dialog {
                        let _ = cli_clipboard::set_contents(text.clone());
                    }
                }
                DialogAction::TextEditorAction(action) => {
                    if let Some(content) = &mut app.dialog_text_content {
                        content.perform(action.clone());
                    }
                }
            }
            Some(app::Task::none())
        }
        Message::ShowMessageDialog(content) => {
            let text = content.clone();
            app.dialog_text_content = Some(text_editor::Content::with_text(&text));
            app.dialog = Some(DialogPage::message_text(text));
            Some(app::Task::none())
        }
        _ => None,
    }
}

