use super::handlers::ServerContext;
use crate::llm::{file_utils, Attachment};
use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode},
    response::Json,
    routing::{delete, post},
    Router,
    body::Body,
};
use futures::stream;
use multer::Multipart;
use serde::Serialize;
use std::{collections::HashMap, path::Path as StdPath, sync::Arc};
use tokio::sync::RwLock;
use uuid::Uuid;

/// Get MIME type from file extension (matches logic from file_utils.rs)
fn get_mime_type_from_extension(extension: &str) -> String {
    match extension.to_lowercase().as_str() {
        // Images
        "jpg" | "jpeg" => "image/jpeg".to_string(),
        "png" => "image/png".to_string(),
        "gif" => "image/gif".to_string(),
        "bmp" => "image/bmp".to_string(),
        "webp" => "image/webp".to_string(),
        "svg" => "image/svg+xml".to_string(),
        "tiff" => "image/tiff".to_string(),
        "ico" => "image/x-icon".to_string(),
        // Documents
        "pdf" => "application/pdf".to_string(),
        "doc" => "application/msword".to_string(),
        "docx" => {
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document".to_string()
        }
        "xls" => "application/vnd.ms-excel".to_string(),
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet".to_string(),
        "ppt" => "application/vnd.ms-powerpoint".to_string(),
        "pptx" => {
            "application/vnd.openxmlformats-officedocument.presentationml.presentation".to_string()
        }
        "odt" => "application/vnd.oasis.opendocument.text".to_string(),
        "ods" => "application/vnd.oasis.opendocument.spreadsheet".to_string(),
        "odp" => "application/vnd.oasis.opendocument.presentation".to_string(),
        // Common text files
        "txt" => "text/plain".to_string(),
        "md" => "text/markdown".to_string(),
        "json" => "application/json".to_string(),
        "xml" => "application/xml".to_string(),
        "csv" => "text/csv".to_string(),
        "log" => "text/plain".to_string(),
        "yaml" | "yml" => "text/yaml".to_string(),
        "html" => "text/html".to_string(),
        "css" => "text/css".to_string(),
        "py" => "text/x-python".to_string(),
        "rs" => "text/x-rust".to_string(),
        "js" => "text/javascript".to_string(),
        "ts" => "text/typescript".to_string(),
        "sh" | "bash" | "zsh" => "text/x-shellscript".to_string(),
        "c" => "text/x-c".to_string(),
        "cpp" | "hpp" => "text/x-c++".to_string(),
        "h" => "text/x-c".to_string(),
        "java" => "text/x-java".to_string(),
        "go" => "text/x-go".to_string(),
        "php" => "text/x-php".to_string(),
        "rb" => "text/x-ruby".to_string(),
        "swift" => "text/x-swift".to_string(),
        "kt" => "text/x-kotlin".to_string(),
        "scala" => "text/x-scala".to_string(),
        "r" => "text/x-r".to_string(),
        "m" => "text/x-objective-c".to_string(),
        "pl" => "text/x-perl".to_string(),
        "lua" => "text/x-lua".to_string(),
        "sql" => "text/x-sql".to_string(),
        "toml" => "text/x-toml".to_string(),
        "ini" | "cfg" | "conf" => "text/plain".to_string(),
        // Everything else defaults to text/plain
        _ => "text/plain".to_string(),
    }
}

#[derive(Debug, Clone)]
pub struct AttachmentStorage {
    attachments: Arc<RwLock<HashMap<String, Attachment>>>,
}

impl AttachmentStorage {
    pub fn new() -> Self {
        Self {
            attachments: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn store(&self, file_id: String, attachment: Attachment) {
        let mut guard = self.attachments.write().await;
        guard.insert(file_id, attachment);
    }

    pub async fn get(&self, file_id: &str) -> Option<Attachment> {
        let guard = self.attachments.read().await;
        guard.get(file_id).cloned()
    }

    pub async fn remove(&self, file_id: &str) -> bool {
        let mut guard = self.attachments.write().await;
        guard.remove(file_id).is_some()
    }

    pub async fn get_multiple(&self, file_ids: &[String]) -> Vec<Attachment> {
        let guard = self.attachments.read().await;
        file_ids
            .iter()
            .filter_map(|id| guard.get(id).cloned())
            .collect()
    }

    pub async fn remove_multiple(&self, file_ids: &[String]) {
        let mut guard = self.attachments.write().await;
        for id in file_ids {
            guard.remove(id);
        }
    }
}

#[derive(Debug, Serialize)]
pub struct AttachFileResponse {
    file_id: String,
    file_name: String,
    mime_type: String,
    file_size: u64,
}

#[derive(Debug, Serialize)]
pub struct RemoveFileResponse {
    success: bool,
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
    // Check authorization
    if !authorize(&headers, &ctx.server_cfg.api_key) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    // Extract boundary from content-type header
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|ct| ct.to_str().ok())
        .ok_or(StatusCode::BAD_REQUEST)?;
    
    let boundary = content_type
        .strip_prefix("multipart/form-data; boundary=")
        .ok_or(StatusCode::BAD_REQUEST)?;

    // Create multipart from body
    let body_bytes = axum::body::to_bytes(body, usize::MAX)
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    
    let body_stream = stream::once(async move { Ok::<_, std::io::Error>(body_bytes) });
    let mut multipart = Multipart::new(body_stream, boundary);

    // Get the file field from multipart
    let mut file_data: Option<(String, Vec<u8>, String)> = None;

    while let Some(field) = multipart.next_field().await.map_err(|_| StatusCode::BAD_REQUEST)? {
        let field_name = field.name().unwrap_or("file");
        if field_name == "file" {
            let file_name = field
                .file_name()
                .ok_or(StatusCode::BAD_REQUEST)?
                .to_string();
            let mime_type = field
                .content_type()
                .map(|m| m.to_string())
                .unwrap_or_else(|| "application/octet-stream".to_string());
            let data = field
                .bytes()
                .await
                .map_err(|_| StatusCode::BAD_REQUEST)?
                .to_vec();
            file_data = Some((file_name, data, mime_type));
            break;
        }
    }

    let (file_name, data, mime_type) = file_data.ok_or(StatusCode::BAD_REQUEST)?;

    // Create attachment
    let file_id = Uuid::new_v4().to_string();
    let file_size = data.len() as u64;

    // Determine MIME type from file extension (more reliable than client-provided MIME type)
    let extension = StdPath::new(&file_name)
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("");
    // Use a helper to get MIME type (inline since it's private)
    let detected_mime_type = get_mime_type_from_extension(extension);
    // Use detected MIME type if client sent generic octet-stream, otherwise use client's
    let final_mime_type = if mime_type == "application/octet-stream" {
        detected_mime_type
    } else {
        mime_type.clone()
    };

    // Determine if this is a text file based on extension (same logic as desktop app)
    let content = if file_utils::FileType::from_extension(extension) == file_utils::FileType::Text {
        String::from_utf8(data.clone()).ok()
    } else {
        None
    };

    // Save file to temp directory
    let temp_dir = std::env::temp_dir().join("cosmic_llm_attachments");
    std::fs::create_dir_all(&temp_dir).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let temp_file_path = temp_dir.join(&file_id);
    std::fs::write(&temp_file_path, &data).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let attachment = Attachment {
        file_path: temp_file_path.to_string_lossy().to_string(),
        file_name: file_name.clone(),
        mime_type: final_mime_type,
        file_size,
        content,
    };

    // Validate file
    if let Err(e) = crate::llm::file_utils::validate_file_for_llm(&attachment) {
        tracing::warn!("File validation failed: {}", e);
        let _ = std::fs::remove_file(&temp_file_path);
        return Err(StatusCode::BAD_REQUEST);
    }

    // Store attachment
    ctx.attachment_storage.store(file_id.clone(), attachment).await;

    Ok(Json(AttachFileResponse {
        file_id,
        file_name,
        mime_type,
        file_size,
    }))
}

pub async fn remove_file_handler(
    State(ctx): State<Arc<ServerContext>>,
    Path(file_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<RemoveFileResponse>, StatusCode> {
    // Check authorization
    if !authorize(&headers, &ctx.server_cfg.api_key) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    // Get attachment to clean up file
    if let Some(attachment) = ctx.attachment_storage.get(&file_id).await {
        // Remove temp file
        if let Err(e) = std::fs::remove_file(&attachment.file_path) {
            tracing::warn!("Failed to remove temp file {}: {}", attachment.file_path, e);
        }
    }

    let removed = ctx.attachment_storage.remove(&file_id).await;

    Ok(Json(RemoveFileResponse { success: removed }))
}

pub fn create_http_router(ctx: Arc<ServerContext>) -> Router {
    Router::new()
        .route("/api/attach-file", post(attach_file_handler))
        .route("/api/attach-file/:file_id", delete(remove_file_handler))
        .with_state(ctx)
}

