mod ui;
mod llm;
mod mcp;
mod storage;
mod config;
mod agentic;
mod prompts;
use tracing_subscriber::EnvFilter;

use tracing::info;


pub fn tracing() {
    let filter = EnvFilter::from_default_env()
    .add_directive("wgpu_core=error".parse().unwrap())
    .add_directive("naga=error".parse().unwrap())
    .add_directive("cosmic_text=error".parse().unwrap())
    .add_directive("sctk=error".parse().unwrap())
    .add_directive("wgpu_hal=error".parse().unwrap())
.add_directive("iced_wgpu=error".parse().unwrap());

tracing_subscriber::fmt()
.with_env_filter(filter)
.init();
}

pub fn main() -> cosmic::iced::Result {
    // Initialize logging
    tracing();

    info!("🚀 Starting cosmic_llm...");
    
    // Run the cosmic application
    cosmic::app::run::<ui::CosmicLlmApp>(ui::settings(), ui::flags())
}