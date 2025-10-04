use cosmic::{
    iced::Length,
    widget::{self, text_editor},
};

use crate::ui::app::Message;

/// Dialog actions for managing popup dialogs
#[derive(Debug, Clone)]
pub enum DialogAction {
    Close,
    CopyText,
    TextEditorAction(text_editor::Action),
}

/// Different types of dialogs that can be shown
#[derive(Debug)]
pub enum DialogPage {
    MessageText(text_editor::Content),
}

impl DialogPage {
    /// Create a dialog for displaying and copying message text
    pub fn view(&self, text_editor_id: &widget::Id) -> widget::Dialog<Message> {
        let spacing = cosmic::theme::active().cosmic().spacing;

        match self {
            DialogPage::MessageText(content) => {
                widget::dialog()
                    .title("Message Text")
                    .primary_action(
                        widget::button::suggested("Copy")
                            .on_press(Message::DialogAction(DialogAction::CopyText))
                    )
                    .secondary_action(
                        widget::button::standard("Close")
                            .on_press(Message::DialogAction(DialogAction::Close))
                    )
                    .control(
                        widget::column::with_children(vec![
                            // Instructions
                            widget::text::body("Select and copy the message text below:")
                                .into(),
                            
                            // Selectable text display using text_editor
                            widget::container(
                                widget::text_editor(content)
                                    .id(text_editor_id.clone())
                                    .height(Length::Fixed(300.0))
                                    .on_action(|action| Message::DialogAction(DialogAction::TextEditorAction(action)))
                            )
                            .width(Length::Fill)
                            .padding(8)
                            .class(cosmic::style::Container::Card)
                            .into(),
                        ])
                        .spacing(spacing.space_s)
                    )
            }
        }
    }
}
