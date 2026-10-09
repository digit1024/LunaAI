//! Central limits for attachment handling (inline vs RAG, image size).

/// Max image file size accepted for upload / inline (bytes).
pub const MAX_IMAGE_BYTES: u64 = 20 * 1024 * 1024;

/// Max document file size accepted for upload / processing (bytes).
pub const MAX_DOC_BYTES: u64 = 100 * 1024 * 1024;

/// Max plain-text file size accepted for upload / processing (bytes).
pub const MAX_TEXT_BYTES: u64 = 10 * 1024 * 1024;

/// Hard ceiling on the multipart upload body (bytes); the outer bound that
/// per-type caps sit under.
pub const MAX_UPLOAD_BYTES: usize = 50 * 1024 * 1024;
