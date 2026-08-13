use super::handlers::ServerContext;
use super::websocket;
use axum::{
    body::Body,
    extract::{
        ws::WebSocketUpgrade,
        DefaultBodyLimit, Path, Request, State,
    },
    http::{header, HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Json, Response},
    routing::{delete, get, post},
    Router,
};
use futures::stream;
use multer::Multipart;
use serde::Serialize;
use std::path::{Component, Path as StdPath, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use subtle::ConstantTimeEq;
use tower_http::timeout::TimeoutLayer;
use uuid::Uuid;

const MAX_UPLOAD_BYTES: usize = 50 * 1024 * 1024; // 50 MB
/// Cap for non-upload HTTP JSON/small requests.
const MAX_HTTP_BODY_BYTES: usize = 2 * 1024 * 1024;
/// Idle/request timeout for HTTP handlers only (not WebSocket).
const HTTP_REQUEST_TIMEOUT: Duration = Duration::from_secs(120);

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
    pub tools: Vec<String>,
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
        .map(|provided| bool::from(provided.as_bytes().ct_eq(expected_key.as_bytes())))
        .unwrap_or(false)
}

fn ct_eq_str(a: &str, b: &str) -> bool {
    bool::from(a.as_bytes().ct_eq(b.as_bytes()))
}

/// True when `name` is exactly `{attachment_uid}.{ext}` with a single extension segment.
fn attachment_filename_matches(name: &str, attachment_uid: &str) -> bool {
    let Some((stem, ext)) = name.split_once('.') else {
        return false;
    };
    !ext.is_empty()
        && !ext.contains('.')
        && !stem.is_empty()
        && stem == attachment_uid
}

async fn require_api_key(
    State(ctx): State<Arc<ServerContext>>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    if !authorize(request.headers(), &ctx.server_cfg.api_key) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    Ok(next.run(request).await)
}

pub async fn attach_file_handler(
    State(ctx): State<Arc<ServerContext>>,
    headers: HeaderMap,
    body: Body,
) -> Result<Json<AttachFileResponse>, StatusCode> {
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|ct| ct.to_str().ok())
        .ok_or(StatusCode::BAD_REQUEST)?;
    let boundary = content_type
        .strip_prefix("multipart/form-data; boundary=")
        .ok_or(StatusCode::BAD_REQUEST)?;

    let body_bytes = axum::body::to_bytes(body, MAX_UPLOAD_BYTES)
        .await
        .map_err(|_| StatusCode::PAYLOAD_TOO_LARGE)?;
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
    tokio::fs::create_dir_all(&target_dir)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let path = target_dir.join(&storage_name);
    tokio::fs::write(&path, &data)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let stored_path = path.display().to_string();

    tracing::info!(
        uid = %uid,
        original_name = %original_name,
        path = %stored_path,
        "Stored upload"
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
) -> Result<Json<RemoveFileResponse>, StatusCode> {
    // Reject path-ish or non-uuid-looking ids early.
    if uid.is_empty()
        || uid.contains('/')
        || uid.contains('\\')
        || uid.contains("..")
        || !uid.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return Err(StatusCode::BAD_REQUEST);
    }

    let uploads_base = ctx.config.uploads_dir();
    let mut removed = false;
    if let Ok(mut rd) = tokio::fs::read_dir(&uploads_base).await {
        while let Ok(Some(sub)) = rd.next_entry().await {
            let path = sub.path();
            if !path.is_dir() {
                continue;
            }
            if let Ok(mut inner) = tokio::fs::read_dir(&path).await {
                while let Ok(Some(e)) = inner.next_entry().await {
                    let name = e.file_name();
                    if let Some(s) = name.to_str() {
                        if attachment_filename_matches(s, &uid) {
                            if tokio::fs::remove_file(e.path()).await.is_ok() {
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
) -> Result<Json<ListMCPServersResponse>, StatusCode> {
    let registry = ctx.mcp_registry.read().await;
    let servers_with_status = registry
        .get_all_server_names_and_statuses()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Get tool names for connected servers
    let mut tool_names_map: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for (server_name, server_connection) in registry.servers.iter() {
        let connection_guard = server_connection.read().await;
        let names: Vec<String> = connection_guard
            .tools()
            .iter()
            .map(|tool| tool.name.clone())
            .collect();
        drop(connection_guard);
        tool_names_map.insert(server_name.clone(), names);
    }
    drop(registry); // Release the read lock

    let servers: Vec<MCPServerInfo> = servers_with_status
        .into_iter()
        .map(|server| {
            let tools = match &server.server_status {
                agentic_loop::mcp_servers_registry::model::ServerStatus::Connected => {
                    tool_names_map
                        .get(&server.server_name)
                        .cloned()
                        .unwrap_or_default()
                }
                agentic_loop::mcp_servers_registry::model::ServerStatus::Failed(_) => Vec::new(),
            };
            let tool_count = tools.len() as u32;

            MCPServerInfo {
                name: server.server_name,
                tool_count,
                tools,
                status: match server.server_status {
                    agentic_loop::mcp_servers_registry::model::ServerStatus::Connected => {
                        MCPServerStatus::Connected
                    }
                    agentic_loop::mcp_servers_registry::model::ServerStatus::Failed(error) => {
                        MCPServerStatus::Failed { error }
                    }
                },
            }
        })
        .collect();

    Ok(Json(ListMCPServersResponse { servers }))
}

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(ctx): State<Arc<ServerContext>>,
) -> Response {
    let ctx = ctx.clone();
    ws.on_upgrade(move |socket| async move {
        if let Err(e) = websocket::handle_ws_upgraded(socket, ctx).await {
            tracing::warn!("WebSocket connection error: {}", e);
        }
    })
    .into_response()
}

/// Resolve `path` under `static_base` without escaping via `..`, absolute segments, or symlinks.
fn resolve_static_path(static_base: &StdPath, path: &str) -> Option<PathBuf> {
    let path = path.trim_start_matches('/');
    if path.is_empty() {
        return None;
    }
    let user = StdPath::new(path);
    let mut joined = static_base.to_path_buf();
    for component in user.components() {
        match component {
            Component::Normal(seg) => joined.push(seg),
            _ => return None,
        }
    }
    let base_canon = std::fs::canonicalize(static_base).ok()?;
    let full_canon = std::fs::canonicalize(&joined).ok()?;
    if !full_canon.starts_with(&base_canon) {
        return None;
    }
    Some(full_canon)
}

/// Serve static files from config_dir/static at /api/static/{static_token}/{*path}.
/// Auth: ephemeral capability token from HealthOk (not the permanent API key).
pub async fn static_file_handler(
    Path((token, path)): Path<(String, String)>,
    State(ctx): State<Arc<ServerContext>>,
) -> Response {
    if !ct_eq_str(&token, &ctx.static_token) {
        return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
    }
    let static_base = ctx.config.static_dir();
    let Some(full_path) = resolve_static_path(&static_base, &path) else {
        return (StatusCode::NOT_FOUND, "Not found").into_response();
    };
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
    let auth_layer = middleware::from_fn_with_state(ctx.clone(), require_api_key);

    // Upload path: larger body limit, still authenticated.
    let upload = Router::new()
        .route("/api/attach-file", post(attach_file_handler))
        .route_layer(auth_layer.clone())
        .layer(DefaultBodyLimit::max(MAX_UPLOAD_BYTES));

    // Other authenticated HTTP routes (not WebSocket).
    let authed_http = Router::new()
        .route("/api/attach-file/{file_id}", delete(remove_file_handler))
        .route("/api/mcp-servers", get(list_mcp_servers_handler))
        .route_layer(auth_layer.clone())
        .layer(DefaultBodyLimit::max(MAX_HTTP_BODY_BYTES));

    // Static files: capability token auth inside handler.
    let static_routes = Router::new()
        .route(
            "/api/static/{static_token}/{*path}",
            get(static_file_handler),
        )
        .layer(DefaultBodyLimit::max(MAX_HTTP_BODY_BYTES));

    // Timed HTTP surface — do NOT wrap /ws (TimeoutLayer would kill long-lived sockets).
    let http = Router::new()
        .merge(upload)
        .merge(authed_http)
        .merge(static_routes)
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            HTTP_REQUEST_TIMEOUT,
        ));

    let ws = Router::new()
        .route("/ws", get(ws_handler))
        .route_layer(auth_layer);

    Router::new()
        .merge(http)
        .merge(ws)
        .with_state(ctx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attachment_filename_exact_match() {
        let uid = "11111111-2222-3333-4444-555555555555";
        assert!(attachment_filename_matches(&format!("{uid}.png"), uid));
        assert!(!attachment_filename_matches(&format!("{uid}x.png"), uid));
        assert!(!attachment_filename_matches(&format!("{uid}.tar.gz"), uid));
        assert!(!attachment_filename_matches(uid, uid));
    }

    #[test]
    fn resolve_static_rejects_traversal() {
        let base = std::env::temp_dir().join(format!("luna_static_test_{}", Uuid::new_v4()));
        std::fs::create_dir_all(&base).unwrap();
        std::fs::write(base.join("ok.txt"), b"hi").unwrap();
        assert!(resolve_static_path(&base, "ok.txt").is_some());
        assert!(resolve_static_path(&base, "../ok.txt").is_none());
        assert!(resolve_static_path(&base, "/etc/passwd").is_none());
        assert!(resolve_static_path(&base, "a/../../etc/passwd").is_none());
        let _ = std::fs::remove_dir_all(&base);
    }
}
