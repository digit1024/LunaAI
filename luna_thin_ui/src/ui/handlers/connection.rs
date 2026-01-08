//! Connection message handlers
//!
//! Handles WebSocket connection-related messages: Connect, Disconnect, ServerConnected, etc.

use cosmic::app;
use crate::client::FileClient;
use crate::ui::app::{LunaThinApp, Message, ConnectionStatus};

/// Handle connection-related messages
pub fn handle_connection_messages(
    app: &mut LunaThinApp,
    message: Message,
) -> Option<app::Task<Message>> {
    match message {
        Message::Connect => {
            // Update config from settings
            app.server_config.host = app.settings_host.clone();
            app.server_config.port = app.settings_port.parse()
                .unwrap_or_else(|e| {
                    tracing::warn!("Failed to parse port '{}': {}, using default 8080", app.settings_port, e);
                    8080
                });
            app.server_config.api_key = app.settings_api_key.clone();
            let _ = app.server_config.save();

            app.file_client = Some(FileClient::new(app.server_config.clone()));
            app.connection_status = ConnectionStatus::Connecting;

            let ws_client = app.ws_client.clone();
            let config = app.server_config.clone();
            Some(app::Task::perform(
                async move {
                    let mut client = ws_client.write().await;
                    match client.connect(config).await {
                        Ok(_) => {
                            // Connection successful - subscription will get receiver via subscribe()
                            Message::ServerConnected
                        },
                        Err(e) => Message::ServerError(e.to_string()),
                    }
                },
                |msg| cosmic::Action::App(msg),
            ))
        }
        Message::Disconnect => {
            let ws_client = app.ws_client.clone();
            tokio::spawn(async move {
                let mut client = ws_client.write().await;
                client.disconnect().await;
            });
            app.connection_status = ConnectionStatus::Disconnected;
            None
        }
        Message::ServerConnected | Message::ConnectionEstablished => {
            // Remove duplicate - both do the same thing
            app.connection_status = ConnectionStatus::Connected;
            app.on_connect();
            None
        }
        Message::ServerDisconnected => {
            app.connection_status = ConnectionStatus::Disconnected;
            None
        }
        Message::ServerError(error) => {
            app.connection_status = ConnectionStatus::Error;
            app.inline_error = Some(error);
            None
        }
        Message::ConnectionFailed(error) => {
            app.connection_status = ConnectionStatus::Error;
            app.inline_error = Some(error);
            None
        }
        _ => None, // Not a connection message
    }
}

