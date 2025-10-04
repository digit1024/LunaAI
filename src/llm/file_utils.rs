use crate::llm::Attachment;
use std::path::Path;
use std::fs;
use anyhow::Result;

/// Supported file types for LLM processing
#[derive(Debug, Clone, PartialEq)]
pub enum FileType {
    Text,
    Image,
    Document,
    Unsupported,
}

impl FileType {
    pub fn from_mime_type(mime_type: &str) -> Self {
        match mime_type {
            // Text files
            mime if mime.starts_with("text/") => FileType::Text,
            // Images
            mime if mime.starts_with("image/") => FileType::Image,
            // Documents
            mime if mime == "application/pdf" => FileType::Document,
            mime if mime == "application/msword" => FileType::Document,
            mime if mime == "application/vnd.openxmlformats-officedocument.wordprocessingml.document" => FileType::Document,
            mime if mime == "application/vnd.ms-excel" => FileType::Document,
            mime if mime == "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet" => FileType::Document,
            mime if mime == "application/vnd.ms-powerpoint" => FileType::Document,
            mime if mime == "application/vnd.openxmlformats-officedocument.presentationml.presentation" => FileType::Document,
            _ => FileType::Unsupported,
        }
    }
    
    pub fn from_extension(extension: &str) -> Self {
        match extension.to_lowercase().as_str() {
            // Text files
            "txt" | "md" | "json" | "xml" | "csv" | "log" | "yaml" | "yml" => FileType::Text,
            // Images
            "jpg" | "jpeg" | "png" | "gif" | "bmp" | "webp" | "svg" => FileType::Image,
            // Documents
            "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" => FileType::Document,
            _ => FileType::Unsupported,
        }
    }
}

/// Create an attachment from a file path
pub fn create_attachment(file_path: &str) -> Result<Attachment> {
    let path = Path::new(file_path);
    
    if !path.exists() {
        return Err(anyhow::anyhow!("File does not exist: {}", file_path));
    }
    
    let file_name = path.file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| anyhow::anyhow!("Invalid file name"))?
        .to_string();
    
    let metadata = fs::metadata(path)?;
    let file_size = metadata.len();
    
    // Determine MIME type from extension
    let extension = path.extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("");
    let mime_type = get_mime_type_from_extension(extension);
    
    // Read content for text files
    let content = if FileType::from_extension(extension) == FileType::Text {
        Some(fs::read_to_string(path)?)
    } else {
        None
    };
    
    Ok(Attachment {
        file_path: file_path.to_string(),
        file_name,
        mime_type,
        file_size,
        content,
    })
}

/// Get MIME type from file extension
fn get_mime_type_from_extension(extension: &str) -> String {
    match extension.to_lowercase().as_str() {
        "txt" => "text/plain".to_string(),
        "md" => "text/markdown".to_string(),
        "json" => "application/json".to_string(),
        "xml" => "application/xml".to_string(),
        "csv" => "text/csv".to_string(),
        "log" => "text/plain".to_string(),
        "yaml" | "yml" => "text/yaml".to_string(),
        "jpg" | "jpeg" => "image/jpeg".to_string(),
        "png" => "image/png".to_string(),
        "gif" => "image/gif".to_string(),
        "bmp" => "image/bmp".to_string(),
        "webp" => "image/webp".to_string(),
        "svg" => "image/svg+xml".to_string(),
        "pdf" => "application/pdf".to_string(),
        "doc" => "application/msword".to_string(),
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document".to_string(),
        "xls" => "application/vnd.ms-excel".to_string(),
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet".to_string(),
        "ppt" => "application/vnd.ms-powerpoint".to_string(),
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation".to_string(),
        _ => "application/octet-stream".to_string(),
    }
}

/// Validate if a file can be processed by the LLM
pub fn validate_file_for_llm(attachment: &Attachment) -> Result<()> {
    let file_type = FileType::from_mime_type(&attachment.mime_type);
    
    match file_type {
        FileType::Unsupported => {
            Err(anyhow::anyhow!("Unsupported file type: {}", attachment.mime_type))
        }
        FileType::Image => {
            // Check file size limit for images (e.g., 10MB)
            if attachment.file_size > 10 * 1024 * 1024 {
                Err(anyhow::anyhow!("Image file too large: {} bytes", attachment.file_size))
            } else {
                Ok(())
            }
        }
        FileType::Text => {
            // Check file size limit for text files (e.g., 1MB)
            if attachment.file_size > 1024 * 1024 {
                Err(anyhow::anyhow!("Text file too large: {} bytes", attachment.file_size))
            } else {
                Ok(())
            }
        }
        FileType::Document => {
            // Check file size limit for documents (e.g., 5MB)
            if attachment.file_size > 5 * 1024 * 1024 {
                Err(anyhow::anyhow!("Document file too large: {} bytes", attachment.file_size))
            } else {
                Ok(())
            }
        }
    }
}