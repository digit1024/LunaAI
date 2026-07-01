//! Connection message handlers
//!
//! Handles WebSocket connection-related messages: Connect, Disconnect, ServerConnected, etc.

use cosmic::app;
use std::sync::atomic::Ordering;
use std::time::Duration;

use crate::client::FileClient;
use crate::ui::app::{ConnectionStatus, LunaThinApp, Message, Page};

const RECONNECT_INITIAL_DELAY: Duration = Duration::from_secs(2);
const RECONNECT_MAX_DELAY: Duration = Duration::from_secs(30);

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

            app.file_client = None;
            app.rest_base = None;
            app.connection_status = ConnectionStatus::Connecting;
            app.inline_error = None;

            let ws_client = app.ws_client.clone();
            let config = app.server_config.clone();
            Some(app::Task::perform(
                async move {
                    let mut client = ws_client.write().await;
                    match client.connect(config.clone()).await {
                        Ok(result) => {
                            let rest_base =
                                config.rest_base_for_ws_secure(result.ws_secure);
                            Message::ServerConnected {
                                insecure_warning: result.insecure_warning,
                                rest_base,
                            }
                        }
                        Err(e) => Message::ServerError(e.to_string()),
                    }
                },
                cosmic::Action::App,
            ))
        }
        Message::Disconnect => {
            app.user_disconnect_flag.store(true, Ordering::Relaxed);
            app.reconnect_in_progress = false;
            app.inline_info = None;
            app.connection_warning = None;
            app.rest_base = None;
            app.file_client = None;

            let ws_client = app.ws_client.clone();
            tokio::spawn(async move {
                let mut client = ws_client.write().await;
                client.disconnect().await;
            });
            app.connection_status = ConnectionStatus::Disconnected;
            None
        }
        Message::ServerConnected {
            insecure_warning,
            rest_base,
        } => {
            app.user_disconnect_flag.store(false, Ordering::Relaxed);
            app.reconnect_in_progress = false;
            app.connection_status = ConnectionStatus::Connected;
            app.inline_error = None;
            app.inline_info = None;
            app.connection_warning = insecure_warning;
            app.rest_base = Some(rest_base.clone());
            app.file_client = Some(FileClient::with_rest_base(
                app.server_config.clone(),
                rest_base,
            ));

            if app.current_page == Page::Settings {
                app.current_page = Page::Chat;
            }

            app.on_connect();
            None
        }
        Message::ConnectionEstablished => {
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
            if app.user_disconnect_flag.load(Ordering::Relaxed) {
                app.reconnect_in_progress = false;
                app.connection_status = ConnectionStatus::Disconnected;
                app.inline_info = None;
                return None;
            }

            if app.reconnect_in_progress {
                return None;
            }

            app.connection_status = ConnectionStatus::Connecting;
            app.inline_info = Some("Reconnecting…".to_string());
            app.inline_error = None;
            app.connection_warning = None;

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
                    let mut delay = RECONNECT_INITIAL_DELAY;
                    let mut attempt = 0u32;

                    loop {
                        if user_disconnect_flag.load(Ordering::Relaxed) {
                            tracing::info!("Reconnect cancelled (user disconnect)");
                            return Message::ServerDisconnected;
                        }

                        attempt += 1;
                        tracing::info!("Reconnect attempt {}", attempt);

                        let result = {
                            let mut client = ws_client.write().await;
                            client.connect(config.clone()).await
                        };

                        match result {
                            Ok(connect_result) => {
                                let rest_base = config
                                    .rest_base_for_ws_secure(connect_result.ws_secure);
                                return Message::ServerConnected {
                                    insecure_warning: connect_result.insecure_warning,
                                    rest_base,
                                };
                            }
                            Err(e) => {
                                tracing::warn!("Reconnect attempt {} failed: {}", attempt, e);
                                tokio::time::sleep(delay).await;
                                delay = delay.saturating_mul(2).min(RECONNECT_MAX_DELAY);
                            }
                        }
                    }
                },
                cosmic::Action::App,
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
