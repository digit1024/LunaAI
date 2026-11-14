use super::handlers::{ServerContext, ServerHandler};
use crate::server::conversation_subscriptions::ConnectionId;
use crate::server::dto::{ClientCommand, ServerEvent};
use anyhow::Result;
use axum::extract::ws::{Message, WebSocket};
use futures::stream::SplitSink;
use futures::{SinkExt, StreamExt};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

/// Reject oversized WebSocket text frames (DoS / memory exhaustion).
/// Enforced at the transport layer in `ws_handler` (tungstenite reassembles at
/// most this many bytes) and re-checked in the read loop as defense in depth.
pub(super) const MAX_WS_TEXT_BYTES: usize = 1024 * 1024; // 1 MiB
/// Per-frame transport cap; messages are reassembled up to MAX_WS_TEXT_BYTES.
pub(super) const MAX_WS_FRAME_BYTES: usize = 256 * 1024; // 256 KiB
/// Upper bound on the outbound write buffer (tungstenite default is unbounded).
/// Generous enough for large events (e.g. a full ConversationLoaded payload)
/// but bounded so a stalled client cannot grow the socket buffer without limit;
/// the connection is closed if it is exceeded.
pub(super) const MAX_WS_WRITE_BUFFER_BYTES: usize = 32 * 1024 * 1024; // 32 MiB
/// Capacity of the per-connection outbound event queue. Bounded so a client
/// that stops reading cannot make the server buffer events without limit;
/// when full, new events are dropped for that connection (deltas are
/// recoverable: `AssistantComplete` carries the full text and clients can
/// resync via `LoadConversation`).
pub(super) const WS_EVENT_CHANNEL_CAP: usize = 1024;

/// Runs the WebSocket command loop after upgrade. Auth is performed by the HTTP layer before upgrade.
pub async fn handle_ws_upgraded(socket: WebSocket, ctx: Arc<ServerContext>) -> Result<()> {
    let connection_start = Instant::now();
    let connection_id = ConnectionId::new();
    tracing::info!("WebSocket connection {}", connection_id.0);

    let (write, mut read) = socket.split();
    let sink: Arc<tokio::sync::Mutex<SplitSink<WebSocket, Message>>> =
        Arc::new(tokio::sync::Mutex::new(write));
    let sink_writer = sink.clone();
    let (out_tx, mut out_rx) = mpsc::channel::<ServerEvent>(WS_EVENT_CHANNEL_CAP);
    let mut handler = ServerHandler::new(ctx.clone(), connection_id, out_tx.clone())?;

    handler.handle_command(ClientCommand::HealthCheck).await;

    let writer = tokio::spawn(async move {
        while let Some(event) = out_rx.recv().await {
            let event_start = Instant::now();
            match serde_json::to_string(&event) {
                Ok(payload) => {
                    if matches!(event, ServerEvent::Error { .. }) {
                        log_error_response(&connection_id.0, &payload, event_start.elapsed());
                    }
                    let mut guard = sink_writer.lock().await;
                    if guard
                        .send(Message::Text(payload.into()))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                Err(err) => {
                    tracing::error!("Failed to serialize ServerEvent: {}", err);
                    log_serialization_error(&connection_id.0, &event, &err);
                    break;
                }
            }
        }
    });

    while let Some(msg) = read.next().await {
        let msg_start = Instant::now();
        match msg {
            Ok(Message::Text(text)) => {
                if text.len() > MAX_WS_TEXT_BYTES {
                    tracing::warn!(
                        connection_id = %connection_id.0,
                        size = text.len(),
                        limit = MAX_WS_TEXT_BYTES,
                        "Rejected oversized WebSocket text frame"
                    );
                    let _ = out_tx.try_send(ServerEvent::Error {
                        message: format!(
                            "WebSocket message exceeds {} byte limit",
                            MAX_WS_TEXT_BYTES
                        ),
                    });
                    continue;
                }
                log_incoming_request(&connection_id.0, &text, msg_start.elapsed());
                match serde_json::from_str::<ClientCommand>(&text) {
                    Ok(command) => {
                        handler.handle_command(command).await;
                    }
                    Err(err) => {
                        log_parse_error(&connection_id.0, &text, &err);
                        let _ = out_tx.try_send(ServerEvent::Error {
                            message: format!("Invalid command payload: {}", err),
                        });
                    }
                }
            }
            Ok(Message::Close(_)) => {
                tracing::info!("Connection {} closed by client", connection_id.0);
                break;
            }
            Ok(Message::Ping(payload)) => {
                let mut guard = sink.lock().await;
                let _ = guard.send(Message::Pong(payload)).await;
            }
            Err(e) => {
                tracing::warn!("WebSocket error for connection {}: {}", connection_id.0, e);
                break;
            }
            _ => {}
        }
    }

    drop(out_tx);
    let _ = writer.await;
    ctx.subscriptions.on_connection_closed(connection_id).await;
    tracing::info!(
        "Connection {} closed after {:?}",
        connection_id.0,
        connection_start.elapsed()
    );
    Ok(())
}

/// Truncate a payload for logging so oversized content doesn't flood the log
/// files and sensitive data isn't retained in full (SMELL-6). Contents are
/// user messages — treat them as potentially secret.
fn truncate_for_log(raw: &str, max_chars: usize) -> std::borrow::Cow<'_, str> {
    if raw.chars().count() <= max_chars {
        return std::borrow::Cow::Borrowed(raw);
    }
    let kept: String = raw.chars().take(max_chars).collect();
    std::borrow::Cow::Owned(format!(
        "{kept}… [truncated, {} chars total]",
        raw.chars().count()
    ))
}

fn log_incoming_request(connection_id: &uuid::Uuid, raw_request: &str, processing_time: Duration) {
    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(raw_request) {
        tracing::info!(
            connection_id = %connection_id,
            command_type = parsed.get("type").and_then(|v| v.as_str()).unwrap_or("unknown"),
            processing_time_ms = processing_time.as_millis(),
            request_size = raw_request.len(),
            "Incoming WebSocket request"
        );
        tracing::debug!(
            connection_id = %connection_id,
            raw_request = %truncate_for_log(raw_request, 2000),
            "Full incoming request details"
        );
    } else {
        tracing::warn!(
            connection_id = %connection_id,
            raw_request = %truncate_for_log(raw_request, 200),
            request_size = raw_request.len(),
            "Received malformed JSON request"
        );
    }
}

fn log_error_response(connection_id: &uuid::Uuid, raw_response: &str, processing_time: Duration) {
    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(raw_response) {
        if let Some(error_msg) = parsed.get("message").and_then(|v| v.as_str()) {
            tracing::error!(
                connection_id = %connection_id,
                error_message = %error_msg,
                processing_time_ms = processing_time.as_millis(),
                response_size = raw_response.len(),
                "Error response sent to client"
            );
        }
        tracing::debug!(
            connection_id = %connection_id,
            raw_response = %truncate_for_log(raw_response, 2000),
            "Full error response details"
        );
    }
}

fn log_parse_error(connection_id: &uuid::Uuid, raw_request: &str, error: &serde_json::Error) {
    // WARN (not ERROR) with a truncated payload: malformed input is a client
    // mistake, not a server fault, and raw request bodies may carry secrets.
    tracing::warn!(
        connection_id = %connection_id,
        raw_request = %truncate_for_log(raw_request, 200),
        error_message = %error,
        error_line = error.line(),
        error_column = error.column(),
        "Failed to parse incoming JSON request"
    );
}

fn log_serialization_error(
    connection_id: &uuid::Uuid,
    event: &ServerEvent,
    error: &serde_json::Error,
) {
    tracing::error!(
        connection_id = %connection_id,
        event_type = ?std::mem::discriminant(event),
        error_message = %error,
        "Failed to serialize server event"
    );
    tracing::debug!(
        connection_id = %connection_id,
        event_debug = ?event,
        "Event that failed to serialize"
    );
}
