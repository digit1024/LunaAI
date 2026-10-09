use crate::llm::attachment_limits::MAX_IMAGE_BYTES;
use crate::llm::mime;
use crate::llm::Attachment;
use anyhow::{Context, Result};
use base64::Engine;
use std::fs;
use std::path::Path;
use tracing;

/// Read up to `max` leading bytes of a file (for content sniffing).
fn read_head(path: &Path, max: usize) -> std::io::Result<Vec<u8>> {
    use std::io::Read;
    let mut file = fs::File::open(path)?;
    let mut buf = vec![0u8; max];
    let n = file.read(&mut buf)?;
    buf.truncate(n);
    Ok(buf)
}

/// Supported file types for LLM processing
#[derive(Debug, Clone, PartialEq)]
pub enum FileType {
    Text,
    Image,
    Document,
    #[allow(dead_code)] // Reserved for future use
    Unsupported,
}

impl FileType {
    pub fn from_extension(extension: &str) -> Self {
        match extension.to_lowercase().as_str() {
            // Images
            "jpg" | "jpeg" | "png" | "gif" | "bmp" | "webp" | "svg" | "tiff" | "ico" => {
                FileType::Image
            }
            // Documents
            "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "odt" | "ods" | "odp" => {
                FileType::Document
            }
            // Everything else is treated as text (including unknown extensions)
            _ => FileType::Text,
        }
    }
}

/// Create an attachment from a file path
pub fn create_attachment(file_path: &str) -> Result<Attachment> {
    let path = Path::new(file_path);

    if !path.exists() {
        anyhow::bail!("File does not exist: {}", file_path);
    }

    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| anyhow::anyhow!("Failed to extract file name from path: invalid file name"))
        .with_context(|| format!("Invalid file name for path: {}", file_path))?
        .to_string();

    let metadata = fs::metadata(path)?;
    let file_size = metadata.len();

    // Determine type: prefer the extension, but fall back to content sniffing
    // for extension-less / unknown files so `.bin` uploads still classify.
    let mut extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_string();
    let mut mime_type = get_mime_type_from_extension(&extension);
    let mut file_type = FileType::from_extension(&extension);

    if extension.is_empty() || !mime::is_known_ext(&extension) {
        if let Ok(head) = read_head(path, 8192) {
            if let Some(sniffed) = mime::sniff(&head) {
                if sniffed.mime.starts_with("image/") || sniffed.ext == "pdf" {
                    extension = sniffed.ext.to_string();
                    mime_type = sniffed.mime.to_string();
                    file_type = FileType::from_extension(&extension);
                } else if extension.is_empty() && sniffed.ext == "txt" {
                    mime_type = "text/plain".to_string();
                    file_type = FileType::Text;
                }
            }
        }
    }

    // Extract content based on file type
    let (content, final_mime_type, final_file_name) = match file_type {
        FileType::Text => {
            // Full text. The RAG-aware inline trim happens exactly once, later,
            // in the send handler — this function must not truncate.
            let text_content = fs::read_to_string(path).ok();
            (text_content, mime_type, file_name)
        }
        FileType::Document => {
            // For document files (PDF, DOCX, XLSX, etc.), convert to markdown
            match markdownify::MarkdownifyInput::from_path(path).and_then(|input| input.convert()) {
                Ok(markdown_content) => {
                    tracing::debug!(
                        file_name = %file_name,
                        content_length = markdown_content.len(),
                        "Converted file to markdown"
                    );
                    // Update file name to indicate it's now markdown
                    let markdown_file_name = if let Some(base_name) = path
                        .file_stem()
                        .and_then(|n| n.to_str())
                    {
                        format!("{}.md", base_name)
                    } else {
                        format!("{}.md", file_name)
                    };
                    (
                        Some(markdown_content),
                        "text/markdown".to_string(),
                        markdown_file_name,
                    )
                }
                Err(e) => {
                    tracing::warn!(
                        file_name = %file_name,
                        error = %e,
                        "Failed to convert file to markdown, storing without content"
                    );
                    // Fall back to storing original file without content
                    (None, mime_type, file_name)
                }
            }
        }
        FileType::Image => {
            let b64 = if file_size <= MAX_IMAGE_BYTES {
                match fs::read(path) {
                    Ok(bytes) => Some(base64::engine::general_purpose::STANDARD.encode(bytes)),
                    Err(e) => {
                        tracing::warn!(file_name = %file_name, error = %e, "Failed to read image for base64");
                        None
                    }
                }
            } else {
                tracing::warn!(file_name = %file_name, file_size, "Image too large for inline encoding");
                None
            };
            (b64, mime_type, file_name)
        }
        FileType::Unsupported => {
            // Unsupported files - try to read as text
            let text_content = fs::read_to_string(path).ok();
            (text_content, mime_type, file_name)
        }
    };

    Ok(Attachment {
        file_path: file_path.to_string(),
        file_name: final_file_name,
        mime_type: final_mime_type,
        file_size,
        content,
        description: None,
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

/// Validate if a file can be processed by the LLM
pub fn validate_file_for_llm(attachment: &Attachment) -> Result<()> {
    let file_type = FileType::from_extension(
        std::path::Path::new(&attachment.file_path)
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or(""),
    );

    match file_type {
        FileType::Image => {
            if attachment.file_size > MAX_IMAGE_BYTES {
                anyhow::bail!("Image file too large: {} bytes (max {})", attachment.file_size, MAX_IMAGE_BYTES)
            } else {
                Ok(())
            }
        }
        FileType::Document => {
            // Check file size limit for documents (e.g., 100MB)
            if attachment.file_size > 100 * 1024 * 1024 {
                anyhow::bail!("Document file too large: {} bytes", attachment.file_size)
            } else {
                Ok(())
            }
        }
        FileType::Text => {
            // Check file size limit for text files (e.g., 10MB)
            if attachment.file_size > 10 * 1024 * 1024 {
                anyhow::bail!("Text file too large: {} bytes", attachment.file_size)
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

/// Remove bulky inline image payloads before persisting to SQLite (paths remain for rehydration).
pub fn strip_attachments_for_storage(mut attachments: Vec<Attachment>) -> Vec<Attachment> {
    for a in &mut attachments {
        if a.mime_type.starts_with("image/") {
            a.content = None;
        }
    }
    attachments
}

/// Restore base64 image `content` from `file_path` when missing (e.g. after loading from DB).
pub fn hydrate_attachments_for_llm(attachments: &mut [Attachment]) -> Result<()> {
    for a in attachments.iter_mut() {
        if !a.mime_type.starts_with("image/") {
            continue;
        }
        if a.content.is_some() {
            continue;
        }
        // Already described by the auxiliary vision model: no need to rebuild
        // base64 the providers won't use.
        if a.description.is_some() {
            continue;
        }
        let path = Path::new(&a.file_path);
        if !path.exists() {
            tracing::warn!(path = %a.file_path, "Attachment file missing for rehydration");
            continue;
        }
        let meta = fs::metadata(path)?;
        if meta.len() > MAX_IMAGE_BYTES {
            tracing::warn!(path = %a.file_path, "Skipping rehydration: image too large");
            continue;
        }
        let bytes = fs::read(path)?;
        a.content = Some(base64::engine::general_purpose::STANDARD.encode(bytes));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp_file(name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("luna_attach_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(bytes).unwrap();
        path
    }

    #[test]
    fn unknown_extension_text_classifies_as_text() {
        let path = temp_file("blob.bin", b"hello, this is plain text");
        let attachment = create_attachment(path.to_str().unwrap()).unwrap();
        assert!(attachment.mime_type.starts_with("text/"), "{}", attachment.mime_type);
        assert_eq!(attachment.content.as_deref(), Some("hello, this is plain text"));
    }

    #[test]
    fn extensionless_png_classifies_as_image() {
        let path = temp_file("blob_no_ext", b"\x89PNG\r\n\x1a\n\x00\x00\x00\x00");
        let attachment = create_attachment(path.to_str().unwrap()).unwrap();
        assert_eq!(attachment.mime_type, "image/png");
        assert!(attachment.content.is_some(), "image should be base64-encoded");
    }

    #[test]
    fn text_is_not_truncated_at_creation() {
        let big = "a".repeat(600_000);
        let path = temp_file("big.txt", big.as_bytes());
        let attachment = create_attachment(path.to_str().unwrap()).unwrap();
        assert_eq!(attachment.content.as_ref().unwrap().chars().count(), 600_000);
    }

    #[test]
    fn hydrate_skips_described_images() {
        let path = temp_file("pic.png", b"\x89PNG\r\n\x1a\n\x00\x00");
        let mut attachment = Attachment {
            file_path: path.to_str().unwrap().to_string(),
            file_name: "pic.png".to_string(),
            mime_type: "image/png".to_string(),
            file_size: 8,
            content: None,
            description: Some("a blue square".to_string()),
        };
        hydrate_attachments_for_llm(std::slice::from_mut(&mut attachment)).unwrap();
        assert!(
            attachment.content.is_none(),
            "described images must not be rehydrated to base64"
        );
    }
}
