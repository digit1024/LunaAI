use super::handlers::{ServerContext, ServerHandler};
use crate::server::dto::{ClientCommand, ServerEvent};
use anyhow::{Context, Result};
use futures::{SinkExt, StreamExt};
use socket2::SockRef;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::TcpListener;
use tokio_tungstenite::{
    accept_hdr_async,
    tungstenite::{
        handshake::server::{Request, Response},
        http::StatusCode,
        Message,
    },
};
use serde_json;

pub async fn serve(ctx: Arc<ServerContext>) -> Result<()> {
    let bind_addr = format!("{}:{}", ctx.server_cfg.host, ctx.server_cfg.port);
    let listener = TcpListener::bind(&bind_addr)
        .await
        .with_context(|| format!("failed to bind to {}", bind_addr))?;
    tracing::info!(address = %bind_addr, "Luna server listening");

    loop {
        let (stream, addr) = listener.accept().await?;
        let ctx_clone = ctx.clone();
        tracing::info!("🔌 Incoming connection from {}", addr);
        tokio::spawn(async move {
            if let Err(err) = handle_connection(stream, ctx_clone).await {
                tracing::warn!("Connection closed: {}", err);
            }
        });
    }
}

// Logging functions for comprehensive error tracking
fn log_incoming_request(connection_id: &uuid::Uuid, raw_request: &str, processing_time: Duration) {
    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(raw_request) {
        tracing::info!(
            connection_id = %connection_id,
            command_type = parsed.get("type").and_then(|v| v.as_str()).unwrap_or("unknown"),
            processing_time_ms = processing_time.as_millis(),
            request_size = raw_request.len(),
            "Incoming WebSocket request"
        );
        
        // Log full request at debug level
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
        
        // Log full error response at debug level
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

fn log_command_error(connection_id: &uuid::Uuid, raw_request: &str, error: anyhow::Error) {
    tracing::error!(
        connection_id = %connection_id,
        raw_request = %raw_request,
        error_message = %error,
        error_chain = %error.chain().map(|e| e.to_string()).collect::<Vec<_>>().join(" -> "),
        "Command processing failed"
    );
}

fn log_websocket_error(connection_id: &uuid::Uuid, error: tokio_tungstenite::tungstenite::Error) {
    tracing::error!(
        connection_id = %connection_id,
        error_message = %error,
        "WebSocket protocol error"
    );
}

fn log_serialization_error(connection_id: &uuid::Uuid, event: &ServerEvent, error: &serde_json::Error) {
    tracing::error!(
        connection_id = %connection_id,
        event_type = ?std::mem::discriminant(event),
        error_message = %error,
        "Failed to serialize server event"
    );
    
    // Try to log a debug representation of the event
    tracing::debug!(
        connection_id = %connection_id,
        event_debug = ?event,
        "Event that failed to serialize"
    );
}

async fn handle_connection(stream: tokio::net::TcpStream, ctx: Arc<ServerContext>) -> Result<()> {
    let connection_start = Instant::now();
    let connection_id = uuid::Uuid::new_v4();
    
    // Configure TCP keepalive to prevent network intermediaries from closing idle connections.
    // This sends keepalive probes at the OS level, complementing application-level health checks.
    // socket2 is needed because tokio::net::TcpStream doesn't expose these socket options.
    let socket_ref = SockRef::from(&stream);
    let keepalive = socket2::TcpKeepalive::new()
        .with_time(Duration::from_secs(60))
        .with_interval(Duration::from_secs(60));
    if let Err(e) = socket_ref.set_tcp_keepalive(&keepalive) {
        tracing::warn!("Failed to set TCP keepalive: {}", e);
    }
    
    let expected_key = ctx.server_cfg.api_key.clone();
    let ws_stream = accept_hdr_async(stream, |req: &Request, mut response: Response| {
        if authorize(req, &expected_key) {
            Ok(response)
        } else {
            *response.status_mut() = StatusCode::UNAUTHORIZED;
            Err(response.map(|_| Some("Unauthorized".to_string())))
        }
    })
    .await?;

    let (write, mut read) = ws_stream.split();
    let sink = Arc::new(tokio::sync::Mutex::new(write));
    let sink_writer = sink.clone();
    let (out_tx, mut out_rx) = tokio::sync::mpsc::unbounded_channel::<ServerEvent>();
    let mut handler = ServerHandler::new(ctx, out_tx.clone())?;

    // Send initial health event so the client can update UI immediately.
    handler.handle_command(ClientCommand::HealthCheck).await;

    let writer = tokio::spawn(async move {
        while let Some(event) = out_rx.recv().await {
            let event_start = Instant::now();
            match serde_json::to_string(&event) {
                Ok(payload) => {
                    // Log outgoing response for error events
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
                // Log incoming request
                log_incoming_request(&connection_id, &text, msg_start.elapsed());
                
match serde_json::from_str::<ClientCommand>(&text) {
                Ok(command) => {
                    handler.handle_command(command).await;
                },
                    Err(err) => {
                        log_parse_error(&connection_id, &text, &err);
                        let _ = out_tx.send(ServerEvent::Error {
                            message: format!("Invalid command payload: {}", err),
                        });
                    }
                }
            },
            Ok(Message::Close(_)) => {
                tracing::info!("Connection {} closed by client", connection_id);
                break;
            },
            Ok(Message::Ping(payload)) => {
                let mut guard = sink.lock().await;
                let _ = guard.send(Message::Pong(payload)).await;
            }
            Err(err) => {
                tracing::warn!("Websocket error for connection {}: {}", connection_id, err);
                log_websocket_error(&connection_id, err);
                break;
            }
            _ => {}
        }
    }

    drop(out_tx);
    writer.await.ok();
    
    tracing::info!("Connection {} closed after {:?}", connection_id, connection_start.elapsed());
    Ok(())
}

fn authorize(req: &Request, expected_key: &str) -> bool {
    extract_api_key(req)
        .map(|provided| provided == expected_key)
        .unwrap_or(false)
}

fn extract_api_key(req: &Request) -> Option<String> {
    if let Some(value) = req.headers().get("x-api-key") {
        return value.to_str().ok().map(|s| s.to_string());
    }
    if let Some(value) = req.headers().get("authorization") {
        if let Ok(header) = value.to_str() {
            if let Some(token) = header.strip_prefix("Bearer ") {
                return Some(token.trim().to_string());
            }
        }
    }
    None
}
