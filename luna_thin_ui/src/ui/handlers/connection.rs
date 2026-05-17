//! Connection message handlers
//!
//! Handles WebSocket connection-related messages: Connect, Disconnect, ServerConnected, etc.

use cosmic::app;
use std::sync::atomic::Ordering;
use std::time::Duration;

use crate::client::FileClient;
use crate::ui::app::{ConnectionStatus, LunaThinApp, Message, Page};

const MAX_RECONNECT_ATTEMPTS: u32 = 3;
const RECONNECT_RETRY_DELAY: Duration = Duration::from_secs(2);

/// Handle connection-related messages
pub fn handle_connection_messages(
    app: &mut LunaThinApp,
    message: Message,
) -> Option<app::Task<Message>> {
    match message {
        Message::Connect => {
            app.user_disconnect_flag.store(false, Ordering::Relaxed);
            app.reconnect_in_progress = false;

            // Update config from settings
            app.server_config.host = app.settings_host.clone();
            app.server_config.port = app
                .settings_port
                .parse()
                .unwrap_or_else(|e| {
                    tracing::warn!(
                        "Failed to parse port '{}': {}, using default 8080",
                        app.settings_port,
                        e
                    );
                    8080
                });
            app.server_config.api_key = app.settings_api_key.clone();
            let _ = app.server_config.save();

            app.file_client = Some(FileClient::new(app.server_config.clone()));
            app.connection_status = ConnectionStatus::Connecting;
            app.inline_error = None;

            let ws_client = app.ws_client.clone();
            let config = app.server_config.clone();
            Some(app::Task::perform(
                async move {
                    let mut client = ws_client.write().await;
                    match client.connect(config).await {
                        Ok(_) => Message::ServerConnected,
                        Err(e) => Message::ServerError(e.to_string()),
                    }
                },
                |msg| cosmic::Action::App(msg),
            ))
        }
        Message::Disconnect => {
            app.user_disconnect_flag.store(true, Ordering::Relaxed);
            app.reconnect_in_progress = false;
            app.inline_info = None;

            let ws_client = app.ws_client.clone();
            tokio::spawn(async move {
                let mut client = ws_client.write().await;
                client.disconnect().await;
            });
            app.connection_status = ConnectionStatus::Disconnected;
            None
        }
        Message::ServerConnected | Message::ConnectionEstablished => {
            app.user_disconnect_flag.store(false, Ordering::Relaxed);
            app.reconnect_in_progress = false;
            app.connection_status = ConnectionStatus::Connected;
            app.inline_error = None;
            app.inline_info = None;

            if app.current_page == Page::Settings {
                app.current_page = Page::Chat;
            }

            app.on_connect();
            None
        }
        Message::ServerDisconnected => {
            app.reconnect_in_progress = false;

            if app.user_disconnect_flag.load(Ordering::Relaxed) {
                app.connection_status = ConnectionStatus::Disconnected;
                return None;
            }

            if app.reconnect_in_progress {
                return None;
            }

            app.connection_status = ConnectionStatus::Connecting;
            app.inline_info = Some("Reconnecting…".to_string());
            app.inline_error = None;

            Some(app::Task::done(cosmic::Action::App(Message::AutoReconnect)))
        }
        Message::AutoReconnect => {
            if app.user_disconnect_flag.load(Ordering::Relaxed) || app.reconnect_in_progress {
                return None;
            }

            app.reconnect_in_progress = true;
            app.connection_status = ConnectionStatus::Connecting;

            let ws_client = app.ws_client.clone();
            let config = app.server_config.clone();
            let user_disconnect_flag = app.user_disconnect_flag.clone();

            Some(app::Task::perform(
                async move {
                    for attempt in 1..=MAX_RECONNECT_ATTEMPTS {
                        if user_disconnect_flag.load(Ordering::Relaxed) {
                            tracing::info!("Reconnect cancelled (user disconnect)");
                            return Message::ServerDisconnected;
                        }

                        tracing::info!(
                            "Silent reconnect attempt {}/{}",
                            attempt,
                            MAX_RECONNECT_ATTEMPTS
                        );

                        let result = {
                            let mut client = ws_client.write().await;
                            client.connect(config.clone()).await
                        };

                        match result {
                            Ok(_) => return Message::ServerConnected,
                            Err(e) => {
                                tracing::warn!("Reconnect attempt {} failed: {}", attempt, e);
                                if attempt < MAX_RECONNECT_ATTEMPTS {
                                    tokio::time::sleep(RECONNECT_RETRY_DELAY).await;
                                }
                            }
                        }
                    }

                    Message::ConnectionFailed(
                        "Could not reconnect to server after 3 attempts.".to_string(),
                    )
                },
                |msg| cosmic::Action::App(msg),
            ))
        }
        Message::ServerError(error) => {
            app.reconnect_in_progress = false;
            app.connection_status = ConnectionStatus::Error;
            app.inline_info = None;
            app.inline_error = Some(error);
            None
        }
        Message::ConnectionFailed(error) => {
            app.reconnect_in_progress = false;
            app.connection_status = ConnectionStatus::Error;
            app.inline_info = None;
            app.inline_error = Some(error);
            None
        }
        _ => None,
    }
}
