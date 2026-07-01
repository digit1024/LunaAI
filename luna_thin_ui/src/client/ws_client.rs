use crate::client::config::ServerConfig;
use crate::server::dto::{ClientCommand, ServerEvent};
use futures::{SinkExt, StreamExt};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::broadcast;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::{
    client::IntoClientRequest,
    http,
    Message as WsMessage,
};

pub type EventReceiver = broadcast::Receiver<ServerEvent>;

/// Shared slot so the background socket task can drop the sender when the loop ends,
/// which closes the broadcast channel and lets UI subscribers detect disconnect.
type EventTxSlot = Arc<Mutex<Option<broadcast::Sender<ServerEvent>>>>;

const HEALTH_CHECK_INTERVAL: Duration = Duration::from_secs(30);
const IDLE_TIMEOUT: Duration = Duration::from_secs(60);
const STREAMING_IDLE_TIMEOUT: Duration = Duration::from_secs(180);

struct ConnectionHandle {
    command_tx: mpsc::UnboundedSender<ClientCommand>,
    is_streaming: Arc<AtomicBool>,
}

/// Cleared by the background task when the socket loop exits.
type HandleSlot = Arc<Mutex<Option<Arc<ConnectionHandle>>>>;

pub struct LunaWsClient {
    handle_slot: HandleSlot,
    event_tx: EventTxSlot,
    connection_task: Option<tokio::task::JoinHandle<()>>,
}

impl LunaWsClient {
    pub fn new() -> Self {
        Self {
            handle_slot: Arc::new(Mutex::new(None)),
            event_tx: Arc::new(Mutex::new(None)),
            connection_task: None,
        }
    }

    /// Get a new event receiver by subscribing to the broadcast channel
    /// Returns None if not connected
    pub fn subscribe(&self) -> Option<EventReceiver> {
        self.event_tx
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .map(|tx| tx.subscribe())
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
                tracing::info!(
                    "✅ WebSocket connected via ws:// (fallback, status: {})",
                    response.status()
                );
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
        headers.insert(
            "authorization",
            format!("Bearer {}", config.api_key).parse()?,
        );

        tracing::debug!(
            "Auth headers: x-api-key={}, authorization=Bearer ...",
            config.api_key
        );

        // Connect with timeout
        let connect_future = tokio_tungstenite::connect_async(request);
        let timeout_result =
            tokio::time::timeout(std::time::Duration::from_secs(10), connect_future).await;

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
        let (event_tx, _) = broadcast::channel::<ServerEvent>(10000);
        let (command_tx, mut command_rx) = mpsc::unbounded_channel::<ClientCommand>();

        let last_rx = Arc::new(Mutex::new(Instant::now()));
        let is_streaming = Arc::new(AtomicBool::new(false));

        let handle = Arc::new(ConnectionHandle {
            command_tx,
            is_streaming: is_streaming.clone(),
        });

        *self.handle_slot.lock().unwrap_or_else(|e| e.into_inner()) = Some(handle.clone());
        let handle_slot = self.handle_slot.clone();
        let event_tx_clone = event_tx.clone();
        let event_tx_slot = self.event_tx.clone();
        *event_tx_slot.lock().unwrap_or_else(|e| e.into_inner()) = Some(event_tx);

        let connection_task = tokio::spawn(async move {
            let mut health_interval = tokio::time::interval(HEALTH_CHECK_INTERVAL);
            // First tick completes immediately; skip it so we don't ping right after connect.
            health_interval.tick().await;

            let touch_rx = |last_rx: &Arc<Mutex<Instant>>| {
                if let Ok(mut guard) = last_rx.lock() {
                    *guard = Instant::now();
                }
            };

            let idle_timed_out = |last_rx: &Arc<Mutex<Instant>>, streaming: bool| -> bool {
                let timeout = if streaming {
                    STREAMING_IDLE_TIMEOUT
                } else {
                    IDLE_TIMEOUT
                };
                last_rx
                    .lock()
                    .map(|guard| guard.elapsed() > timeout)
                    .unwrap_or(true)
            };

            loop {
                tokio::select! {
                    Some(cmd) = command_rx.recv() => {
                        match serde_json::to_string(&cmd) {
                            Ok(json) => {
                                if let Err(e) = write.send(WsMessage::Text(json.into())).await {
                                    tracing::error!("Failed to send command: {}", e);
                                    break;
                                }
                                touch_rx(&last_rx);
                            }
                            Err(e) => {
                                tracing::error!("Failed to serialize command: {}", e);
                            }
                        }
                    }
                    Some(msg) = read.next() => {
                        match msg {
                            Ok(WsMessage::Text(text)) => {
                                touch_rx(&last_rx);
                                match serde_json::from_str::<ServerEvent>(&text) {
                                    Ok(event) => {
                                        let _ = event_tx_clone.send(event);
                                    }
                                    Err(e) => {
                                        tracing::error!(
                                            "Failed to deserialize event: {}. Raw: {}",
                                            e,
                                            text
                                        );
                                    }
                                }
                            }
                            Ok(WsMessage::Close(_)) => {
                                tracing::info!("WebSocket closed by server");
                                break;
                            }
                            Ok(WsMessage::Ping(data)) => {
                                touch_rx(&last_rx);
                                if let Err(e) = write.send(WsMessage::Pong(data)).await {
                                    tracing::error!("Failed to send pong: {}", e);
                                    break;
                                }
                            }
                            Ok(WsMessage::Pong(_)) => {
                                touch_rx(&last_rx);
                            }
                            Err(e) => {
                                tracing::error!("WebSocket error: {}", e);
                                break;
                            }
                            _ => {}
                        }
                    }
                    _ = health_interval.tick() => {
                        let streaming = is_streaming.load(Ordering::Relaxed);
                        if idle_timed_out(&last_rx, streaming) {
                            let secs = if streaming {
                                STREAMING_IDLE_TIMEOUT.as_secs()
                            } else {
                                IDLE_TIMEOUT.as_secs()
                            };
                            tracing::warn!(
                                "Connection timed out (no inbound traffic for {}s)",
                                secs
                            );
                            break;
                        }
                        if !streaming {
                            if let Ok(json) = serde_json::to_string(&ClientCommand::HealthCheck) {
                                if let Err(e) = write.send(WsMessage::Text(json.into())).await {
                                    tracing::error!("Failed to send health check: {}", e);
                                    break;
                                }
                                tracing::debug!("Health check sent (keepalive)");
                            }
                        }
                    }
                    else => break,
                }
            }

            *handle_slot.lock().unwrap_or_else(|e| e.into_inner()) = None;
            *event_tx_slot.lock().unwrap_or_else(|e| e.into_inner()) = None;
            tracing::info!("WebSocket connection loop ended");
        });

        self.connection_task = Some(connection_task);
    }

    pub async fn disconnect(&mut self) {
        if let Some(task) = self.connection_task.take() {
            task.abort();
            let _ = task.await;
        }
        *self.handle_slot.lock().unwrap_or_else(|e| e.into_inner()) = None;
        *self.event_tx.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }

    pub fn set_streaming(&self, streaming: bool) {
        if let Some(handle) = self.handle_slot.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            handle.is_streaming.store(streaming, Ordering::Relaxed);
            tracing::debug!("WebSocket streaming state: {}", streaming);
        }
    }

    pub fn send(&self, command: ClientCommand) {
        if let Some(handle) = self.handle_slot.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            if let Err(e) = handle.command_tx.send(command) {
                tracing::error!("Failed to send command: {}", e);
            }
        } else {
            tracing::warn!("Not connected, cannot send command");
        }
    }

    pub fn is_connected(&self) -> bool {
        self.handle_slot.lock().unwrap_or_else(|e| e.into_inner()).is_some()
    }
}

impl Default for LunaWsClient {
    fn default() -> Self {
        Self::new()
    }
}
