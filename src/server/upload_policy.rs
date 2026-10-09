//! Upload policy: turn an uploaded filename + bytes into an accepted type,
//! or a structured rejection. Content is authoritative — a declared extension
//! that disagrees with the bytes is rejected rather than trusted.

use crate::llm::attachment_limits::{MAX_DOC_BYTES, MAX_IMAGE_BYTES, MAX_TEXT_BYTES};
use crate::llm::mime;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UploadKind {
    Image,
    Document,
    Text,
}

impl UploadKind {
    /// Per-type upload cap (bytes).
    #[allow(dead_code)]
    pub fn max_bytes(&self) -> u64 {
        match self {
            UploadKind::Image => MAX_IMAGE_BYTES,
            UploadKind::Document => MAX_DOC_BYTES,
            UploadKind::Text => MAX_TEXT_BYTES,
        }
    }
}

/// Accepted upload: the canonical extension to store under and its kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UploadClass {
    pub ext: String,
    pub kind: UploadKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UploadReject {
    Empty,
    TooLarge { kind: UploadKind, max_bytes: u64, actual: u64 },
    UnsupportedContent,
    ExtensionMismatch { claimed: String, detected: String },
}

impl UploadReject {
    /// HTTP status for this rejection.
    pub fn status_code(&self) -> u16 {
        match self {
            UploadReject::Empty => 400,
            UploadReject::TooLarge { .. } => 413,
            UploadReject::UnsupportedContent | UploadReject::ExtensionMismatch { .. } => 415,
        }
    }

    pub fn message(&self) -> String {
        match self {
            UploadReject::Empty => "Uploaded file is empty".to_string(),
            UploadReject::TooLarge { max_bytes, actual, .. } => format!(
                "File too large: {actual} bytes (max {} MB for this type)",
                max_bytes / (1024 * 1024)
            ),
            UploadReject::UnsupportedContent => {
                "Unsupported file content (could not detect a known image, document, or text type)"
                    .to_string()
            }
            UploadReject::ExtensionMismatch { claimed, detected } => format!(
                "File extension '.{claimed}' does not match its content (detected {detected})"
            ),
        }
    }
}

fn claim_category(ext: &str) -> Option<UploadKind> {
    if mime::is_image_ext(ext) {
        Some(UploadKind::Image)
    } else if mime::is_document_ext(ext) {
        Some(UploadKind::Document)
    } else if mime::is_text_ext(ext) {
        Some(UploadKind::Text)
    } else {
        None
    }
}

/// Classify an upload under the default per-type caps.
pub fn classify(original_name: &str, bytes: &[u8]) -> Result<UploadClass, UploadReject> {
    classify_with_caps(
        original_name,
        bytes,
        MAX_IMAGE_BYTES,
        MAX_DOC_BYTES,
        MAX_TEXT_BYTES,
    )
}

/// Classify with explicit caps (used by tests).
pub fn classify_with_caps(
    original_name: &str,
    bytes: &[u8],
    image_max: u64,
    doc_max: u64,
    text_max: u64,
) -> Result<UploadClass, UploadReject> {
    if bytes.is_empty() {
        return Err(UploadReject::Empty);
    }

    let claimed = mime::extension_of(original_name);
    let claim = claim_category(&claimed);
    let detected = mime::sniff(bytes);

    let (ext, kind): (String, UploadKind) = match detected.as_ref() {
        Some(s) if s.mime.starts_with("image/") => {
            if let Some(ck) = claim {
                if ck != UploadKind::Image {
                    return Err(UploadReject::ExtensionMismatch {
                        claimed,
                        detected: s.ext.to_string(),
                    });
                }
            }
            (s.ext.to_string(), UploadKind::Image)
        }
        Some(s) if s.ext == "pdf" => {
            if let Some(ck) = claim {
                if ck != UploadKind::Document {
                    return Err(UploadReject::ExtensionMismatch {
                        claimed,
                        detected: s.ext.to_string(),
                    });
                }
            }
            ("pdf".to_string(), UploadKind::Document)
        }
        Some(s) if s.ext == "zip" => {
            // Office formats are ZIP containers; the extension names the subtype.
            if mime::OFFICE_EXTS.contains(&claimed.as_str()) {
                (claimed.clone(), UploadKind::Document)
            } else {
                return Err(UploadReject::UnsupportedContent);
            }
        }
        Some(s) if s.ext == "txt" => {
            // SVG is text but a declared image; accept it as text-backed.
            let svg = claim == Some(UploadKind::Image) && claimed == "svg";
            if let Some(ck) = claim {
                let compatible = ck == UploadKind::Text || svg;
                if !compatible {
                    return Err(UploadReject::ExtensionMismatch {
                        claimed,
                        detected: s.ext.to_string(),
                    });
                }
            }
            if svg {
                ("svg".to_string(), UploadKind::Text)
            } else if mime::is_text_ext(&claimed) {
                (claimed.clone(), UploadKind::Text)
            } else {
                ("txt".to_string(), UploadKind::Text)
            }
        }
        _ => return Err(UploadReject::UnsupportedContent),
    };

    let max = match kind {
        UploadKind::Image => image_max,
        UploadKind::Document => doc_max,
        UploadKind::Text => text_max,
    };
    let actual = bytes.len() as u64;
    if actual > max {
        return Err(UploadReject::TooLarge {
            kind,
            max_bytes: max,
            actual,
        });
    }

    Ok(UploadClass { ext, kind })
}

/// True when `name` is exactly `{attachment_uid}.{ext}` with one extension segment.
pub fn filename_matches(name: &str, attachment_uid: &str) -> bool {
    let Some((stem, ext)) = name.split_once('.') else {
        return false;
    };
    !ext.is_empty() && !ext.contains('.') && !stem.is_empty() && stem == attachment_uid
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_matching_content() {
        let c = classify("logo.png", b"\x89PNG\r\n\x1a\n....").unwrap();
        assert_eq!(c.ext, "png");
        assert_eq!(c.kind, UploadKind::Image);
    }

    #[test]
    fn rejects_empty_and_unknown() {
        assert_eq!(classify("x.txt", b""), Err(UploadReject::Empty));
        assert_eq!(
            classify("x.bin", &[0x00, 0x01, 0x02, 0x03]),
            Err(UploadReject::UnsupportedContent)
        );
    }

    #[test]
    fn rejects_extension_content_mismatch() {
        let err = classify("evil.png", b"%PDF-1.7").unwrap_err();
        assert!(matches!(err, UploadReject::ExtensionMismatch { .. }));
    }

    #[test]
    fn office_zip_uses_claimed_subtype() {
        let c = classify("report.docx", b"PK\x03\x04rest").unwrap();
        assert_eq!(c.ext, "docx");
        assert_eq!(c.kind, UploadKind::Document);
        // A bare zip is not an accepted document.
        assert_eq!(
            classify("archive.zip", b"PK\x03\x04rest"),
            Err(UploadReject::UnsupportedContent)
        );
    }

    #[test]
    fn svg_is_accepted_as_text() {
        let c = classify("icon.svg", b"<svg xmlns=\"...\"></svg>").unwrap();
        assert_eq!(c.ext, "svg");
        assert_eq!(c.kind, UploadKind::Text);
    }

    #[test]
    fn unknown_text_extension_falls_back_to_txt() {
        let c = classify("notes.weird", b"just some text").unwrap();
        assert_eq!(c.ext, "txt");
        assert_eq!(c.kind, UploadKind::Text);
    }

    #[test]
    fn enforces_per_type_caps() {
        let img = classify_with_caps("a.png", b"\x89PNG\r\n\x1a\nxx", 4, 100, 100).unwrap_err();
        assert!(matches!(img, UploadReject::TooLarge { kind: UploadKind::Image, .. }));

        let txt = classify_with_caps("a.txt", b"hello", 100, 100, 2).unwrap_err();
        assert!(matches!(txt, UploadReject::TooLarge { kind: UploadKind::Text, .. }));
    }
}
