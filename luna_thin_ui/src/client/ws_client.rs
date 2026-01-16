use crate::client::config::ServerConfig;
use crate::server::dto::{ClientCommand, ServerEvent};
use futures::{SinkExt, StreamExt};
use tokio::sync::broadcast;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::{
    client::IntoClientRequest,
    http,
    Message as WsMessage,
};

pub type EventReceiver = broadcast::Receiver<ServerEvent>;
type CommandSender = mpsc::UnboundedSender<ClientCommand>;

pub struct LunaWsClient {
    command_tx: Option<CommandSender>,
    event_tx: Option<broadcast::Sender<ServerEvent>>,
    connection_task: Option<tokio::task::JoinHandle<()>>,
}

impl LunaWsClient {
    pub fn new() -> Self {
        Self {
            command_tx: None,
            event_tx: None,
            connection_task: None,
        }
    }

    /// Get a new event receiver by subscribing to the broadcast channel
    /// Returns None if not connected
    pub fn subscribe(&self) -> Option<EventReceiver> {
        self.event_tx.as_ref().map(|tx| tx.subscribe())
    }

    pub async fn connect(
        &mut self,
        config: ServerConfig,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Disconnect if already connected
        self.disconnect().await;

        // Try secure connection (wss://) first, then fallback to insecure (ws://)
        let secure_uri = config.websocket_uri_secure();
        let insecure_uri = config.websocket_uri_insecure();
        
        // Try wss:// first
        match Self::try_connect(&secure_uri, &config).await {
            Ok((ws_stream, response)) => {
                tracing::info!("✅ WebSocket connected via wss:// (status: {})", response.status());
                self.setup_connection(ws_stream).await;
                return Ok(());
            }
            Err(e) => {
                tracing::warn!("⚠️ Secure connection (wss://) failed: {}", e);
            }
        }

        // Fallback to ws://
        tracing::info!("🔌 Falling back to insecure connection (ws://)");
        match Self::try_connect(&insecure_uri, &config).await {
            Ok((ws_stream, response)) => {
                tracing::info!("✅ WebSocket connected via ws:// (fallback, status: {})", response.status());
                self.setup_connection(ws_stream).await;
                Ok(())
            }
            Err(e) => {
                tracing::error!("❌ Both secure and insecure connections failed");
                Err(e)
            }
        }
    }

    async fn try_connect(
        uri: &str,
        config: &ServerConfig,
    ) -> Result<
        (
            tokio_tungstenite::WebSocketStream<
                tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
            >,
            http::Response<Option<Vec<u8>>>,
        ),
        Box<dyn std::error::Error + Send + Sync>,
    > {
        tracing::info!("🔌 Attempting connection to {}", uri);

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

        match timeout_result {
            Ok(Ok((stream, resp))) => Ok((stream, resp)),
            Ok(Err(e)) => {
                tracing::error!("❌ WebSocket connection failed: {}", e);
                Err(e.into())
            }
            Err(_) => {
                tracing::error!("❌ Connection timeout after 10 seconds");
                Err("Connection timeout".into())
            }
        }
    }

    async fn setup_connection(
        &mut self,
        ws_stream: tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    ) {

        let (mut write, mut read) = ws_stream.split();

        // Create broadcast channel for events (allows multiple receivers)
        // Increased buffer size to prevent dropping events during rapid streaming
        let (event_tx, _) = broadcast::channel::<ServerEvent>(10000);
        let (command_tx, mut command_rx) = mpsc::unbounded_channel::<ClientCommand>();

        let event_tx_clone = event_tx.clone();
        self.event_tx = Some(event_tx);
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
                                        // Broadcast to all subscribers (ignore errors if no subscribers)
                                        let _ = event_tx_clone.send(event);
                                    }
                                    Err(e) => {
                                        tracing::error!("Failed to deserialize event: {}. Raw: {}", e, text);
                                    }
                                }
                            }
                            Ok(WsMessage::Close(_)) => {
                                tracing::info!("WebSocket closed by server");
                                let _ = event_tx_clone.send(ServerEvent::Error {
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
                                let _ = event_tx_clone.send(ServerEvent::Error {
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
    }

    pub async fn disconnect(&mut self) {
        // Abort connection task first
        if let Some(task) = self.connection_task.take() {
            task.abort();
            // Wait for task to complete cleanup
            let _ = task.await;
        }
        // Drop command sender to close channel
        self.command_tx = None;
        // Drop event sender to close broadcast channel (all receivers will get RecvError)
        self.event_tx = None;
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

