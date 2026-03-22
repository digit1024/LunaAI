//! Central limits for attachment handling (inline vs RAG, image size).

/// Max image file size accepted for upload / inline (bytes).
pub const MAX_IMAGE_BYTES: u64 = 20 * 1024 * 1024;

/// Max extracted text or document markdown inlined in one attachment (chars).
pub const MAX_INLINE_TEXT_CHARS: usize = 500_000;
