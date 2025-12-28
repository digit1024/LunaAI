//! Service modules for business logic
//!
//! Services are stateless (or use Arc for shared state) and provide
//! reusable business logic that can be used by both desktop UI and server.

pub mod message_converter;
pub mod context_service;
pub mod tool_call_manager;

pub use message_converter::MessageConverter;
pub use context_service::ContextService;
pub use tool_call_manager::ToolCallManager;

