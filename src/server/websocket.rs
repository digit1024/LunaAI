use super::handlers::{ServerContext, ServerHandler};
use crate::server::dto::{ClientCommand, ServerEvent};
use anyhow::{Context, Result};
use futures::{SinkExt, StreamExt};
use socket2::SockRef;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio_tungstenite::{
    accept_hdr_async,
    tungstenite::{
        handshake::server::{Request, Response},
        http::StatusCode,
        Message,
    },
};

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

async fn handle_connection(stream: tokio::net::TcpStream, ctx: Arc<ServerContext>) -> Result<()> {
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
            match serde_json::to_string(&event) {
                Ok(payload) => {
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
                    break;
                }
            }
        }
    });

    while let Some(msg) = read.next().await {
        match msg {
            Ok(Message::Text(text)) => match serde_json::from_str::<ClientCommand>(&text) {
                Ok(command) => handler.handle_command(command).await,
                Err(err) => {
                    let _ = out_tx.send(ServerEvent::Error {
                        message: format!("Invalid command payload: {}", err),
                    });
                }
            },
            Ok(Message::Close(_)) => break,
            Ok(Message::Ping(payload)) => {
                let mut guard = sink.lock().await;
                let _ = guard.send(Message::Pong(payload)).await;
            }
            Err(err) => {
                tracing::warn!("Websocket error: {}", err);
                break;
            }
            _ => {}
        }
    }

    drop(out_tx);
    writer.await.ok();
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
