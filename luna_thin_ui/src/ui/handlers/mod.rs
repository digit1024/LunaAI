//! Message handlers module
//!
//! Extracted handlers for different message categories to reduce app.rs size.

pub mod connection;
pub mod chat;
pub mod history_memories;
pub mod navigation;
pub mod settings;
pub mod server_events;
pub mod tts;

pub use connection::handle_connection_messages;
pub use chat::handle_chat_messages;
pub use history_memories::handle_history_memories_messages;
pub use navigation::handle_navigation_messages;
pub use settings::handle_settings_messages;
pub use server_events::handle_server_event_messages;
pub use tts::handle_tts_messages;


