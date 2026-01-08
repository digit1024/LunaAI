use crate::client::config::ServerConfig;
use crate::server::dto::{ClientCommand, ServerEvent};
use futures::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::{
    client::IntoClientRequest,
    Message as WsMessage,
};

pub type EventReceiver = mpsc::UnboundedReceiver<ServerEvent>;
pub type EventSender = mpsc::UnboundedSender<ServerEvent>;
type CommandSender = mpsc::UnboundedSender<ClientCommand>;

pub struct LunaWsClient {
    command_tx: Option<CommandSender>,
    event_rx: Option<EventReceiver>,
    connection_task: Option<tokio::task::JoinHandle<()>>,
}

impl LunaWsClient {
    pub fn new() -> Self {
        Self {
            command_tx: None,
            event_rx: None,
            connection_task: None,
        }
    }

    pub fn take_event_receiver(&mut self) -> Option<EventReceiver> {
        self.event_rx.take()
    }

    pub async fn connect(
        &mut self,
        config: ServerConfig,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Disconnect if already connected
        self.disconnect().await;

        let uri = config.websocket_uri();
        tracing::info!("🔌 Connecting to {}", uri);

        // Build request with auth headers (same as mobile app)
        let mut request = uri.into_client_request()?;
        let headers = request.headers_mut();
        headers.insert("x-api-key", config.api_key.parse()?);
        headers.insert("authorization", format!("Bearer {}", config.api_key).parse()?);
        
        tracing::debug!("Auth headers: x-api-key={}, authorization=Bearer ...", config.api_key);

        // Connect with timeout
        let connect_future = tokio_tungstenite::connect_async(request);
        let timeout_result = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            connect_future,
        ).await;

        let (ws_stream, _response) = match timeout_result {
            Ok(Ok((stream, resp))) => {
                tracing::info!("✅ WebSocket connection established (status: {})", resp.status());
                (stream, resp)
            }
            Ok(Err(e)) => {
                tracing::error!("❌ WebSocket connection failed: {}", e);
                return Err(e.into());
            }
            Err(_) => {
                tracing::error!("❌ Connection timeout after 10 seconds");
                return Err("Connection timeout".into());
            }
        };

        let (mut write, mut read) = ws_stream.split();

        // Create channels
        let (event_tx, event_rx) = mpsc::unbounded_channel::<ServerEvent>();
        let (command_tx, mut command_rx) = mpsc::unbounded_channel::<ClientCommand>();

        self.event_rx = Some(event_rx);
        self.command_tx = Some(command_tx);

        // Spawn connection task
        let connection_task = tokio::spawn(async move {
            loop {
                tokio::select! {
                    // Handle outgoing commands
                    Some(cmd) = command_rx.recv() => {
                        match serde_json::to_string(&cmd) {
                            Ok(json) => {
                                if let Err(e) = write.send(WsMessage::Text(json.into())).await {
                                    tracing::error!("Failed to send command: {}", e);
                                    break;
                                }
                            }
                            Err(e) => {
                                tracing::error!("Failed to serialize command: {}", e);
                            }
                        }
                    }
                    // Handle incoming events
                    Some(msg) = read.next() => {
                        match msg {
                            Ok(WsMessage::Text(text)) => {
                                match serde_json::from_str::<ServerEvent>(&text) {
                                    Ok(event) => {
                                        if event_tx.send(event).is_err() {
                                            tracing::warn!("Event receiver dropped");
                                            break;
                                        }
                                    }
                                    Err(e) => {
                                        tracing::error!("Failed to deserialize event: {}. Raw: {}", e, text);
                                    }
                                }
                            }
                            Ok(WsMessage::Close(_)) => {
                                tracing::info!("WebSocket closed by server");
                                let _ = event_tx.send(ServerEvent::Error {
                                    message: "Connection closed by server".to_string(),
                                });
                                break;
                            }
                            Ok(WsMessage::Ping(data)) => {
                                if let Err(e) = write.send(WsMessage::Pong(data)).await {
                                    tracing::error!("Failed to send pong: {}", e);
                                }
                            }
                            Err(e) => {
                                tracing::error!("WebSocket error: {}", e);
                                let _ = event_tx.send(ServerEvent::Error {
                                    message: format!("WebSocket error: {}", e),
                                });
                                break;
                            }
                            _ => {}
                        }
                    }
                    else => break,
                }
            }
            tracing::info!("WebSocket connection loop ended");
        });

        self.connection_task = Some(connection_task);
        Ok(())
    }

    pub async fn disconnect(&mut self) {
        if let Some(task) = self.connection_task.take() {
            task.abort();
        }
        self.command_tx = None;
        self.event_rx = None;
    }

    pub fn send(&self, command: ClientCommand) {
        if let Some(ref tx) = self.command_tx {
            if let Err(e) = tx.send(command) {
                tracing::error!("Failed to send command: {}", e);
            }
        } else {
            tracing::warn!("Not connected, cannot send command");
        }
    }

    pub fn is_connected(&self) -> bool {
        self.command_tx.is_some()
    }
}

impl Default for LunaWsClient {
    fn default() -> Self {
        Self::new()
    }
}

