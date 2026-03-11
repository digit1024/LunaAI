//! Service modules for business logic
//!
//! Services are stateless (or use Arc for shared state) and provide
//! reusable business logic that can be used by both desktop UI and server.

pub mod message_converter;
pub mod context_service;
pub mod schedule_service;
pub mod memory_rag;
pub mod deep_sleep_service;

pub use message_converter::MessageConverter;
pub use context_service::ContextService;
pub use schedule_service::{ScheduleService, next_run_from_cron};

