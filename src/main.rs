
mod config;
mod embeddings;
mod llm;
mod mcp;
mod prompts;
mod rig_core;
mod server;
mod services;
mod storage;
mod tools_policy;
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

    #[arg(
        long,
        help = "Reorganize memory vectors: delete all memory_vec rows, re-embed all memories, rebuild index. Requires [embedding] enabled."
    )]
    reorganize_memories: bool,
}

pub fn tracing() {
    use tracing_subscriber::EnvFilter;
    let base = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));
    // Suppress noisy: hyper_util, mcp_stderr. Enable Rig spans for traceability (rig=info; use rig=trace for request/response logs).
    let filter = base
        .add_directive("hyper_util=info".parse().unwrap())
        .add_directive("mcp_stderr=error".parse().unwrap());

    tracing_subscriber::fmt()
        .with_env_filter(filter)
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

    if cli.reorganize_memories {
        info!("Reorganizing memory vectors");
        let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
        rt.block_on(run_reorganize_memories(cli.config));
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

    let resolved = match app_config.resolve_profile(&profile_name) {
        Some(r) => r,
        None => {
            tracing::error!(profile = %profile_name, "Deep sleep profile or preset not found in config");
            std::process::exit(1);
        }
    };

    let llm_client = llm::build_llm_client(resolved.preset());

    // Open storage using same path as the server
    let mut sqlite_settings = storage::sqlite_storage_simple::SqliteSettings::from(&app_config.server);
    if app_config.embedding.is_active() {
        sqlite_settings.embedding_dimension = Some(app_config.embedding.dimensions);
    }
    let storage = storage::Storage::new_default_with_settings(sqlite_settings)
        .expect("Failed to open database");
    let storage = Arc::new(Mutex::new(storage));

    let embedding_provider = if app_config.embedding.is_active() {
        embeddings::OpenAiEmbeddingProvider::from_config(&app_config.embedding).ok()
    } else {
        None
    };

    match services::deep_sleep_service::run_deep_sleep_cycle(
        storage,
        deep_sleep_cfg,
        llm_client,
        embedding_provider,
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

async fn run_reorganize_memories(config_path: Option<PathBuf>) {
    use std::sync::Arc;
    use tokio::sync::Mutex;

    let app_config = if let Some(path) = config_path {
        config::AppConfig::load_from_path(Some(&path)).expect("Failed to load config")
    } else {
        config::AppConfig::load().expect("Failed to load config")
    };

    if !app_config.embedding.is_active() {
        tracing::error!(
            "Reorganize requires [embedding] enabled in config.toml. Add [embedding] section with enabled=true, endpoint, model, dimensions."
        );
        std::process::exit(1);
    }

    let embedding_provider = match embeddings::OpenAiEmbeddingProvider::from_config(&app_config.embedding) {
        Ok(p) => p,
        Err(e) => {
            tracing::error!(error = %e, "Failed to create embedding provider");
            std::process::exit(1);
        }
    };

    let mut sqlite_settings = storage::sqlite_storage_simple::SqliteSettings::from(&app_config.server);
    sqlite_settings.embedding_dimension = Some(app_config.embedding.dimensions);
    let storage = storage::Storage::new_default_with_settings(sqlite_settings)
        .expect("Failed to open database");
    let storage = Arc::new(Mutex::new(storage));

    match services::deep_sleep_service::reorganize_memory_vectors(storage, embedding_provider).await {
        Ok(()) => info!("Memory vector reorganize completed successfully"),
        Err(e) => {
            tracing::error!(error = %e, "Reorganize failed");
            std::process::exit(1);
        }
    }
}
