//! UI Widgets for ThinUI
//!
//! Reusable components matching the original desktop app styling.

pub mod error_banner;
pub mod info_banner;
pub mod markdown_viewer;
pub mod message_bubble;
pub mod selectable_text;
pub mod menu_bar;
pub mod page_header;
pub mod tool_call;
pub mod typing_indicator;

pub use error_banner::error_banner;
pub use info_banner::info_banner;
pub use message_bubble::{message_bubble, BubbleContext};
pub use tool_call::{ToolCallWidget, ToolCallStatus, ToolCallMessage};
pub use typing_indicator::typing_indicator;

