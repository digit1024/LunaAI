//! Content-based type detection (leading magic bytes) shared by the upload
//! policy and attachment classification.
//!
//! Extension-based MIME is spoofable and absent on many uploads, so both the
//! HTTP upload policy and `file_utils::create_attachment` consult these helpers
//! to recover the real type from the bytes.

/// A detected type from magic bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sniffed {
    pub ext: &'static str,
    pub mime: &'static str,
}

const IMAGE_EXTS: &[&str] = &[
    "jpg", "jpeg", "png", "gif", "bmp", "webp", "tiff", "tif", "ico", "svg",
];
const DOCUMENT_EXTS: &[&str] = &[
    "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "odt", "ods", "odp",
];
const TEXT_EXTS: &[&str] = &[
    "txt", "md", "json", "xml", "yaml", "yml", "toml", "csv", "log", "html", "htm",
    "css", "js", "ts", "jsx", "tsx", "py", "rs", "sh", "bash", "zsh", "c", "cc",
    "cpp", "h", "hpp", "java", "go", "php", "rb", "swift", "kt", "scala", "r", "m",
    "pl", "lua", "sql", "ini", "cfg", "conf",
];

/// Office/document formats that are ZIP containers on disk.
pub const OFFICE_EXTS: &[&str] = &["docx", "xlsx", "pptx", "odt", "ods", "odp"];

pub fn is_image_ext(ext: &str) -> bool {
    IMAGE_EXTS.contains(&ext)
}
pub fn is_document_ext(ext: &str) -> bool {
    DOCUMENT_EXTS.contains(&ext)
}
pub fn is_text_ext(ext: &str) -> bool {
    TEXT_EXTS.contains(&ext)
}
pub fn is_known_ext(ext: &str) -> bool {
    is_image_ext(ext) || is_document_ext(ext) || is_text_ext(ext)
}

/// True when the bytes look like UTF-8 text (no NUL, decodable head).
pub fn looks_like_text(bytes: &[u8]) -> bool {
    let head = &bytes[..bytes.len().min(8192)];
    if head.contains(&0) {
        return false;
    }
    if std::str::from_utf8(head).is_ok() {
        return true;
    }
    // The 8 KiB cut may have split a multi-byte char; retry a few bytes shorter.
    let trim = head.len().saturating_sub(3);
    std::str::from_utf8(&head[..trim]).is_ok()
}

/// Detect a type from leading magic bytes. `None` = unrecognized binary.
pub fn sniff(bytes: &[u8]) -> Option<Sniffed> {
    let s = |ext, mime| Some(Sniffed { ext, mime });

    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return s("png", "image/png");
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return s("jpg", "image/jpeg");
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return s("gif", "image/gif");
    }
    if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return s("webp", "image/webp");
    }
    if bytes.starts_with(b"BM") {
        return s("bmp", "image/bmp");
    }
    if bytes.starts_with(b"II*\0") || bytes.starts_with(b"MM\0*") {
        return s("tiff", "image/tiff");
    }
    if bytes.starts_with(b"\0\0\x01\0") {
        return s("ico", "image/x-icon");
    }
    if bytes.starts_with(b"%PDF") {
        return s("pdf", "application/pdf");
    }
    if bytes.starts_with(b"PK\x03\x04") || bytes.starts_with(b"PK\x05\x06") {
        return s("zip", "application/zip");
    }
    if looks_like_text(bytes) {
        return s("txt", "text/plain");
    }
    None
}

/// Lowercased extension of a filename, or `""`.
pub fn extension_of(name: &str) -> String {
    std::path::Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniffs_common_formats() {
        assert_eq!(sniff(b"\x89PNG\r\n\x1a\n....").unwrap().ext, "png");
        assert_eq!(sniff(&[0xFF, 0xD8, 0xFF, 0xE0]).unwrap().ext, "jpg");
        assert_eq!(sniff(b"GIF89a......").unwrap().ext, "gif");
        assert_eq!(sniff(b"RIFF\0\0\0\0WEBPVP8 ").unwrap().ext, "webp");
        assert_eq!(sniff(b"%PDF-1.7").unwrap().ext, "pdf");
        assert_eq!(sniff(b"PK\x03\x04rest").unwrap().ext, "zip");
        assert_eq!(sniff(b"hello world").unwrap().ext, "txt");
        assert!(sniff(&[0x00, 0x01, 0x02, 0x03]).is_none());
    }

    #[test]
    fn text_detection_rejects_binary() {
        assert!(looks_like_text("plain text\n".as_bytes()));
        assert!(!looks_like_text(&[0x00, 0xFF, 0x10, 0x20]));
    }
}
