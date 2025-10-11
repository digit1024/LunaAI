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
    pub fn from_extension(extension: &str) -> Self {
        match extension.to_lowercase().as_str() {
            // Images
            "jpg" | "jpeg" | "png" | "gif" | "bmp" | "webp" | "svg" | "tiff" | "ico" => FileType::Image,
            // Documents
            "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "odt" | "ods" | "odp" => FileType::Document,
            // Everything else is treated as text (including unknown extensions)
            _ => FileType::Text,
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
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document".to_string(),
        "xls" => "application/vnd.ms-excel".to_string(),
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet".to_string(),
        "ppt" => "application/vnd.ms-powerpoint".to_string(),
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation".to_string(),
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

/// Validate if a file can be processed by the LLM
pub fn validate_file_for_llm(attachment: &Attachment) -> Result<()> {
    let file_type = FileType::from_extension(
        std::path::Path::new(&attachment.file_path)
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("")
    );
    
    match file_type {
        FileType::Image => {
            // Check file size limit for images (e.g., 50MB)
            if attachment.file_size > 50 * 1024 * 1024 {
                Err(anyhow::anyhow!("Image file too large: {} bytes", attachment.file_size))
            } else {
                Ok(())
            }
        }
        FileType::Document => {
            // Check file size limit for documents (e.g., 100MB)
            if attachment.file_size > 100 * 1024 * 1024 {
                Err(anyhow::anyhow!("Document file too large: {} bytes", attachment.file_size))
            } else {
                Ok(())
            }
        }
        FileType::Text => {
            // Check file size limit for text files (e.g., 10MB)
            if attachment.file_size > 10 * 1024 * 1024 {
                Err(anyhow::anyhow!("Text file too large: {} bytes", attachment.file_size))
            } else {
                Ok(())
            }
        }
        FileType::Unsupported => {
            // This should never happen now since everything defaults to Text
            Ok(())
        }
    }
}