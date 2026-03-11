use super::handlers::ServerContext;
use super::websocket;
use axum::{
    extract::{
        ws::WebSocketUpgrade,
        Path, State,
    },
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Json, Response},
    routing::{delete, get, post},
    Router,
    body::Body,
};
use futures::stream;
use multer::Multipart;
use serde::Serialize;
use std::path::{Path as StdPath, PathBuf};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, Serialize)]
pub struct AttachFileResponse {
    pub uid: String,
    pub original_name: String,
    /// Full path on server: config_dir/uploads/{uid}.{ext}
    pub stored_path: String,
}

#[derive(Debug, Serialize)]
pub struct RemoveFileResponse {
    success: bool,
}

#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum MCPServerStatus {
    Connected,
    Failed { error: String },
}

#[derive(Debug, Serialize)]
pub struct MCPServerInfo {
    pub name: String,
    pub tool_count: u32,
    #[serde(flatten)]
    pub status: MCPServerStatus,
}

#[derive(Debug, Serialize)]
pub struct ListMCPServersResponse {
    pub servers: Vec<MCPServerInfo>,
}

fn extract_api_key(headers: &HeaderMap) -> Option<String> {
    if let Some(value) = headers.get("x-api-key") {
        return value.to_str().ok().map(|s| s.to_string());
    }
    if let Some(value) = headers.get("authorization") {
        if let Ok(header) = value.to_str() {
            if let Some(token) = header.strip_prefix("Bearer ") {
                return Some(token.trim().to_string());
            }
        }
    }
    None
}

fn authorize(headers: &HeaderMap, expected_key: &str) -> bool {
    extract_api_key(headers)
        .map(|provided| provided == expected_key)
        .unwrap_or(false)
}

pub async fn attach_file_handler(
    State(ctx): State<Arc<ServerContext>>,
    headers: HeaderMap,
    body: Body,
) -> Result<Json<AttachFileResponse>, StatusCode> {
    if !authorize(&headers, &ctx.server_cfg.api_key) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|ct| ct.to_str().ok())
        .ok_or(StatusCode::BAD_REQUEST)?;
    let boundary = content_type
        .strip_prefix("multipart/form-data; boundary=")
        .ok_or(StatusCode::BAD_REQUEST)?;

    let body_bytes = axum::body::to_bytes(body, usize::MAX)
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let body_stream = stream::once(async move { Ok::<_, std::io::Error>(body_bytes) });
    let mut multipart = Multipart::new(body_stream, boundary);

    let mut file_data: Option<(String, Vec<u8>)> = None;
    let mut conversation_id: Option<String> = None;
    while let Some(field) = multipart.next_field().await.map_err(|_| StatusCode::BAD_REQUEST)? {
        let name = field.name().unwrap_or("");
        match name {
            "file" => {
                let file_name = field
                    .file_name()
                    .ok_or(StatusCode::BAD_REQUEST)?
                    .to_string();
                let data = field
                    .bytes()
                    .await
                    .map_err(|_| StatusCode::BAD_REQUEST)?
                    .to_vec();
                file_data = Some((file_name, data));
            }
            "conversation_id" => {
                let s = field
                    .text()
                    .await
                    .map_err(|_| StatusCode::BAD_REQUEST)?
                    .trim()
                    .to_string();
                if !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
                    conversation_id = Some(s);
                }
            }
            _ => {}
        }
    }

    let (original_name, data) = file_data.ok_or(StatusCode::BAD_REQUEST)?;

    let subdir = conversation_id
        .as_deref()
        .filter(|s| !s.is_empty())
        .unwrap_or("_no_conversation");
    let uid = Uuid::new_v4();
    let ext = StdPath::new(&original_name)
        .extension()
        .and_then(|e| e.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("bin");
    let storage_name = format!("{}.{}", uid, ext);

    let uploads_base = ctx.config.uploads_dir();
    let target_dir = uploads_base.join(subdir);
    std::fs::create_dir_all(&target_dir).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let path = target_dir.join(&storage_name);
    std::fs::write(&path, &data).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let stored_path = path.display().to_string();

    tracing::info!(
        "File uploaded: original_name={}, uid={}, stored_path={}, size={}",
        original_name,
        uid,
        stored_path,
        data.len()
    );

    Ok(Json(AttachFileResponse {
        uid: uid.to_string(),
        original_name,
        stored_path,
    }))
}

pub async fn remove_file_handler(
    State(ctx): State<Arc<ServerContext>>,
    Path(uid): Path<String>,
    headers: HeaderMap,
) -> Result<Json<RemoveFileResponse>, StatusCode> {
    if !authorize(&headers, &ctx.server_cfg.api_key) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let uploads_base = ctx.config.uploads_dir();
    let prefix = format!("{}.", uid);
    let mut removed = false;
    if let Ok(rd) = std::fs::read_dir(&uploads_base) {
        for sub in rd.flatten() {
            let path = sub.path();
            if !path.is_dir() {
                continue;
            }
            if let Ok(inner) = std::fs::read_dir(&path) {
                for e in inner.flatten() {
                    let name = e.file_name();
                    if let Some(s) = name.to_str() {
                        if s.starts_with(&prefix) {
                            if std::fs::remove_file(e.path()).is_ok() {
                                removed = true;
                                tracing::info!("Removed upload {}", s);
                            }
                            break;
                        }
                    }
                }
            }
            if removed {
                break;
            }
        }
    }

    Ok(Json(RemoveFileResponse { success: removed }))
}

pub async fn list_mcp_servers_handler(
    State(ctx): State<Arc<ServerContext>>,
    headers: HeaderMap,
) -> Result<Json<ListMCPServersResponse>, StatusCode> {
    // Check authorization
    if !authorize(&headers, &ctx.server_cfg.api_key) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let registry = ctx.mcp_registry.read().await;
    let servers_with_status = registry.get_all_server_names_and_statuses();

    // Get tool counts for connected servers
    let mut tool_counts: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    for server_name in registry.get_server_names() {
        let count = registry.get_all_tools_by_server_name(&server_name).len() as u32;
        tool_counts.insert(server_name, count);
    }

    let servers: Vec<MCPServerInfo> = servers_with_status
        .into_iter()
        .map(|server| {
            let tool_count = match &server.server_status {
                crate::mcp::ServerStatus::Connected => {
                    tool_counts.get(&server.server_name).copied().unwrap_or(0)
                }
                crate::mcp::ServerStatus::Failed(_) => 0,
            };

            MCPServerInfo {
                name: server.server_name,
                tool_count,
                status: match server.server_status {
                    crate::mcp::ServerStatus::Connected => MCPServerStatus::Connected,
                    crate::mcp::ServerStatus::Failed(error) => MCPServerStatus::Failed { error },
                },
            }
        })
        .collect();

    Ok(Json(ListMCPServersResponse { servers }))
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    headers: HeaderMap,
    State(ctx): State<Arc<ServerContext>>,
) -> Response {
    if !authorize(&headers, &ctx.server_cfg.api_key) {
        return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
    }
    let ctx = ctx.clone();
    ws.on_upgrade(move |socket| async move {
        if let Err(e) = websocket::handle_ws_upgraded(socket, ctx).await {
            tracing::warn!("WebSocket connection error: {}", e);
        }
    })
    .into_response()
}

/// Serve static files from config_dir/static at /api/static/{api_key}/{*path}.
/// Auth: api_key is the first path segment (e.g. /api/static/YourKey/conv-id/image.jpg).
pub async fn static_file_handler(
    Path((api_key, path)): Path<(String, String)>,
    State(ctx): State<Arc<ServerContext>>,
) -> Response {
    if api_key != ctx.server_cfg.api_key {
        return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
    }
    let path = path.trim_start_matches('/');
    if path.is_empty() || path.contains("..") {
        return (StatusCode::NOT_FOUND, "Not found").into_response();
    }
    let static_base = ctx.config.static_dir();
    let full_path: PathBuf = static_base.join(path);
    if !full_path.starts_with(&static_base) {
        return (StatusCode::NOT_FOUND, "Not found").into_response();
    }
    let meta = match tokio::fs::metadata(&full_path).await {
        Ok(m) => m,
        Err(_) => return (StatusCode::NOT_FOUND, "Not found").into_response(),
    };
    if !meta.is_file() {
        return (StatusCode::NOT_FOUND, "Not found").into_response();
    }
    let body = match tokio::fs::read(&full_path).await {
        Ok(b) => b,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to read file").into_response(),
    };
    let mime_type = mime_guess::from_path(&full_path).first_or_octet_stream();
    (
        [(header::CONTENT_TYPE, mime_type.as_ref())],
        body,
    )
        .into_response()
}

pub fn create_http_router(ctx: Arc<ServerContext>) -> Router {
    Router::new()
        .route("/api/attach-file", post(attach_file_handler))
        .route("/api/attach-file/{file_id}", delete(remove_file_handler))
        .route("/api/mcp-servers", get(list_mcp_servers_handler))
        .route("/api/static/{api_key}/{*path}", get(static_file_handler))
        .route("/ws", get(ws_handler))
        .with_state(ctx)
}

