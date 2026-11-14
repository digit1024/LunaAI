pub mod conversation_subscriptions;
pub mod context_pipeline;
pub mod dto;
mod handlers;
mod http;
mod upload_policy;
mod websocket;

use crate::server::handlers::{run_scheduled_task, ServerContext};
use crate::services::ScheduleService;
use crate::storage::ScheduledJob;
use crate::{
    config::AppConfig,
    prompts::PromptManager,
    storage::{sqlite_storage_simple::SqliteSettings, Storage},
};
use agentic_loop::mcp_servers_registry::MCPServerRegistry;
use anyhow::{Context, Result};
use chrono::Utc;
use std::{collections::HashMap, path::PathBuf, sync::Arc};
use tokio::sync::{Mutex, RwLock};
use uuid::Uuid;

pub struct ServerOptions {
    pub config_path: Option<PathBuf>,
}

pub fn run(options: ServerOptions) -> Result<()> {
    let runtime = tokio::runtime::Runtime::new().context("failed to create Tokio runtime")?;
    runtime.block_on(async move { launch(options).await })
}

async fn launch(options: ServerOptions) -> Result<()> {
    let config = Arc::new(load_config_or_default(options.config_path.as_ref()));
    config
        .server
        .validate_security()
        .map_err(|msg| anyhow::anyhow!(msg))?;
    warn_if_default_profile_unresolved(&config);

    let prompt_manager = load_prompt_manager(&config);
    let (sqlite_settings, embedding_provider) = build_sqlite_and_embedding_settings(&config);
    let storage = init_storage(sqlite_settings).await?;
    let llm_observer = init_llm_audit_writer(storage.clone());
    let mcp_registry = Arc::new(RwLock::new(MCPServerRegistry::new()));
    let mcp_config = load_mcp_config(&config);
    let default_allowed_tool_names =
        initialize_mcp_registry(&mcp_registry, &mcp_config, &config).await;

    let ctx = build_server_context(
        config,
        prompt_manager,
        storage,
        mcp_registry,
        default_allowed_tool_names,
        embedding_provider,
        llm_observer,
    )?;

    spawn_background_workers(ctx.clone());

    let bind_addr = format!("{}:{}", ctx.config.server.host, ctx.config.server.port);
    if ctx.config.server.host.trim() == "0.0.0.0" || ctx.config.server.host.trim() == "::" {
        tracing::warn!(
            host = %ctx.config.server.host,
            "Server is binding on all interfaces; ensure api_key is strong and network exposure is intentional"
        );
    }
    let listener = tokio::net::TcpListener::bind(&bind_addr)
        .await
        .context("failed to bind server")?;
    tracing::info!(address = %bind_addr, "Luna server listening (HTTP + WebSocket on /ws)");

    let app = http::create_http_router(ctx.clone());
    // Serve with connect info so the auth middleware can rate-limit failures
    // per client IP (brute-force slowdown on exposed interfaces).
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await
    .context("server error")?;

    // Sockets have drained; abort any in-flight agent runs so nothing keeps
    // writing to storage after shutdown (they would otherwise be killed
    // mid-write when the runtime drops, leaving incomplete tool tails).
    {
        let mut runs = ctx.active_agent_runs.write().await;
        for (conv_id, abort) in runs.drain() {
            tracing::info!(conversation_id = %conv_id, "Aborting active agent run for shutdown");
            abort.abort();
        }
    }
    tracing::info!("Luna server stopped");
    Ok(())
}

/// Resolves when the process is asked to stop: Ctrl-C (SIGINT) or SIGTERM
/// (systemd / containers). On non-Unix platforms only Ctrl-C is available.
async fn shutdown_signal() {
    let ctrl_c = async {
        match tokio::signal::ctrl_c().await {
            Ok(()) => {}
            Err(e) => {
                tracing::warn!(error = %e, "Failed to install Ctrl-C handler");
                std::future::pending::<()>().await;
            }
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut sig) => {
                let _ = sig.recv().await;
            }
            Err(e) => {
                tracing::warn!(error = %e, "Failed to install SIGTERM handler");
                std::future::pending::<()>().await;
            }
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    tracing::info!("Shutdown signal received; draining in-flight requests...");
}

fn load_config_or_default(path: Option<&PathBuf>) -> AppConfig {
    load_config(path).unwrap_or_else(|err| {
        tracing::warn!("Failed to load config: {}. Falling back to defaults.", err);
        AppConfig::default()
    })
}

fn warn_if_default_profile_unresolved(config: &AppConfig) {
    if config.resolve_default_profile().is_none() {
        tracing::warn!(
            default = %config.default,
            "Default profile does not resolve (missing profile or model_preset). WebSocket connections will fail until config is fixed. See docs/sample_config.toml."
        );
    }
}

fn load_prompt_manager(config: &AppConfig) -> PromptManager {
    PromptManager::load_from_config(&config.prompts).unwrap_or_else(|err| {
        tracing::warn!("Failed to load prompts: {}", err);
        PromptManager::load_from_config(&crate::prompts::PromptConfig::default())
            .unwrap_or_else(|e| {
                tracing::error!(error = %e, "Failed to load default prompt config, using empty PromptManager");
                PromptManager {
                    system_prompt: None,
                }
            })
    })
}

fn build_sqlite_and_embedding_settings(
    config: &AppConfig,
) -> (SqliteSettings, Option<Arc<dyn crate::embeddings::EmbeddingProvider>>) {
    let mut sqlite_settings = SqliteSettings::from(&config.server);
    let embedding_provider = if config.embedding.is_active() {
        sqlite_settings.embedding_dimension = Some(config.embedding.dimensions);
        tracing::info!(
            dimensions = config.embedding.dimensions,
            "Embedding enabled for memory vector search"
        );
        match crate::embeddings::OpenAiEmbeddingProvider::from_config(&config.embedding) {
            Ok(p) => Some(p),
            Err(e) => {
                tracing::warn!(error = %e, "Failed to create embedding provider; memory recall disabled");
                sqlite_settings.embedding_dimension = None;
                None
            }
        }
    } else {
        None
    };
    (sqlite_settings, embedding_provider)
}

async fn init_storage(sqlite_settings: SqliteSettings) -> Result<Arc<Mutex<Storage>>> {
    // Fail fast. Silently falling back to a predictable temp-dir DB lets another
    // local user pre-create or symlink that path (data leak / corruption) and
    // masks real failures (e.g. disk full, bad permissions) as "lost history".
    // Surface the error and refuse to start instead.
    let storage = Storage::new_default_with_settings(sqlite_settings).map_err(|err| {
        anyhow::anyhow!(
            "Failed to open the SQLite database: {err}. Check the data directory \
             permissions and available disk space. Refusing to fall back to a \
             temp-dir database."
        )
    })?;

    Ok(Arc::new(Mutex::new(storage)))
}

fn init_llm_audit_writer(
    storage: Arc<Mutex<Storage>>,
) -> Arc<dyn crate::llm::LlmObserver> {
    let (tx, mut rx) =
        tokio::sync::mpsc::channel::<crate::llm::LlmCallRecord>(LLM_AUDIT_CHANNEL_CAP);
    let observer: Arc<dyn crate::llm::LlmObserver> =
        Arc::new(LlmCallAuditObserver { tx });

    tokio::spawn(async move {
        while let Some(record) = rx.recv().await {
            let storage_guard = storage.lock().await;
            if let Err(e) = storage_guard.insert_llm_call(&record) {
                tracing::warn!(
                    error = %e,
                    call_id = %record.call_id,
                    "Failed to persist llm_call audit row"
                );
            }
        }
        tracing::debug!("LLM audit writer channel closed; exiting");
    });

    observer
}

fn build_server_context(
    config: Arc<AppConfig>,
    prompt_manager: PromptManager,
    storage: Arc<Mutex<Storage>>,
    mcp_registry: Arc<RwLock<MCPServerRegistry>>,
    default_allowed_tool_names: std::collections::HashSet<String>,
    embedding_provider: Option<Arc<dyn crate::embeddings::EmbeddingProvider>>,
    llm_observer: Arc<dyn crate::llm::LlmObserver>,
) -> Result<Arc<ServerContext>> {
    crate::llm::set_llm_observer(llm_observer.clone());

    Ok(Arc::new(ServerContext {
        config: config.clone(),
        server_cfg: Arc::new(config.server.clone()),
        static_token: Uuid::new_v4().to_string(),
        auth_throttle: Arc::new(http::AuthThrottle::new(
            config.server.auth_rate_limit_max,
            config.server.auth_rate_limit_window_secs,
        )),
        prompt_manager,
        storage: storage.clone(),
        mcp_registry,
        subscriptions: Arc::new(conversation_subscriptions::ConversationSubscriptions::new()),
        schedule_service: Arc::new(ScheduleService::new(storage)),
        default_allowed_tool_names,
        embedding_provider,
        active_agent_runs: Arc::new(RwLock::new(HashMap::new())),
    }))
}

fn spawn_background_workers(ctx: Arc<ServerContext>) {
    if ctx.config.title_summary.title_generation_profile.is_some() {
        spawn_title_generation_thread(ctx.config.clone(), ctx.storage.clone());
    }
    spawn_scheduler_loop(ctx.clone());
    if ctx.config.deep_sleep.enabled {
        if ctx.config.deep_sleep.profile.is_some() {
            tracing::info!(
                profile = %ctx.config.deep_sleep.profile.as_deref().unwrap_or(""),
                interval_hours = ctx.config.deep_sleep.interval_hours,
                max_conversations = ctx.config.deep_sleep.max_conversations_per_run,
                "Deep Sleep: enabled, first check in {}s",
                DEEP_SLEEP_POLL_SECS
            );
            spawn_deep_sleep_loop(ctx);
        } else {
            tracing::warn!("Deep sleep is enabled but no profile configured -- skipping");
        }
    } else {
        tracing::info!("Deep Sleep: disabled (set [deep_sleep] enabled=true in config)");
    }
}

fn load_config(path: Option<&PathBuf>) -> Result<AppConfig, config::ConfigError> {
    if let Some(custom) = path {
        AppConfig::load_from_path(Some(custom))
    } else {
        AppConfig::load()
    }
}

fn load_mcp_config(config: &AppConfig) -> crate::config::MCPConfig {
    crate::config::MCPConfig::load_from_json().unwrap_or_else(|err| {
        tracing::warn!("Failed to load external MCP config: {}", err);
        config.mcp.clone()
    })
}

/// Convert main app MCPConfig to agentic-loop MCPConfig
fn convert_mcp_config(config: &crate::config::MCPConfig) -> agentic_loop::mcp_config::MCPConfig {
    // Since both configs have identical structure, convert via JSON
    let json = serde_json::to_value(config).unwrap_or_default();
    serde_json::from_value(json).unwrap_or_else(|_| agentic_loop::mcp_config::MCPConfig::new())
}

/// Initialize MCP registry from config, then apply default profile's tools policy.
/// Returns the allowed tool names set for the default profile (for SessionState).
async fn initialize_mcp_registry(
    registry: &Arc<RwLock<MCPServerRegistry>>,
    config: &crate::config::MCPConfig,
    app_config: &AppConfig,
) -> std::collections::HashSet<String> {
    // Convert to agentic-loop config type and connect servers
    let agentic_config = convert_mcp_config(config);
    {
        let mut guard = registry.write().await;
        if let Err(err) = guard.initialize_from_config(&agentic_config).await {
            tracing::warn!("MCP registry init failed: {}", err);
        }
    }

    // Compute default profile's tools policy. This is just a snapshot of the allow-set for
    // the initial SessionState; nothing is mutated on the registry. Per-session profile changes
    // recompute their own allow-set independently — sessions can no longer trample each other.
    let default_allowed = match app_config.resolve_default_profile() {
        Some(resolved) => {
            match crate::tools_policy::compute_allowed_tool_names(registry, app_config, resolved.profile()).await {
                Ok(set) => set,
                Err(e) => {
                    tracing::warn!("Tools policy compute failed: {}; no tools allowed", e);
                    std::collections::HashSet::new()
                }
            }
        }
        None => std::collections::HashSet::new(),
    };

    // Log all connected servers and their tools
    {
        let guard = registry.read().await;
        let connected_count = guard.servers.len();
        let failed_count = guard.failed_servers.len();
        if connected_count > 0 {
            tracing::info!(count = connected_count, "Connected MCP servers:");
            for (server_name, server_connection) in guard.servers.iter() {
                let connection_guard = server_connection.read().await;
                let tools = connection_guard.tools();
                let tool_names: Vec<String> = tools.iter().map(|tool| tool.name.clone()).collect();
                let enabled_count = tool_names
                    .iter()
                    .filter(|name| default_allowed.contains(name.as_str()))
                    .count();
                drop(connection_guard);
                tracing::info!(
                    server = %server_name,
                    tool_count = tool_names.len(),
                    enabled_count = enabled_count,
                    tools = %tool_names.join(", ")
                );
            }
        } else {
            tracing::warn!("No MCP servers connected");
        }
        if failed_count > 0 {
            tracing::error!(count = failed_count, "Failed to connect to MCP servers:");
            for (server_name, error_msg) in guard.failed_servers.iter() {
                tracing::error!(
                    server = %server_name,
                    error = %error_msg,
                    "Failed to connect to MCP server"
                );
            }
        }
        tracing::info!(
            connected = connected_count,
            failed = failed_count,
            total = connected_count + failed_count,
            "MCP server initialization complete"
        );
    }
    default_allowed
}

const SCHEDULER_INTERVAL_SECS: u64 = 45;
const SCHEDULER_BATCH_LIMIT: u32 = 10;

fn spawn_scheduler_loop(ctx: Arc<ServerContext>) {
    tokio::spawn(async move {
        // Recover jobs left in 'running' by a previous crash/restart before
        // scheduling anything new, so they don't stay stuck forever.
        {
            let storage = ctx.storage.lock().await;
            match storage.recover_stuck_running_jobs(Utc::now().timestamp()) {
                Ok(0) => {}
                Ok(n) => tracing::warn!(
                    count = n,
                    "Scheduler: recovered jobs stuck in 'running' after restart"
                ),
                Err(e) => tracing::warn!(error = %e, "Scheduler: failed to recover stuck jobs"),
            }
        }

        let mut interval = tokio::time::interval(std::time::Duration::from_secs(SCHEDULER_INTERVAL_SECS));
        interval.tick().await;
        loop {
            interval.tick().await;
            let now = Utc::now().timestamp();
            let jobs: Vec<ScheduledJob> = {
                let storage = ctx.storage.lock().await;
                match storage.get_due_scheduled_jobs(now, SCHEDULER_BATCH_LIMIT) {
                    Ok(j) => j,
                    Err(e) => {
                        tracing::warn!("Scheduler: failed to get due jobs: {}", e);
                        continue;
                    }
                }
            };
            for job in jobs {
                let taken = {
                    let storage = ctx.storage.lock().await;
                    match storage.set_scheduled_job_running(&job.id, now) {
                        Ok(t) => t,
                        Err(e) => {
                            tracing::warn!(job_id = %job.id, "Scheduler: failed to mark running: {}", e);
                            continue;
                        }
                    }
                };
                if !taken {
                    continue;
                }
                let job_id = job.id.clone();
                // Run each job in its own task so a panic inside a job cannot
                // kill the scheduler loop or leave the row stuck in 'running'.
                let job_ctx = ctx.clone();
                let handle = tokio::spawn(async move { run_scheduled_task(job_ctx, job).await });
                match handle.await {
                    Ok(Ok(())) => {}
                    Ok(Err(e)) => {
                        tracing::warn!("Scheduler: run_scheduled_task failed: {}", e);
                        let storage = ctx.storage.lock().await;
                        let _ = storage.set_scheduled_job_completed(
                            &job_id,
                            Utc::now().timestamp(),
                            true,
                            Some(&e.to_string()),
                        );
                    }
                    Err(join_err) => {
                        tracing::error!(
                            job_id = %job_id,
                            error = %join_err,
                            "Scheduler: job task panicked; marking job failed"
                        );
                        let storage = ctx.storage.lock().await;
                        let _ = storage.set_scheduled_job_completed(
                            &job_id,
                            Utc::now().timestamp(),
                            true,
                            Some("Job task panicked"),
                        );
                    }
                }
            }
        }
    });
}

/// Deep sleep background loop: checks every 5 minutes if a cycle is due.
const DEEP_SLEEP_POLL_SECS: u64 = 300; // 5 minutes

fn spawn_deep_sleep_loop(ctx: Arc<ServerContext>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(DEEP_SLEEP_POLL_SECS));
        interval.tick().await; // skip immediate first tick
        loop {
            interval.tick().await;

            let deep_sleep_cfg = &ctx.config.deep_sleep;

            // Check if due
            let is_due = {
                let guard = ctx.storage.lock().await;
                crate::services::deep_sleep_service::is_due(&guard, deep_sleep_cfg.interval_hours)
            };

            if !is_due {
                continue;
            }

            tracing::info!("Deep Sleep: cycle is due, starting...");

            // Build LLM client from the configured profile
            let profile_name = match &deep_sleep_cfg.profile {
                Some(name) => name.clone(),
                None => {
                    tracing::warn!("Deep Sleep: no profile configured, skipping");
                    continue;
                }
            };

            let resolved = match ctx.config.resolve_profile(&profile_name) {
                Some(r) => r,
                None => {
                    tracing::warn!(
                        profile = %profile_name,
                        "Deep Sleep: profile or preset not found, skipping"
                    );
                    continue;
                }
            };

            let profile_max_tokens = resolved.preset().max_tokens;
            let llm_client = crate::llm::build_llm_client(resolved.preset());

            if let Err(e) = crate::services::deep_sleep_service::run_deep_sleep_cycle(
                ctx.storage.clone(),
                deep_sleep_cfg,
                llm_client,
                ctx.embedding_provider.clone(),
                profile_max_tokens,
            )
            .await
            {
                tracing::error!(error = %e, "Deep Sleep: cycle failed");
            }
        }
    });
}

/// Background writer for the LLM call audit log.
///
/// `LlmCallSpan` finishes are synchronous (they fire from `Drop` too),
/// so we cannot acquire a `tokio::Mutex` lock from inside `record()`.
/// Instead, every record is pushed to a bounded channel and a single
/// background task drains it into SQLite. The channel is bounded so a stalled
/// writer cannot grow memory without limit; under sustained backpressure the
/// newest audit records are dropped with a warning (audit data is best-effort —
/// the LLM hot path must never depend on it).
struct LlmCallAuditObserver {
    tx: tokio::sync::mpsc::Sender<crate::llm::LlmCallRecord>,
}

/// Capacity of the bounded LLM-audit channel.
const LLM_AUDIT_CHANNEL_CAP: usize = 8192;

impl crate::llm::LlmObserver for LlmCallAuditObserver {
    fn record(&self, record: crate::llm::LlmCallRecord) {
        use tokio::sync::mpsc::error::TrySendError;
        match self.tx.try_send(record) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => {
                tracing::warn!("LLM audit queue full; dropping audit record (best-effort log)");
            }
            Err(TrySendError::Closed(_)) => {
                tracing::warn!("LLM audit log receiver dropped; record lost");
            }
        }
    }
}

fn spawn_title_generation_thread(
    config: Arc<AppConfig>,
    storage: Arc<Mutex<Storage>>,
) {
    tokio::spawn(async move {
        let title_config = &config.title_summary;
        let sleep_duration = std::time::Duration::from_secs(title_config.summary_loop_sleep_seconds);

        loop {
            tokio::time::sleep(sleep_duration).await;

            // Get conversations without titles
            let conversation_ids = {
                let storage_guard = storage.lock().await;
                match storage_guard.get_conversations_without_title() {
                    Ok(ids) => ids,
                    Err(e) => {
                        tracing::warn!("Failed to get conversations without titles: {}", e);
                        continue;
                    }
                }
            };

            if conversation_ids.is_empty() {
                continue;
            }

            // Get the profile to use for title generation
            // This should always be Some since we only start the thread if profile is configured
            let profile_name = match &title_config.title_generation_profile {
                Some(name) => name,
                None => {
                    tracing::warn!("Title generation profile not configured, stopping thread");
                    break;
                }
            };

            let resolved = match config.resolve_profile(profile_name) {
                Some(r) => r.clone(),
                None => {
                    tracing::warn!(
                        profile_name = %profile_name,
                        "Title generation profile or preset not found, stopping thread"
                    );
                    break;
                }
            };

                // Generate title for each conversation
            for conversation_id in conversation_ids {
                let conversation_id_str = conversation_id.to_string();
                let preset = resolved.preset().clone();
                let summary_chars = title_config.summary_chars;
                let system_prompt = title_config.title_generation_system_prompt.clone();
                
                // Generate title using Storage wrapper method
                // Load messages first, then release lock before async LLM call
                let title_result = {
                    let messages = {
                        let storage_guard = storage.lock().await;
                        storage_guard.load_conversation_messages(&conversation_id_str)
                    };
                    
                    let messages = match messages {
                        Ok(msgs) => msgs,
                        Err(e) => {
                            tracing::warn!("Failed to load messages for conversation {}: {}", conversation_id, e);
                            continue;
                        }
                    };
                    
                    // Skip if there are no messages
                    if messages.is_empty() {
                        tracing::debug!("Skipping title generation for conversation {}: no messages", conversation_id);
                        continue;
                    }
                    
                    // Check if last message is older than 1 minute
                    if let Some(last_message) = messages.last() {
                        let last_message_time = std::time::UNIX_EPOCH + std::time::Duration::from_secs(last_message.created_at as u64);
                        let now = std::time::SystemTime::now();
                        
                        if let Ok(duration_since_last_message) = now.duration_since(last_message_time) {
                            let one_minute = std::time::Duration::from_secs(60);
                            if duration_since_last_message < one_minute {
                                tracing::debug!(
                                    "Skipping title generation for conversation {}: last message is only {} seconds old",
                                    conversation_id,
                                    duration_since_last_message.as_secs()
                                );
                                continue;
                            }
                        }
                    }
                    
                    // Now call async function without holding the lock
                    use crate::storage::title_generation::generate_title_from_messages;
                    generate_title_from_messages(
                        messages,
                        &preset,
                        summary_chars,
                        &system_prompt,
                    ).await
                };

                match title_result {
                    Ok(title) => {
                        let storage_guard = storage.lock().await;
                        if let Err(e) = storage_guard.update_conversation_title_and_flag(&conversation_id, &title) {
                            tracing::warn!("Failed to update title for conversation {}: {}", conversation_id, e);
                        } else {
                            tracing::info!("Generated title for conversation {}: {}", conversation_id, title);
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Failed to generate title for conversation {}: {}", conversation_id, e);
                    }
                }
            }
        }
    });
}

