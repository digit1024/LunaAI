mod agentic;
mod config;
mod llm;
mod mcp;
mod prompts;
mod server;
mod services;
mod storage;
mod types;

use clap::Parser;
use std::path::PathBuf;
use tracing::info;

#[derive(Debug, Parser)]
#[command(author, version, about = "Luna AI Server")]
struct Cli {
    #[arg(
        long,
        value_name = "PATH",
        help = "Override the configuration file path"
    )]
    config: Option<PathBuf>,
}

pub fn tracing() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
}

pub fn main() {
    // Initialize logging
    tracing();

    let cli = Cli::parse();

    info!("Launching Luna server");
        if let Err(err) = server::run(server::ServerOptions {
            config_path: cli.config,
        }) {
            tracing::error!(error = %err, "Server failed");
            std::process::exit(1);
    }
}
