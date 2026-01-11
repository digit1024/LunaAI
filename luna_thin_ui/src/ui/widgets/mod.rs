//! UI Widgets for ThinUI
//!
//! Reusable components matching the original desktop app styling.

pub mod error_banner;
pub mod message_bubble;
pub mod menu_bar;
pub mod tool_call;
pub mod typing_indicator;

pub use error_banner::error_banner;
pub use message_bubble::{message_bubble, BubbleContext};
pub use tool_call::{ToolCallWidget, ToolCallStatus, ToolCallMessage};
pub use typing_indicator::typing_indicator;

