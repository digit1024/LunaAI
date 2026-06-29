//! Navigation message handlers
//!
//! Handles navigation-related messages: SelectConversation, DeleteConversation, NewConversation, NavigateTo

use cosmic::app;
use crate::ui::app::{LunaThinApp, Message, Page};

/// Handle navigation-related messages
pub fn handle_navigation_messages(
    app: &mut LunaThinApp,
    message: Message,
) -> Option<app::Task<Message>> {
    match message {
        Message::BackToChat => {
            app.current_page = Page::Chat;
            app.update_nav_model();
            None
        }
        Message::NavigateTo(page) => {
            app.current_page = page;
            // Load MCP servers when navigating to that page
            if page == crate::ui::app::Page::MCPServers && app.connection_status == crate::ui::app::ConnectionStatus::Connected {
                return Some(app::Task::perform(
                    async { Message::LoadMCPServers },
                    cosmic::Action::App,
                ));
            }
            if page == crate::ui::app::Page::Memories && app.connection_status == crate::ui::app::ConnectionStatus::Connected {
                return Some(app::Task::perform(
                    async { Message::LoadMemories },
                    cosmic::Action::App,
                ));
            }
            None
        }
        Message::SelectConversation(conv_id) => {
            tracing::info!("📂 SelectConversation: {}", conv_id);
            app.current_page = Page::Chat;
            if app.current_conversation_id.as_deref() == Some(conv_id.as_str())
                && !app.messages.is_empty()
            {
                app.update_nav_model();
                return None;
            }
            app.send_command(crate::server::dto::ClientCommand::LoadConversation {
                conversation_id: conv_id.clone(),
            });
            app.update_nav_model();
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
            app.current_conversation_internal = false;
            app.messages.clear();
            app.current_assistant_bubble_id = None;
            app.current_page = crate::ui::app::Page::Chat;
            app.update_nav_model();
            None
        }
        _ => None, // Not a navigation message
    }
}


