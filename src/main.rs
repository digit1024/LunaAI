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

    #[arg(
        long,
        help = "Run one deep sleep cycle and exit (memory maintenance)"
    )]
    deep_sleep: bool,
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

    if cli.deep_sleep {
        info!("Running manual deep sleep cycle");
        let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
        rt.block_on(run_deep_sleep_manual(cli.config));
        return;
    }

    info!("Launching Luna server");
        if let Err(err) = server::run(server::ServerOptions {
            config_path: cli.config,
        }) {
            tracing::error!(error = %err, "Server failed");
            std::process::exit(1);
    }
}

async fn run_deep_sleep_manual(config_path: Option<PathBuf>) {
    use std::sync::Arc;
    use tokio::sync::Mutex;

    let app_config = if let Some(path) = config_path {
        config::AppConfig::load_from_path(Some(&path)).expect("Failed to load config")
    } else {
        config::AppConfig::load().expect("Failed to load config")
    };

    let deep_sleep_cfg = &app_config.deep_sleep;
    let profile_name = match &deep_sleep_cfg.profile {
        Some(name) => name.clone(),
        None => {
            tracing::error!("No deep_sleep.profile configured in config.toml");
            std::process::exit(1);
        }
    };

    let profile = match app_config.get_profile(&profile_name) {
        Some(p) => p.clone(),
        None => {
            tracing::error!(profile = %profile_name, "Deep sleep profile not found in config");
            std::process::exit(1);
        }
    };

    let llm_client = llm::build_llm_client(&profile);

    // Open storage using same path as the server
    let sqlite_settings = storage::sqlite_storage_simple::SqliteSettings::from(&app_config.server);
    let storage = storage::Storage::new_default_with_settings(sqlite_settings)
        .expect("Failed to open database");
    let storage = Arc::new(Mutex::new(storage));

    match services::deep_sleep_service::run_deep_sleep_cycle(
        storage,
        deep_sleep_cfg,
        llm_client,
    )
    .await
    {
        Ok(()) => info!("Deep sleep cycle completed successfully"),
        Err(e) => {
            tracing::error!(error = %e, "Deep sleep cycle failed");
            std::process::exit(1);
        }
    }
}
