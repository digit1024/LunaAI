//! Settings message handlers
//!
//! Handles settings-related messages: HostChanged, PortChanged, ApiKeyChanged, ChangeProfile, OpenSettings

use cosmic::app;
use crate::ui::app::{LunaThinApp, Message};

/// Handle settings-related messages
pub fn handle_settings_messages(
    app: &mut LunaThinApp,
    message: Message,
) -> Option<app::Task<Message>> {
    match message {
        Message::HostChanged(host) => {
            app.settings_host = host;
            None
        }
        Message::PortChanged(port) => {
            app.settings_port = port;
            None
        }
        Message::ApiKeyChanged(api_key) => {
            app.settings_api_key = api_key;
            None
        }
        Message::ChangeProfile(profile) => {
            app.send_command(crate::server::dto::ClientCommand::ChangeProfile { profile });
            None
        }
        Message::OpenSettings => {
            app.current_page = crate::ui::app::Page::Settings;
            None
        }
        _ => None, // Not a settings message
    }
}


