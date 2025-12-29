//! UI state modules
//!
//! Extracted state management from the god object `app.rs`.
//! Each module manages a focused area of application state.

pub mod conversation;
pub mod tool_calls;
pub mod attachments;
pub mod context;

pub use conversation::ConversationState;
pub use tool_calls::ToolCallState;
pub use attachments::AttachmentState;
pub use context::ContextState;

