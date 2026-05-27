mod agentic;
mod config;
mod embeddings;
mod llm;
mod mcp;
mod prompts;
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

/// Log file directory: `$XDG_STATE_HOME/luna/logs`, falling back to
/// `~/.local/state/luna/logs` per XDG basedir spec.
fn log_dir() -> std::path::PathBuf {
    if let Ok(state_home) = std::env::var("XDG_STATE_HOME") {
        if !state_home.is_empty() {
            return std::path::PathBuf::from(state_home).join("luna").join("logs");
        }
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    std::path::PathBuf::from(home)
        .join(".local")
        .join("state")
        .join("luna")
        .join("logs")
}

/// Holds the non-blocking file appender's worker guard. Must be kept
/// alive for the lifetime of the process or the file layer drops events.
pub struct LoggingGuard {
    _file_guard: tracing_appender::non_blocking::WorkerGuard,
}

pub fn tracing() -> Option<LoggingGuard> {
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;
    use tracing_subscriber::EnvFilter;

    // Default filter:
    // - info everywhere
    // - llm_call at debug (so we get every per-call event by default)
    // - llm.body silenced (raw payloads only when explicitly requested)
    // - hyper_util/h2/reqwest/mcp_stderr clamped down
    // RUST_LOG overrides everything when set.
    let default_directives = "info,llm_call=debug,llm.body=off,hyper_util=info,h2=info,reqwest=info,mcp_stderr=error";
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(default_directives));

    // Stderr layer (pretty, what we used to have).
    let stderr_layer = tracing_subscriber::fmt::layer()
        .with_target(true)
        .with_writer(std::io::stderr);

    // File layer (JSON, daily-rotated) — only enabled if we can create the dir.
    let dir = log_dir();
    let file_layer_result = std::fs::create_dir_all(&dir).map(|_| {
        let file_appender = tracing_appender::rolling::daily(&dir, "luna.log");
        let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
        let layer = tracing_subscriber::fmt::layer()
            .with_target(true)
            .with_thread_ids(false)
            .with_ansi(false)
            .with_writer(non_blocking)
            .json();
        (layer, guard)
    });

    match file_layer_result {
        Ok((file_layer, guard)) => {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(stderr_layer)
                .with(file_layer)
                .init();
            tracing::info!(log_dir = %dir.display(), "File logging enabled");
            Some(LoggingGuard { _file_guard: guard })
        }
        Err(err) => {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(stderr_layer)
                .init();
            tracing::warn!(error = %err, log_dir = %dir.display(), "File logging disabled; stderr only");
            None
        }
    }
}

pub fn main() {
    // Hold this guard for the whole process lifetime — dropping it stops
    // the non-blocking file appender's flush thread.
    let _logging_guard = tracing();

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

    let profile_max_tokens = resolved.preset().max_tokens;

    match services::deep_sleep_service::run_deep_sleep_cycle(
        storage,
        deep_sleep_cfg,
        llm_client,
        embedding_provider,
        profile_max_tokens,
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
