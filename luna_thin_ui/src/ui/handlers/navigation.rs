//! Navigation message handlers
//!
//! Handles navigation-related messages: SelectConversation, DeleteConversation, NewConversation, NavigateTo

use cosmic::app;
use crate::ui::app::{LunaThinApp, Message};

/// Handle navigation-related messages
pub fn handle_navigation_messages(
    app: &mut LunaThinApp,
    message: Message,
) -> Option<app::Task<Message>> {
    match message {
        Message::NavigateTo(page) => {
            app.current_page = page;
            None
        }
        Message::SelectConversation(conv_id) => {
            tracing::info!("📂 SelectConversation: {}", conv_id);
            app.send_command(crate::server::dto::ClientCommand::LoadConversation { 
                conversation_id: conv_id.clone() 
            });
            None
        }
        Message::DeleteConversation(conv_id) => {
            app.send_command(crate::server::dto::ClientCommand::DeleteConversation { 
                conversation_id: conv_id 
            });
            None
        }
        Message::NewConversation => {
            app.current_conversation_id = None;
            app.messages.clear();
            app.current_assistant_bubble_id = None;
            app.current_page = crate::ui::app::Page::Chat;
            app.update_nav_model();
            None
        }
        _ => None, // Not a navigation message
    }
}


