pub mod dto;
mod handlers;
mod websocket;

use self::handlers::ServerContext;
use crate::{
    config::{AppConfig, MCPConfig},
    prompts::PromptManager,
    storage::{sqlite_storage_simple::SqliteSettings, Storage},
};
use anyhow::{Context, Result};
use std::{path::PathBuf, sync::Arc};
use tokio::sync::{Mutex, RwLock};

pub struct ServerOptions {
    pub config_path: Option<PathBuf>,
}

pub fn run(options: ServerOptions) -> Result<()> {
    let runtime = tokio::runtime::Runtime::new().context("failed to create Tokio runtime")?;
    runtime.block_on(async move { launch(options).await })
}

async fn launch(options: ServerOptions) -> Result<()> {
    let raw_config = load_config(options.config_path.as_ref()).unwrap_or_else(|err| {
        tracing::warn!("Failed to load config: {}. Falling back to defaults.", err);
        AppConfig::default()
    });
    let config = Arc::new(raw_config);
    let prompt_manager = PromptManager::load_from_config(&config.prompts).unwrap_or_else(|err| {
        tracing::warn!("Failed to load prompts: {}", err);
        PromptManager::load_from_config(&crate::prompts::PromptConfig::default())
            .expect("Prompt defaults must load")
    });
    let sqlite_settings = SqliteSettings::from(&config.server);
    let storage =
        Storage::new_default_with_settings(sqlite_settings.clone()).unwrap_or_else(|err| {
            tracing::error!("SQLite init failed: {}. Using temp db.", err);
            Storage::new_with_settings(
                std::env::temp_dir().join("cosmic_llm_server.db"),
                sqlite_settings,
            )
            .expect("temporary sqlite init must succeed")
        });

    let storage = Arc::new(Mutex::new(storage));
    let mcp_registry = Arc::new(RwLock::new(crate::mcp::MCPServerRegistry::new()));
    let mcp_config = load_mcp_config(&config);
    initialize_mcp_registry(&mcp_registry, &mcp_config, &config).await;

    let ctx = Arc::new(ServerContext {
        config: config.clone(),
        server_cfg: Arc::new(config.server.clone()),
        prompt_manager,
        storage,
        mcp_registry,
    });

    websocket::serve(ctx).await
}

fn load_config(path: Option<&PathBuf>) -> Result<AppConfig, config::ConfigError> {
    if let Some(custom) = path {
        AppConfig::load_from_path(Some(custom))
    } else {
        AppConfig::load()
    }
}

fn load_mcp_config(config: &AppConfig) -> MCPConfig {
    MCPConfig::load_from_json().unwrap_or_else(|err| {
        tracing::warn!("Failed to load external MCP config: {}", err);
        config.mcp.clone()
    })
}

async fn initialize_mcp_registry(
    registry: &Arc<RwLock<crate::mcp::MCPServerRegistry>>,
    config: &MCPConfig,
    app_config: &AppConfig,
) {
    let default_tools = app_config
        .get_default_profile()
        .map(|profile| profile.enabled_mcp.clone())
        .unwrap_or_default();

    let mut guard = registry.write().await;
    if let Err(err) = guard.initialize_from_config(config).await {
        tracing::warn!("MCP registry init failed: {}", err);
    } else if !default_tools.is_empty() {
        guard.apply_profile_tool_defaults(&default_tools);
    }
}
