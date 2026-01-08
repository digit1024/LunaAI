//! Server event message handlers
//!
//! Handles server events from WebSocket: All ServerEvent variants

use cosmic::app;
use crate::ui::app::{LunaThinApp, Message};

/// Handle server event messages
pub fn handle_server_event_messages(
    app: &mut LunaThinApp,
    message: Message,
) -> Option<app::Task<Message>> {
    if let Message::ServerEvent(event) = message {
        tracing::debug!("📥 ServerEvent received: {:?}", event);
        return Some(app.handle_server_event(event));
    }
    None
}

