//! Message handlers module
//!
//! Extracted handlers for different message categories to reduce app.rs size.

pub mod chat;
pub mod tools;
pub mod navigation;
pub mod agent;
pub mod settings;
pub mod dbus;
pub mod dialog;
pub mod mcp;

pub use chat::handle_chat_messages;
pub use tools::handle_tool_messages;
pub use navigation::handle_navigation_messages;
pub use agent::handle_agent_messages;
pub use settings::handle_settings_messages;
pub use dbus::handle_dbus_messages;
pub use dialog::handle_dialog_messages;
pub use mcp::handle_mcp_messages;

