use super::handlers::ServerContext;
use super::upload_policy;
use super::websocket;
use crate::llm::attachment_limits::MAX_UPLOAD_BYTES;
use axum::{
    body::Body,
    extract::{
        ws::WebSocketUpgrade,
        ConnectInfo, DefaultBodyLimit, Path, Request, State,
    },
    http::{header, HeaderMap, HeaderValue, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Json, Response},
    routing::{delete, get, post},
    Router,
};
use futures::stream;
use multer::Multipart;
use serde::Serialize;
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::path::{Component, Path as StdPath, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use subtle::ConstantTimeEq;
use tower_http::timeout::TimeoutLayer;
use uuid::Uuid;

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

/// Security headers for responses that carry user-supplied content:
/// no MIME sniffing, no caching by intermediaries, and scriptable formats
/// (SVG/HTML) forced to download instead of rendering inline.
fn insert_content_headers(headers: &mut HeaderMap, mime: &str) {
    if let Ok(v) = HeaderValue::from_str(mime) {
        headers.insert(header::CONTENT_TYPE, v);
    }
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-store"),
    );
    let disposition = if mime.eq_ignore_ascii_case("image/svg+xml")
        || mime.to_ascii_lowercase().starts_with("text/html")
    {
        "attachment"
    } else {
        "inline"
    };
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static(disposition),
    );
}

/// Parse the `boundary` parameter out of a `multipart/form-data` Content-Type
/// value. Handles any parameter order, case variations and quoted values
/// (a plain `strip_prefix` parse rejected all of these).
fn parse_multipart_boundary(content_type: &str) -> Option<String> {
    let mut parts = content_type.split(';');
    if !parts.next()?.trim().eq_ignore_ascii_case("multipart/form-data") {
        return None;
    }
    for param in parts {
        let Some((name, value)) = param.split_once('=') else {
            continue;
        };
        if !name.trim().eq_ignore_ascii_case("boundary") {
            continue;
        }
        let value = value.trim();
        let value = value
            .strip_prefix('"')
            .and_then(|v| v.strip_suffix('"'))
            .unwrap_or(value);
        if value.is_empty() {
            return None;
        }
        return Some(value.to_string());
    }
    None
}

async fn require_api_key(
    State(ctx): State<Arc<ServerContext>>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    // Client IP is injected into request extensions when served via
    // `into_make_service_with_connect_info`. When absent (e.g. router-level
    // tests) throttling is skipped but auth is still enforced.
    let ip = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ConnectInfo(addr)| addr.ip());

    if let Some(ip) = ip {
        if ctx.auth_throttle.is_throttled(ip) {
            return Err(StatusCode::TOO_MANY_REQUESTS);
        }
    }

    if !authorize(request.headers(), &ctx.server_cfg.api_key) {
        if let Some(ip) = ip {
            if ctx.auth_throttle.record_failure(ip) {
                tracing::warn!(
                    %ip,
                    "Too many failed auth attempts from this IP; rejecting with 429 until the window resets"
                );
            }
        }
        return Err(StatusCode::UNAUTHORIZED);
    }

    if let Some(ip) = ip {
        ctx.auth_throttle.clear(ip);
    }
    Ok(next.run(request).await)
}

/// Per-IP authentication-failure throttle. After `max` failed attempts within
/// `window_secs`, further requests from that IP are rejected with 429 until the
/// window rolls over. Slows down API-key brute forcing when the server is
/// exposed to a network (default bind is loopback; see [server].host).
pub struct AuthThrottle {
    max: u32,
    window_secs: u64,
    /// ip -> (failure count, window start unix secs). Plain std Mutex: never
    /// held across an await.
    state: std::sync::Mutex<HashMap<IpAddr, (u32, u64)>>,
}

impl AuthThrottle {
    pub fn new(max: u32, window_secs: u64) -> Self {
        Self {
            max,
            window_secs,
            state: std::sync::Mutex::new(HashMap::new()),
        }
    }

    fn now_secs() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }

    /// Record a failed attempt; returns true when the IP just crossed the limit.
    pub fn record_failure(&self, ip: IpAddr) -> bool {
        let now = Self::now_secs();
        let mut map = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let count = {
            let entry = map.entry(ip).or_insert((0, now));
            if now.saturating_sub(entry.1) >= self.window_secs {
                *entry = (0, now);
            }
            entry.0 = entry.0.saturating_add(1);
            entry.0
        };
        // Opportunistic cleanup so the map stays bounded under spoofed/rotating
        // sources; only kicks in at a size no single user would reach.
        if map.len() > 4096 {
            map.retain(|_, (_, start)| now.saturating_sub(*start) < self.window_secs);
        }
        count >= self.max
    }

    /// True when this IP is currently over the failure limit within its window.
    pub fn is_throttled(&self, ip: IpAddr) -> bool {
        let now = Self::now_secs();
        let map = self.state.lock().unwrap_or_else(|e| e.into_inner());
        match map.get(&ip) {
            Some((count, start)) => {
                now.saturating_sub(*start) < self.window_secs && *count >= self.max
            }
            None => false,
        }
    }

    /// Forget failures for an IP after a successful authentication.
    pub fn clear(&self, ip: IpAddr) {
        let mut map = self.state.lock().unwrap_or_else(|e| e.into_inner());
        map.remove(&ip);
    }
}

type UploadError = (StatusCode, String);

pub async fn attach_file_handler(
    State(ctx): State<Arc<ServerContext>>,
    headers: HeaderMap,
    body: Body,
) -> Result<Json<AttachFileResponse>, UploadError> {
    let bad_request = |msg: &str| (StatusCode::BAD_REQUEST, msg.to_string());

    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|ct| ct.to_str().ok())
        .ok_or_else(|| bad_request("Missing Content-Type"))?;
    let boundary = parse_multipart_boundary(content_type)
        .ok_or_else(|| bad_request("Expected multipart/form-data with a boundary"))?;

    let body_bytes = axum::body::to_bytes(body, MAX_UPLOAD_BYTES)
        .await
        .map_err(|_| (StatusCode::PAYLOAD_TOO_LARGE, "Upload exceeds the size limit".to_string()))?;
    let body_stream = stream::once(async move { Ok::<_, std::io::Error>(body_bytes) });
    let mut multipart = Multipart::new(body_stream, boundary);

    let mut file_data: Option<(String, Vec<u8>)> = None;
    let mut conversation_id: Option<String> = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| bad_request("Malformed multipart body"))?
    {
        let name = field.name().unwrap_or("");
        match name {
            "file" => {
                let file_name = field
                    .file_name()
                    .ok_or_else(|| bad_request("File field has no filename"))?
                    .to_string();
                let data = field
                    .bytes()
                    .await
                    .map_err(|_| bad_request("Failed to read file field"))?
                    .to_vec();
                file_data = Some((file_name, data));
            }
            "conversation_id" => {
                let s = field
                    .text()
                    .await
                    .map_err(|_| bad_request("Failed to read conversation_id field"))?
                    .trim()
                    .to_string();
                if !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
                    conversation_id = Some(s);
                }
            }
            _ => {}
        }
    }

    let (original_name, data) = file_data.ok_or_else(|| bad_request("Missing file field"))?;

    // Content is authoritative: reject spoofed/unsupported/oversized uploads.
    let class = upload_policy::classify(&original_name, &data).map_err(|reject| {
        (
            StatusCode::from_u16(reject.status_code()).unwrap_or(StatusCode::BAD_REQUEST),
            reject.message(),
        )
    })?;

    let subdir = conversation_id
        .as_deref()
        .filter(|s| !s.is_empty())
        .unwrap_or("_no_conversation");
    let uploads_base = ctx.config.uploads_dir();
    let target_dir = uploads_base.join(subdir);
    tokio::fs::create_dir_all(&target_dir).await.map_err(|_| {
        (StatusCode::INTERNAL_SERVER_ERROR, "Failed to create upload directory".to_string())
    })?;

    // Dedupe: reuse an identical existing upload instead of storing a copy.
    if let Some((existing_uid, existing_path)) = find_duplicate(&target_dir, &data).await {
        tracing::info!(uid = %existing_uid, original_name = %original_name, "Reused identical upload");
        return Ok(Json(AttachFileResponse {
            uid: existing_uid,
            original_name,
            stored_path: existing_path.display().to_string(),
        }));
    }

    let uid = Uuid::new_v4();
    let storage_name = format!("{}.{}", uid, class.ext);
    let path = target_dir.join(&storage_name);
    tokio::fs::write(&path, &data).await.map_err(|_| {
        (StatusCode::INTERNAL_SERVER_ERROR, "Failed to write upload".to_string())
    })?;

    let stored_path = path.display().to_string();
    tracing::info!(
        uid = %uid,
        original_name = %original_name,
        ext = %class.ext,
        kind = ?class.kind,
        path = %stored_path,
        "Stored upload"
    );

    Ok(Json(AttachFileResponse {
        uid: uid.to_string(),
        original_name,
        stored_path,
    }))
}

/// Return an existing file in `dir` with identical bytes (size prefilter + byte compare).
async fn find_duplicate(dir: &StdPath, data: &[u8]) -> Option<(String, PathBuf)> {
    let mut entries = tokio::fs::read_dir(dir).await.ok()?;
    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        let meta = match tokio::fs::symlink_metadata(&path).await {
            Ok(m) => m,
            Err(_) => continue,
        };
        if !meta.is_file() || meta.len() != data.len() as u64 {
            continue;
        }
        if let Ok(existing) = tokio::fs::read(&path).await {
            if existing == data {
                let stem = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_string();
                return Some((stem, path));
            }
        }
    }
    None
}

/// Resolve an uploaded file by attachment uid across conversation subdirs.
async fn find_upload_by_uid(uploads_base: &StdPath, uid: &str) -> Option<PathBuf> {
    let mut subdirs = tokio::fs::read_dir(uploads_base).await.ok()?;
    while let Ok(Some(sub)) = subdirs.next_entry().await {
        if !sub.file_type().await.map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let Ok(mut inner) = tokio::fs::read_dir(sub.path()).await else {
            continue;
        };
        while let Ok(Some(entry)) = inner.next_entry().await {
            let matches = entry
                .file_name()
                .to_str()
                .map(|name| upload_policy::filename_matches(name, uid))
                .unwrap_or(false);
            if matches {
                return Some(entry.path());
            }
        }
    }
    None
}

/// Serve an uploaded attachment's bytes for preview, by uid (authenticated).
pub async fn attachment_file_handler(
    State(ctx): State<Arc<ServerContext>>,
    Path(uid): Path<String>,
) -> Result<Response, StatusCode> {
    if uid.is_empty()
        || uid.contains('/')
        || uid.contains('\\')
        || uid.contains("..")
        || !uid.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    let uploads_base = ctx.config.uploads_dir();
    let path = find_upload_by_uid(&uploads_base, &uid)
        .await
        .ok_or(StatusCode::NOT_FOUND)?;
    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mime = crate::llm::mime::sniff(&bytes)
        .map(|s| s.mime)
        .unwrap_or("application/octet-stream");
    let mut resp_headers = HeaderMap::new();
    insert_content_headers(&mut resp_headers, mime);
    Ok((resp_headers, bytes).into_response())
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
                        if upload_policy::filename_matches(s, &uid) {
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
    // Enforce size caps at the transport layer: without these, tungstenite
    // buffers up to its 64 MiB message / 16 MiB frame defaults before the
    // app-level check in the read loop ever sees the message (memory DoS).
    ws.max_message_size(websocket::MAX_WS_TEXT_BYTES)
        .max_frame_size(websocket::MAX_WS_FRAME_BYTES)
        .max_write_buffer_size(websocket::MAX_WS_WRITE_BUFFER_BYTES)
        .on_upgrade(move |socket| async move {
            if let Err(e) = websocket::handle_ws_upgraded(socket, ctx).await {
                tracing::warn!("WebSocket connection error: {}", e);
            }
        })
        .into_response()
}

/// Resolve `path` under `static_base` without escaping via `..`, absolute segments, or symlinks.
async fn resolve_static_path(static_base: &StdPath, path: &str) -> Option<PathBuf> {
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
    let base_canon = tokio::fs::canonicalize(static_base).await.ok()?;
    let full_canon = tokio::fs::canonicalize(&joined).await.ok()?;
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
    let Some(full_path) = resolve_static_path(&static_base, &path).await else {
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
    let mut resp_headers = HeaderMap::new();
    insert_content_headers(&mut resp_headers, mime_type.as_ref());
    if mime_type.as_ref() == "image/svg+xml" {
        // Defense in depth for user-published SVGs: sandbox neutralizes any
        // embedded script even if the file is opened directly in a browser.
        resp_headers.insert(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static("sandbox"),
        );
    }
    (resp_headers, body).into_response()
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
        .route("/api/attachment/{uid}", get(attachment_file_handler))
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
        assert!(upload_policy::filename_matches(&format!("{uid}.png"), uid));
        assert!(!upload_policy::filename_matches(&format!("{uid}x.png"), uid));
        assert!(!upload_policy::filename_matches(&format!("{uid}.tar.gz"), uid));
        assert!(!upload_policy::filename_matches(uid, uid));
    }

    #[test]
    fn parses_multipart_boundary_variants() {        // Standard form
        assert_eq!(
            parse_multipart_boundary("multipart/form-data; boundary=abc123").as_deref(),
            Some("abc123")
        );
        // Quoted boundary
        assert_eq!(
            parse_multipart_boundary("multipart/form-data; boundary=\"quoted-x\"").as_deref(),
            Some("quoted-x")
        );
        // Case variations + extra parameters before boundary
        assert_eq!(
            parse_multipart_boundary("Multipart/Form-Data; charset=utf-8; boundary=xyz").as_deref(),
            Some("xyz")
        );
        // Wrong media type / missing / empty
        assert_eq!(parse_multipart_boundary("application/json; boundary=abc"), None);
        assert_eq!(parse_multipart_boundary("multipart/form-data"), None);
        assert_eq!(parse_multipart_boundary("multipart/form-data; boundary=\"\""), None);
        assert_eq!(parse_multipart_boundary("multipart/form-data; boundary="), None);
    }

    #[test]
    fn auth_throttle_locks_after_max_failures() {
        let throttle = AuthThrottle::new(3, 60);
        let ip: IpAddr = "10.1.2.3".parse().unwrap();
        let other: IpAddr = "10.1.2.4".parse().unwrap();

        assert!(!throttle.is_throttled(ip));
        assert!(!throttle.record_failure(ip)); // 1
        assert!(!throttle.record_failure(ip)); // 2
        assert!(throttle.record_failure(ip)); // 3 -> crosses the limit
        assert!(throttle.is_throttled(ip));
        // Other IPs are unaffected.
        assert!(!throttle.is_throttled(other));

        // A successful auth clears the counter.
        throttle.clear(ip);
        assert!(!throttle.is_throttled(ip));
        assert!(!throttle.record_failure(ip));
    }

    #[tokio::test]
    async fn auth_throttle_returns_429_after_repeated_failures() {
        use std::net::{Ipv4Addr, SocketAddrV4};
        use tower::ServiceExt;

        let ctx = test_ctx_with("test-key-0123456789abcdef", 2);
        let app = create_http_router(ctx);
        let peer = ConnectInfo(SocketAddr::V4(SocketAddrV4::new(
            Ipv4Addr::new(203, 0, 113, 9),
            1234,
        )));

        let bad = || {
            Request::builder()
                .uri("/api/mcp-servers")
                .extension(peer)
                .header("x-api-key", "wrong")
                .body(Body::empty())
                .unwrap()
        };

        // First two wrong-key attempts: 401. Third crosses the limit: 429.
        assert_eq!(
            app.clone().oneshot(bad()).await.unwrap().status(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            app.clone().oneshot(bad()).await.unwrap().status(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            app.clone().oneshot(bad()).await.unwrap().status(),
            StatusCode::TOO_MANY_REQUESTS
        );

        // Throttle is per-IP: a correct key from a different IP still works.
        let other = ConnectInfo(SocketAddr::V4(SocketAddrV4::new(
            Ipv4Addr::new(198, 51, 100, 7),
            1,
        )));
        let ok_req = Request::builder()
            .uri("/api/mcp-servers")
            .extension(other)
            .header("x-api-key", "test-key-0123456789abcdef")
            .body(Body::empty())
            .unwrap();
        assert_eq!(
            app.oneshot(ok_req).await.unwrap().status(),
            StatusCode::OK
        );
    }

    #[tokio::test]
    async fn finds_upload_by_uid_across_subdirs() {
        let base = std::env::temp_dir().join(format!("luna_findupload_{}", Uuid::new_v4()));
        let sub = base.join("conv");
        tokio::fs::create_dir_all(&sub).await.unwrap();
        let uid = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";
        tokio::fs::write(sub.join(format!("{uid}.txt")), b"hi")
            .await
            .unwrap();

        assert!(find_upload_by_uid(&base, uid).await.is_some());
        assert!(find_upload_by_uid(&base, "no-such-uid").await.is_none());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[tokio::test]
    async fn resolve_static_rejects_traversal() {
        let base = std::env::temp_dir().join(format!("luna_static_test_{}", Uuid::new_v4()));
        std::fs::create_dir_all(&base).unwrap();
        std::fs::write(base.join("ok.txt"), b"hi").unwrap();
        assert!(resolve_static_path(&base, "ok.txt").await.is_some());
        assert!(resolve_static_path(&base, "../ok.txt").await.is_none());
        assert!(resolve_static_path(&base, "/etc/passwd").await.is_none());
        assert!(resolve_static_path(&base, "a/../../etc/passwd").await.is_none());
        let _ = std::fs::remove_dir_all(&base);
    }

    /// Minimal ServerContext over a throwaway temp DB for router-level tests.
    fn test_ctx(api_key: &str) -> Arc<ServerContext> {
        test_ctx_with(api_key, 30)
    }

    fn test_ctx_with(api_key: &str, auth_rate_limit_max: u32) -> Arc<ServerContext> {
        use crate::prompts::PromptManager;
        use crate::services::ScheduleService;
        use crate::storage::sqlite_storage_simple::SqliteSettings;
        use crate::storage::Storage;
        use agentic_loop::mcp_servers_registry::MCPServerRegistry;
        use tokio::sync::RwLock;

        let mut config = crate::config::AppConfig::default();
        config.server.api_key = api_key.to_string();
        config.server.auth_rate_limit_max = auth_rate_limit_max;
        let config = Arc::new(config);

        let db_path = std::env::temp_dir().join(format!("luna_http_test_{}.db", Uuid::new_v4()));
        let storage =
            Storage::new_with_settings(&db_path, SqliteSettings::default()).expect("test db");
        // Unlink now; the open connection keeps working and nothing lingers.
        let _ = std::fs::remove_file(&db_path);
        let storage = Arc::new(storage);

        Arc::new(ServerContext {
            config: config.clone(),
            server_cfg: Arc::new(config.server.clone()),
            static_token: Uuid::new_v4().to_string(),
            auth_throttle: Arc::new(AuthThrottle::new(
                config.server.auth_rate_limit_max,
                config.server.auth_rate_limit_window_secs,
            )),
            prompt_manager: PromptManager { system_prompt: None },
            storage: storage.clone(),
            mcp_registry: Arc::new(RwLock::new(MCPServerRegistry::new())),
            subscriptions: Arc::new(
                crate::server::conversation_subscriptions::ConversationSubscriptions::new(),
            ),
            schedule_service: Arc::new(ScheduleService::new(storage)),
            default_allowed_tool_names: std::collections::HashSet::new(),
            embedding_provider: None,
            active_agent_runs: Arc::new(RwLock::new(std::collections::HashMap::new())),
        })
    }

    #[tokio::test]
    async fn auth_middleware_gates_api_routes() {
        use tower::ServiceExt;

        let ctx = test_ctx("test-key-0123456789abcdef");
        let app = create_http_router(ctx);

        // No key -> 401
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/mcp-servers")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

        // Wrong key -> 401
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/mcp-servers")
                    .header("x-api-key", "wrong-key")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

        // x-api-key -> 200
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/mcp-servers")
                    .header("x-api-key", "test-key-0123456789abcdef")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // Authorization: Bearer -> 200
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/mcp-servers")
                    .header("authorization", "Bearer test-key-0123456789abcdef")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // /ws is gated before the upgrade handshake
        let resp = app
            .oneshot(Request::builder().uri("/ws").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn static_route_requires_capability_token() {
        use tower::ServiceExt;

        let ctx = test_ctx("test-key-0123456789abcdef");
        let token = ctx.static_token.clone();
        let app = create_http_router(ctx);

        // Wrong token -> 401 (token is checked before touching the filesystem)
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/static/not-the-token/nope.txt")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

        // Right token, missing file -> 404
        let resp = app
            .oneshot(
                Request::builder()
                    .uri(format!("/api/static/{token}/definitely-missing.txt"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }
}
