//! UI Pages for ThinUI
//!
//! Main views: Chat, History, Settings

pub mod chat;
pub mod history;
pub mod settings;

pub use chat::{chat_page, ChatPageState};
pub use history::history_page;
pub use settings::settings_page;






