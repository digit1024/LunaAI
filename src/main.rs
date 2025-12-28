mod agentic;
mod config;
mod dbus;
mod llm;
mod mcp;
mod prompts;
mod resources;
mod server;
mod storage;
mod ui;

use clap::Parser;
use std::path::PathBuf;
use tracing::info;
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
#[command(author, version, about = "Luna AI Desktop + Server")]
struct Cli {
    #[arg(
        long,
        help = "Start the Luna WebSocket server instead of the desktop UI"
    )]
    server: bool,
    #[arg(
        long,
        value_name = "PATH",
        help = "Override the configuration file path"
    )]
    config: Option<PathBuf>,
}

pub fn tracing() {
    let mut filter = EnvFilter::from_default_env();
    
    // Add directives, ignoring parse errors (they're just noise filters)
    if let Ok(directive) = "wgpu_core=error".parse() {
        filter = filter.add_directive(directive);
    }
    if let Ok(directive) = "naga=error".parse() {
        filter = filter.add_directive(directive);
    }
    if let Ok(directive) = "cosmic_text=error".parse() {
        filter = filter.add_directive(directive);
    }
    if let Ok(directive) = "sctk=error".parse() {
        filter = filter.add_directive(directive);
    }
    if let Ok(directive) = "wgpu_hal=error".parse() {
        filter = filter.add_directive(directive);
    }
    if let Ok(directive) = "iced_wgpu=error".parse() {
        filter = filter.add_directive(directive);
    }

    tracing_subscriber::fmt().with_env_filter(filter).init();
}

pub fn main() -> cosmic::iced::Result {
    // Initialize logging
    tracing();

    let cli = Cli::parse();

    if cli.server {
        info!("Launching Luna server mode");
        if let Err(err) = server::run(server::ServerOptions {
            config_path: cli.config,
        }) {
            eprintln!("Server failed: {err:#}");
            std::process::exit(1);
        }
        Ok(())
    } else {
        info!("Starting cosmic_llm UI");
        // Run the cosmic application
        cosmic::app::run::<ui::CosmicLlmApp>(ui::settings(), ui::flags())
    }
}
