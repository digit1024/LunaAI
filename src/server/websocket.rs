use super::handlers::{ServerContext, ServerHandler};
use crate::server::dto::{ClientCommand, ServerEvent};
use anyhow::Result;
use axum::extract::ws::{Message, WebSocket};
use futures::stream::SplitSink;
use futures::{SinkExt, StreamExt};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use serde_json;

/// Runs the WebSocket command loop after upgrade. Auth is performed by the HTTP layer before upgrade.
pub async fn handle_ws_upgraded(socket: WebSocket, ctx: Arc<ServerContext>) -> Result<()> {
    let connection_start = Instant::now();
    let connection_id = uuid::Uuid::new_v4();
    tracing::info!("WebSocket connection {}", connection_id);

    let (write, mut read) = socket.split();
    let sink: Arc<tokio::sync::Mutex<SplitSink<WebSocket, Message>>> =
        Arc::new(tokio::sync::Mutex::new(write));
    let sink_writer = sink.clone();
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<ServerEvent>();
    let mut handler = ServerHandler::new(ctx, out_tx.clone())?;

    handler.handle_command(ClientCommand::HealthCheck).await;

    let writer = tokio::spawn(async move {
        while let Some(event) = out_rx.recv().await {
            let event_start = Instant::now();
            match serde_json::to_string(&event) {
                Ok(payload) => {
                    if matches!(event, ServerEvent::Error { .. }) {
                        log_error_response(&connection_id, &payload, event_start.elapsed());
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
                    log_serialization_error(&connection_id, &event, &err);
                    break;
                }
            }
        }
    });

    while let Some(msg) = read.next().await {
        let msg_start = Instant::now();
        match msg {
            Ok(Message::Text(text)) => {
                log_incoming_request(&connection_id, &text, msg_start.elapsed());
                match serde_json::from_str::<ClientCommand>(&text) {
                    Ok(command) => {
                        handler.handle_command(command).await;
                    }
                    Err(err) => {
                        log_parse_error(&connection_id, &text, &err);
                        let _ = out_tx.send(ServerEvent::Error {
                            message: format!("Invalid command payload: {}", err),
                        });
                    }
                }
            }
            Ok(Message::Close(_)) => {
                tracing::info!("Connection {} closed by client", connection_id);
                break;
            }
            Ok(Message::Ping(payload)) => {
                let mut guard = sink.lock().await;
                let _ = guard.send(Message::Pong(payload)).await;
            }
            Err(e) => {
                tracing::warn!("WebSocket error for connection {}: {}", connection_id, e);
                break;
            }
            _ => {}
        }
    }

    drop(out_tx);
    let _ = writer.await;
    tracing::info!("Connection {} closed after {:?}", connection_id, connection_start.elapsed());
    Ok(())
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
            raw_request = %raw_request,
            "Full incoming request details"
        );
    } else {
        tracing::warn!(
            connection_id = %connection_id,
            raw_request = %raw_request,
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
            raw_response = %raw_response,
            "Full error response details"
        );
    }
}

fn log_parse_error(connection_id: &uuid::Uuid, raw_request: &str, error: &serde_json::Error) {
    tracing::error!(
        connection_id = %connection_id,
        raw_request = %raw_request,
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
