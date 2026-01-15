//! UI Pages for ThinUI
//!
//! Main views: Chat, History, MCP Servers, Settings

pub mod chat;
pub mod history;
pub mod mcp_servers;
pub mod settings;

pub use chat::{chat_page, ChatPageState};
pub use history::history_page;
pub use mcp_servers::mcp_servers_page;
pub use settings::settings_page;













