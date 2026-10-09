//! Context compaction: boundary-based summarization that replaces
//! importance-scored message slicing.
//!
//! The summary of earlier turns is carried as a normal `assistant` message
//! (never `system`): providers keep only one system slot (Anthropic discards
//! every system message after the first), and a `system` summary would also be
//! hoisted to the top by context selection. The stable [`SUMMARY_MARKER`]
//! header identifies it wherever the raw role is ambiguous.

pub mod boundaries;
pub mod engine;
pub mod stubs;

/// Stable header prepended to any compaction summary. Code keys on this rather
/// than the (provider-visible) role, so a summary stays recognizable after
/// storage, reload, and re-conversion.
pub const SUMMARY_MARKER: &str = "[CONTEXT COMPACTION — earlier turns summarized]";

/// True when `content` is a compaction summary (marker-prefixed).
pub fn is_summary_content(content: &str) -> bool {
    content.trim_start().starts_with(SUMMARY_MARKER)
}
