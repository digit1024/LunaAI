//! Central limits for attachment handling (inline vs RAG, image size).

/// Max image file size accepted for upload / inline (bytes).
pub const MAX_IMAGE_BYTES: u64 = 20 * 1024 * 1024;

/// Max document file size accepted for upload / processing (bytes).
pub const MAX_DOC_BYTES: u64 = 100 * 1024 * 1024;

/// Max plain-text file size accepted for upload / processing (bytes).
pub const MAX_TEXT_BYTES: u64 = 10 * 1024 * 1024;

/// Hard ceiling on the multipart upload body (bytes); the outer bound that
/// per-type caps sit under. Must be >= the largest per-type cap
/// (`MAX_DOC_BYTES`) plus multipart framing overhead, otherwise that cap is
/// unreachable and larger documents are rejected with a confusing 413 at the
/// transport layer instead of the per-type policy check.
pub const MAX_UPLOAD_BYTES: usize = MAX_DOC_BYTES as usize + 1024 * 1024;
