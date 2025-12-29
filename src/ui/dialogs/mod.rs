use cosmic::{
    iced::Length,
    widget::{self, text_editor},
};

use crate::ui::app::Message;

/// Dialog actions for managing popup dialogs
#[derive(Debug, Clone)]
pub enum DialogAction {
    #[allow(dead_code)] // Used in app.rs but compiler doesn't detect
    Open(DialogPage),
    #[allow(dead_code)] // Used in app.rs but compiler doesn't detect
    Update(DialogPage),
    Close,
    #[allow(dead_code)] // Used in app.rs but compiler doesn't detect
    Complete,
    CopyText,
    TextEditorAction(text_editor::Action),
}

/// Different types of dialogs that can be shown
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DialogPage {
    MessageText(String),
}

impl DialogPage {
    /// Create a dialog for displaying and copying message text
    pub fn view<'a>(&'a self, content: &'a text_editor::Content) -> widget::Dialog<'a, Message> {
        let spacing = cosmic::theme::active().cosmic().spacing;

        match self {
            DialogPage::MessageText(_) => {
                widget::dialog()
                    .title("Message Text")
                    .primary_action(
                        widget::button::suggested("Copy")
                            .on_press(Message::DialogAction(DialogAction::CopyText)),
                    )
                    .secondary_action(
                        widget::button::standard("Close")
                            .on_press(Message::DialogAction(DialogAction::Close)),
                    )
                    .control(
                        widget::column::with_children(vec![
                            // Instructions
                            widget::text("Click Copy to copy the message text to clipboard")
                                .size(14)
                                .into(),
                            widget::Space::with_height(Length::Fixed(16.0)).into(),
                            // Text editor with the content
                            text_editor(content)
                                .height(Length::Fixed(300.0))
                                .on_action(|action| {
                                    Message::DialogAction(DialogAction::TextEditorAction(action))
                                })
                                .into(),
                        ])
                        .spacing(spacing.space_s),
                    )
            }
        }
    }
    
    /// Create a MessageText dialog from a string
    pub fn message_text(text: String) -> Self {
        Self::MessageText(text)
    }
    
    /// Get the text content for MessageText dialog
    #[allow(dead_code)] // Public API method
    pub fn text(&self) -> Option<&str> {
        match self {
            DialogPage::MessageText(text) => Some(text),
        }
    }
}
