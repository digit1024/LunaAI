//! Tool wrappers for Rig engine: MCP tools (via rmcp) + internal tools (schedule_task, memory).
//!
//! Internal tools implement ToolDyn and emit ServerEvent (tool_started, tool_result, tool_error).
//! MCP tools use Rig's native rmcp_tools; ToolPlanned/ToolStarted/ToolResult come from pipeline stream.

use crate::embeddings::EmbeddingProvider;
use crate::mcp::McpRegistry;
use crate::server::conversation_subscriptions::ConversationSubscriptions;
use crate::server::dto::{PlannedToolView, ServerEvent};
use crate::services::ScheduleService;
use crate::storage::Storage;
use rig::completion::ToolDefinition;
use rig::tool::{ToolDyn, ToolError};
use rmcp::model::Tool as RmcpTool;
use rmcp::service::ServerSink;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use uuid::Uuid;

// ── Internal tools (schedule_task, memory) ──

/// Internal tool: schedule_task
struct ScheduleTaskTool {
    schedule_service: Arc<ScheduleService>,
    subscriptions: Arc<ConversationSubscriptions>,
    conversation_id: Uuid,
    profile_name: String,
}

impl ScheduleTaskTool {
    fn new(
        schedule_service: Arc<ScheduleService>,
        subscriptions: Arc<ConversationSubscriptions>,
        conversation_id: Uuid,
        profile_name: String,
    ) -> Self {
        Self {
            schedule_service,
            subscriptions,
            conversation_id,
            profile_name,
        }
    }
}

impl ToolDyn for ScheduleTaskTool {
    fn name(&self) -> String {
        "schedule_task".to_string()
    }

    fn definition<'a>(
        &'a self,
        _prompt: String,
    ) -> rig::wasm_compat::WasmBoxedFuture<'a, ToolDefinition> {
        Box::pin(async move {
            ToolDefinition {
                name: "schedule_task".to_string(),
                description: "Schedule a task to run at a later time, once or repeatedly. Use for reminders, 'do X in 1 hour', 'every day at 9am'.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "run_at": { "type": "string", "description": "When to run: relative e.g. 'in 30 minutes', or ISO 8601" },
                        "message": { "type": "string", "description": "Task/reminder or full initial prompt for new conversation" },
                        "schedule": { "type": "string", "description": "Optional. 'once' or cron (e.g. '0 9 * * *' daily 9am UTC)" },
                        "new_conversation": { "type": "boolean", "description": "If true, create fresh conversation at run time" },
                        "title": { "type": "string", "description": "Optional. For new_conversation: title" }
                    },
                    "required": ["run_at", "message"]
                }),
            }
        })
    }

    fn call<'a>(
        &'a self,
        args: String,
    ) -> rig::wasm_compat::WasmBoxedFuture<'a, Result<String, ToolError>> {
        Box::pin(async move {
            let name = self.name();
            let cid_str = self.conversation_id.to_string();
            let tool_call_id = format!("rig-{}", Uuid::new_v4());
            let params_value: Value =
                serde_json::from_str(&args).unwrap_or_else(|_| Value::Object(serde_json::Map::new()));
            let _ = self.subscriptions.broadcast(
                self.conversation_id,
                ServerEvent::ToolPlanned {
                    conversation_id: cid_str.clone(),
                    tools: vec![PlannedToolView {
                        id: tool_call_id.clone(),
                        name: name.clone(),
                        params_json: params_value.clone(),
                    }],
                },
            ).await;
            let _ = self.subscriptions.broadcast(
                self.conversation_id,
                ServerEvent::ToolStarted {
                    conversation_id: cid_str.clone(),
                    tool_call_id: tool_call_id.clone(),
                    name: name.clone(),
                    params_json: params_value,
                },
            ).await;

            let params: serde_json::Map<String, Value> =
                serde_json::from_str(&args).unwrap_or_default();
            let run_at = params.get("run_at").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let message = params.get("message").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let schedule = params.get("schedule").and_then(|v| v.as_str()).map(String::from);
            let new_conversation = params.get("new_conversation").and_then(|v| v.as_bool()).unwrap_or(false);
            let title = params.get("title").and_then(|v| v.as_str()).map(String::from);

            if message.is_empty() {
                let _ = self.subscriptions.broadcast(
                    self.conversation_id,
                    ServerEvent::ToolError {
                        conversation_id: cid_str,
                        tool_call_id,
                        name,
                        error: "schedule_task requires non-empty 'message'".to_string(),
                    },
                ).await;
                return Err(ToolError::ToolCallError(Box::new(std::io::Error::other(
                    "schedule_task requires non-empty 'message'",
                ))));
            }

            let result = self
                .schedule_service
                .schedule_task(
                    Some(self.conversation_id),
                    message,
                    &run_at,
                    schedule,
                    Some(self.profile_name.clone()),
                    title,
                    new_conversation,
                )
                .await;

            match result {
                Ok(job) => {
                    let schedule_desc = job
                        .schedule
                        .as_ref()
                        .filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("once"))
                        .map(|s| format!(", recurring: {}", s))
                        .unwrap_or_default();
                    let run_at_utc = chrono::DateTime::from_timestamp(job.run_at_utc_secs, 0)
                        .map(|dt| dt.format("%Y-%m-%d %H:%M UTC").to_string())
                        .unwrap_or_else(|| job.run_at_utc_secs.to_string());
                    let content = format!("Scheduled for {} (id: {}){}", run_at_utc, job.id, schedule_desc);
                    let result_json = json!({ "content": content, "is_error": false });
                    let _ = self.subscriptions.broadcast(
                        self.conversation_id,
                        ServerEvent::ToolResult {
                            conversation_id: cid_str,
                            tool_call_id,
                            name,
                            result_json,
                        },
                    ).await;
                    Ok(content)
                }
                Err(e) => {
                    let err_msg = format!("Failed to schedule: {}", e);
                    let _ = self.subscriptions.broadcast(
                        self.conversation_id,
                        ServerEvent::ToolError {
                            conversation_id: cid_str,
                            tool_call_id,
                            name,
                            error: err_msg.clone(),
                        },
                    ).await;
                    Err(ToolError::ToolCallError(Box::new(std::io::Error::other(err_msg))))
                }
            }
        })
    }
}

/// Internal tool: cancel_scheduled_task
struct CancelScheduledTaskTool {
    schedule_service: Arc<ScheduleService>,
    subscriptions: Arc<ConversationSubscriptions>,
    conversation_id: Uuid,
}

impl ToolDyn for CancelScheduledTaskTool {
    fn name(&self) -> String {
        "cancel_scheduled_task".to_string()
    }

    fn definition<'a>(
        &'a self,
        _prompt: String,
    ) -> rig::wasm_compat::WasmBoxedFuture<'a, ToolDefinition> {
        Box::pin(async move {
            ToolDefinition {
                name: "cancel_scheduled_task".to_string(),
                description: "Cancel a scheduled task by its id.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": { "job_id": { "type": "string", "description": "UUID of the job to cancel" } },
                    "required": ["job_id"]
                }),
            }
        })
    }

    fn call<'a>(
        &'a self,
        args: String,
    ) -> rig::wasm_compat::WasmBoxedFuture<'a, Result<String, ToolError>> {
        Box::pin(async move {
            let name = self.name();
            let cid_str = self.conversation_id.to_string();
            let tool_call_id = format!("rig-{}", Uuid::new_v4());
            let params_value: Value =
                serde_json::from_str(&args).unwrap_or_else(|_| Value::Object(serde_json::Map::new()));
            let _ = self.subscriptions.broadcast(
                self.conversation_id,
                ServerEvent::ToolPlanned {
                    conversation_id: cid_str.clone(),
                    tools: vec![PlannedToolView {
                        id: tool_call_id.clone(),
                        name: name.clone(),
                        params_json: params_value.clone(),
                    }],
                },
            ).await;
            let _ = self.subscriptions.broadcast(
                self.conversation_id,
                ServerEvent::ToolStarted {
                    conversation_id: cid_str.clone(),
                    tool_call_id: tool_call_id.clone(),
                    name: name.clone(),
                    params_json: params_value,
                },
            ).await;

            let params: serde_json::Map<String, Value> =
                serde_json::from_str(&args).unwrap_or_default();
            let job_id = params.get("job_id").and_then(|v| v.as_str()).unwrap_or("").trim();
            if job_id.is_empty() {
                let _ = self.subscriptions.broadcast(
                    self.conversation_id,
                    ServerEvent::ToolError {
                        conversation_id: cid_str,
                        tool_call_id,
                        name,
                        error: "cancel_scheduled_task requires non-empty 'job_id'".to_string(),
                    },
                ).await;
                return Err(ToolError::ToolCallError(Box::new(std::io::Error::other(
                    "cancel_scheduled_task requires non-empty 'job_id'",
                ))));
            }
            match self.schedule_service.cancel_scheduled_task(job_id).await {
                Ok(true) => {
                    let content = format!("Scheduled task {} has been cancelled.", job_id);
                    let result_json = json!({ "content": content, "is_error": false });
                    let _ = self.subscriptions.broadcast(
                        self.conversation_id,
                        ServerEvent::ToolResult {
                            conversation_id: cid_str,
                            tool_call_id,
                            name,
                            result_json,
                        },
                    ).await;
                    Ok(content)
                }
                Ok(false) => {
                    let content = format!("No scheduled task found with id '{}'.", job_id);
                    let result_json = json!({ "content": content, "is_error": false });
                    let _ = self.subscriptions.broadcast(
                        self.conversation_id,
                        ServerEvent::ToolResult {
                            conversation_id: cid_str,
                            tool_call_id,
                            name,
                            result_json,
                        },
                    ).await;
                    Ok(content)
                }
                Err(e) => {
                    let err_msg = format!("Failed to cancel: {}", e);
                    let _ = self.subscriptions.broadcast(
                        self.conversation_id,
                        ServerEvent::ToolError {
                            conversation_id: cid_str,
                            tool_call_id,
                            name,
                            error: err_msg.clone(),
                        },
                    ).await;
                    Err(ToolError::ToolCallError(Box::new(std::io::Error::other(err_msg))))
                }
            }
        })
    }
}

/// Internal tool: store_memory
struct StoreMemoryTool {
    storage: Arc<Mutex<Storage>>,
    embedding_provider: Option<Arc<dyn EmbeddingProvider>>,
    subscriptions: Arc<ConversationSubscriptions>,
    conversation_id: Uuid,
}

impl ToolDyn for StoreMemoryTool {
    fn name(&self) -> String {
        "store_memory".to_string()
    }

    fn definition<'a>(
        &'a self,
        _prompt: String,
    ) -> rig::wasm_compat::WasmBoxedFuture<'a, ToolDefinition> {
        Box::pin(async move {
            ToolDefinition {
                name: "store_memory".to_string(),
                description: "Store important facts or preferences in long-term memory.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "content": { "type": "string", "description": "The fact to remember" },
                        "category": { "type": "string", "description": "Tag (e.g. 'work', 'personal')" },
                        "importance": { "type": "integer", "description": "Priority 1-10 (default: 5)" }
                    },
                    "required": ["content"]
                }),
            }
        })
    }

    fn call<'a>(
        &'a self,
        args: String,
    ) -> rig::wasm_compat::WasmBoxedFuture<'a, Result<String, ToolError>> {
        Box::pin(async move {
            let name = self.name();
            let cid_str = self.conversation_id.to_string();
            let tool_call_id = format!("rig-{}", Uuid::new_v4());
            let params_value: Value =
                serde_json::from_str(&args).unwrap_or_else(|_| Value::Object(serde_json::Map::new()));
            let _ = self.subscriptions.broadcast(
                self.conversation_id,
                ServerEvent::ToolPlanned {
                    conversation_id: cid_str.clone(),
                    tools: vec![PlannedToolView {
                        id: tool_call_id.clone(),
                        name: name.clone(),
                        params_json: params_value.clone(),
                    }],
                },
            ).await;
            let _ = self.subscriptions.broadcast(
                self.conversation_id,
                ServerEvent::ToolStarted {
                    conversation_id: cid_str.clone(),
                    tool_call_id: tool_call_id.clone(),
                    name: name.clone(),
                    params_json: params_value,
                },
            ).await;

            let params: serde_json::Map<String, Value> =
                serde_json::from_str(&args).unwrap_or_default();
            let content = params.get("content").and_then(|v| v.as_str()).unwrap_or("").to_string();
            if content.is_empty() {
                let _ = self.subscriptions.broadcast(
                    self.conversation_id,
                    ServerEvent::ToolError {
                        conversation_id: cid_str,
                        tool_call_id,
                        name,
                        error: "store_memory requires non-empty 'content'".to_string(),
                    },
                ).await;
                return Err(ToolError::ToolCallError(Box::new(std::io::Error::other(
                    "store_memory requires non-empty 'content'",
                ))));
            }
            let category = params.get("category").and_then(|v| v.as_str());
            let importance = params.get("importance").and_then(|v| v.as_i64()).map(|v| v as i32);

            let guard = self.storage.lock().await;
            match guard.store_memory(&content, category, importance) {
                Ok(entry) => {
                    if let Some(provider) = &self.embedding_provider {
                        if let Ok(embedding) = provider.embed(&content).await {
                            let _ = guard.insert_memory_vec_row(entry.id, &embedding);
                        }
                    }
                    let result_str = serde_json::to_string(&entry).unwrap_or_else(|_| format!("Stored (id: {})", entry.id));
                    let result_json = json!({ "content": result_str, "is_error": false });
                    let _ = self.subscriptions.broadcast(
                        self.conversation_id,
                        ServerEvent::ToolResult {
                            conversation_id: cid_str,
                            tool_call_id,
                            name,
                            result_json,
                        },
                    ).await;
                    Ok(result_str)
                }
                Err(e) => {
                    let err_msg = format!("Failed to store memory: {}", e);
                    let _ = self.subscriptions.broadcast(
                        self.conversation_id,
                        ServerEvent::ToolError {
                            conversation_id: cid_str,
                            tool_call_id,
                            name,
                            error: err_msg.clone(),
                        },
                    ).await;
                    Err(ToolError::ToolCallError(Box::new(std::io::Error::other(err_msg))))
                }
            }
        })
    }
}

/// Internal tool: search_memory
struct SearchMemoryTool {
    storage: Arc<Mutex<Storage>>,
    subscriptions: Arc<ConversationSubscriptions>,
    conversation_id: Uuid,
}

impl ToolDyn for SearchMemoryTool {
    fn name(&self) -> String {
        "search_memory".to_string()
    }

    fn definition<'a>(
        &'a self,
        _prompt: String,
    ) -> rig::wasm_compat::WasmBoxedFuture<'a, ToolDefinition> {
        Box::pin(async move {
            ToolDefinition {
                name: "search_memory".to_string(),
                description: "Search long-term memory by keywords.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "keywords": { "type": "array", "items": { "type": "string" }, "description": "Keywords to search (OR)" }
                    },
                    "required": ["keywords"]
                }),
            }
        })
    }

    fn call<'a>(
        &'a self,
        args: String,
    ) -> rig::wasm_compat::WasmBoxedFuture<'a, Result<String, ToolError>> {
        Box::pin(async move {
            let name = self.name();
            let cid_str = self.conversation_id.to_string();
            let tool_call_id = format!("rig-{}", Uuid::new_v4());
            let params_value: Value =
                serde_json::from_str(&args).unwrap_or_else(|_| Value::Object(serde_json::Map::new()));
            let _ = self.subscriptions.broadcast(
                self.conversation_id,
                ServerEvent::ToolPlanned {
                    conversation_id: cid_str.clone(),
                    tools: vec![PlannedToolView {
                        id: tool_call_id.clone(),
                        name: name.clone(),
                        params_json: params_value.clone(),
                    }],
                },
            ).await;
            let _ = self.subscriptions.broadcast(
                self.conversation_id,
                ServerEvent::ToolStarted {
                    conversation_id: cid_str.clone(),
                    tool_call_id: tool_call_id.clone(),
                    name: name.clone(),
                    params_json: params_value,
                },
            ).await;

            let params: serde_json::Map<String, Value> =
                serde_json::from_str(&args).unwrap_or_default();
            let keywords: Vec<String> = params
                .get("keywords")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_default();
            if keywords.is_empty() {
                let _ = self.subscriptions.broadcast(
                    self.conversation_id,
                    ServerEvent::ToolError {
                        conversation_id: cid_str,
                        tool_call_id,
                        name,
                        error: "search_memory requires non-empty 'keywords'".to_string(),
                    },
                ).await;
                return Err(ToolError::ToolCallError(Box::new(std::io::Error::other(
                    "search_memory requires non-empty 'keywords'",
                ))));
            }
            let guard = self.storage.lock().await;
            match guard.search_memory(&keywords, 10) {
                Ok(entries) => {
                    let result_str = serde_json::to_string(&entries).unwrap_or_else(|_| "[]".to_string());
                    let result_json = json!({ "content": result_str, "is_error": false });
                    let _ = self.subscriptions.broadcast(
                        self.conversation_id,
                        ServerEvent::ToolResult {
                            conversation_id: cid_str,
                            tool_call_id,
                            name,
                            result_json,
                        },
                    ).await;
                    Ok(result_str)
                }
                Err(e) => {
                    let err_msg = format!("Failed to search: {}", e);
                    let _ = self.subscriptions.broadcast(
                        self.conversation_id,
                        ServerEvent::ToolError {
                            conversation_id: cid_str,
                            tool_call_id,
                            name,
                            error: err_msg.clone(),
                        },
                    ).await;
                    Err(ToolError::ToolCallError(Box::new(std::io::Error::other(err_msg))))
                }
            }
        })
    }
}

/// Internal tool: search_memory_by_category
struct SearchMemoryByCategoryTool {
    storage: Arc<Mutex<Storage>>,
    subscriptions: Arc<ConversationSubscriptions>,
    conversation_id: Uuid,
}

impl ToolDyn for SearchMemoryByCategoryTool {
    fn name(&self) -> String {
        "search_memory_by_category".to_string()
    }

    fn definition<'a>(
        &'a self,
        _prompt: String,
    ) -> rig::wasm_compat::WasmBoxedFuture<'a, ToolDefinition> {
        Box::pin(async move {
            ToolDefinition {
                name: "search_memory_by_category".to_string(),
                description: "Search memory by category.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": { "category": { "type": "string", "description": "Category to filter" } },
                    "required": ["category"]
                }),
            }
        })
    }

    fn call<'a>(
        &'a self,
        args: String,
    ) -> rig::wasm_compat::WasmBoxedFuture<'a, Result<String, ToolError>> {
        Box::pin(async move {
            let name = self.name();
            let cid_str = self.conversation_id.to_string();
            let tool_call_id = format!("rig-{}", Uuid::new_v4());
            let params_value: Value =
                serde_json::from_str(&args).unwrap_or_else(|_| Value::Object(serde_json::Map::new()));
            let _ = self.subscriptions.broadcast(
                self.conversation_id,
                ServerEvent::ToolPlanned {
                    conversation_id: cid_str.clone(),
                    tools: vec![PlannedToolView {
                        id: tool_call_id.clone(),
                        name: name.clone(),
                        params_json: params_value.clone(),
                    }],
                },
            ).await;
            let _ = self.subscriptions.broadcast(
                self.conversation_id,
                ServerEvent::ToolStarted {
                    conversation_id: cid_str.clone(),
                    tool_call_id: tool_call_id.clone(),
                    name: name.clone(),
                    params_json: params_value,
                },
            ).await;

            let params: serde_json::Map<String, Value> =
                serde_json::from_str(&args).unwrap_or_default();
            let category = params.get("category").and_then(|v| v.as_str()).unwrap_or("");
            if category.is_empty() {
                let _ = self.subscriptions.broadcast(
                    self.conversation_id,
                    ServerEvent::ToolError {
                        conversation_id: cid_str,
                        tool_call_id,
                        name,
                        error: "search_memory_by_category requires non-empty 'category'".to_string(),
                    },
                ).await;
                return Err(ToolError::ToolCallError(Box::new(std::io::Error::other(
                    "search_memory_by_category requires non-empty 'category'",
                ))));
            }
            let guard = self.storage.lock().await;
            match guard.search_memory_by_category(category, 50) {
                Ok(entries) => {
                    let result_str = serde_json::to_string(&entries).unwrap_or_else(|_| "[]".to_string());
                    let result_json = json!({ "content": result_str, "is_error": false });
                    let _ = self.subscriptions.broadcast(
                        self.conversation_id,
                        ServerEvent::ToolResult {
                            conversation_id: cid_str,
                            tool_call_id,
                            name,
                            result_json,
                        },
                    ).await;
                    Ok(result_str)
                }
                Err(e) => {
                    let err_msg = format!("Failed to search: {}", e);
                    let _ = self.subscriptions.broadcast(
                        self.conversation_id,
                        ServerEvent::ToolError {
                            conversation_id: cid_str,
                            tool_call_id,
                            name,
                            error: err_msg.clone(),
                        },
                    ).await;
                    Err(ToolError::ToolCallError(Box::new(std::io::Error::other(err_msg))))
                }
            }
        })
    }
}

/// Internal tool: delete_memory
struct DeleteMemoryTool {
    storage: Arc<Mutex<Storage>>,
    subscriptions: Arc<ConversationSubscriptions>,
    conversation_id: Uuid,
}

impl ToolDyn for DeleteMemoryTool {
    fn name(&self) -> String {
        "delete_memory".to_string()
    }

    fn definition<'a>(
        &'a self,
        _prompt: String,
    ) -> rig::wasm_compat::WasmBoxedFuture<'a, ToolDefinition> {
        Box::pin(async move {
            ToolDefinition {
                name: "delete_memory".to_string(),
                description: "Delete a memory entry by ID.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": { "memory_id": { "type": "integer", "description": "ID of memory to remove" } },
                    "required": ["memory_id"]
                }),
            }
        })
    }

    fn call<'a>(
        &'a self,
        args: String,
    ) -> rig::wasm_compat::WasmBoxedFuture<'a, Result<String, ToolError>> {
        Box::pin(async move {
            let name = self.name();
            let cid_str = self.conversation_id.to_string();
            let tool_call_id = format!("rig-{}", Uuid::new_v4());
            let params_value: Value =
                serde_json::from_str(&args).unwrap_or_else(|_| Value::Object(serde_json::Map::new()));
            let _ = self.subscriptions.broadcast(
                self.conversation_id,
                ServerEvent::ToolPlanned {
                    conversation_id: cid_str.clone(),
                    tools: vec![PlannedToolView {
                        id: tool_call_id.clone(),
                        name: name.clone(),
                        params_json: params_value.clone(),
                    }],
                },
            ).await;
            let _ = self.subscriptions.broadcast(
                self.conversation_id,
                ServerEvent::ToolStarted {
                    conversation_id: cid_str.clone(),
                    tool_call_id: tool_call_id.clone(),
                    name: name.clone(),
                    params_json: params_value,
                },
            ).await;

            let params: serde_json::Map<String, Value> =
                serde_json::from_str(&args).unwrap_or_default();
            let memory_id = params.get("memory_id").and_then(|v| v.as_i64()).unwrap_or(0);
            if memory_id == 0 {
                let _ = self.subscriptions.broadcast(
                    self.conversation_id,
                    ServerEvent::ToolError {
                        conversation_id: cid_str,
                        tool_call_id,
                        name,
                        error: "delete_memory requires non-zero 'memory_id'".to_string(),
                    },
                ).await;
                return Err(ToolError::ToolCallError(Box::new(std::io::Error::other(
                    "delete_memory requires non-zero 'memory_id'",
                ))));
            }
            let guard = self.storage.lock().await;
            match guard.delete_memory(memory_id) {
                Ok(true) => {
                    let content = format!("Memory {} deleted.", memory_id);
                    let result_json = json!({ "content": content, "is_error": false });
                    let _ = self.subscriptions.broadcast(
                        self.conversation_id,
                        ServerEvent::ToolResult {
                            conversation_id: cid_str,
                            tool_call_id,
                            name,
                            result_json,
                        },
                    ).await;
                    Ok(content)
                }
                Ok(false) => {
                    let content = format!("No memory found with id {}.", memory_id);
                    let result_json = json!({ "content": content, "is_error": false });
                    let _ = self.subscriptions.broadcast(
                        self.conversation_id,
                        ServerEvent::ToolResult {
                            conversation_id: cid_str,
                            tool_call_id,
                            name,
                            result_json,
                        },
                    ).await;
                    Ok(content)
                }
                Err(e) => {
                    let err_msg = format!("Failed to delete: {}", e);
                    let _ = self.subscriptions.broadcast(
                        self.conversation_id,
                        ServerEvent::ToolError {
                            conversation_id: cid_str,
                            tool_call_id,
                            name,
                            error: err_msg.clone(),
                        },
                    ).await;
                    Err(ToolError::ToolCallError(Box::new(std::io::Error::other(err_msg))))
                }
            }
        })
    }
}

/// Build turn tools: MCP (rmcp) + internal (schedule_task, memory).
/// Returns (mcp_servers: (tools, peer) per server, internal_tools).
pub async fn build_turn_tools(
    registry: &Arc<RwLock<McpRegistry>>,
    allowed_tool_names: &HashSet<String>,
    subscriptions: Arc<ConversationSubscriptions>,
    conversation_id: Uuid,
    profile_name: String,
    schedule_service: Arc<ScheduleService>,
    storage: Arc<Mutex<Storage>>,
    embedding_provider: Option<Arc<dyn EmbeddingProvider>>,
) -> anyhow::Result<(
    Vec<(Vec<RmcpTool>, ServerSink)>,
    Vec<Box<dyn rig::tool::ToolDyn + 'static>>,
)> {
    let mcp_servers = {
        let guard = registry.read().await;
        guard.get_mcp_servers_for_turn(allowed_tool_names)
    };

    let mut internal_tools: Vec<Box<dyn rig::tool::ToolDyn + 'static>> = Vec::new();
    if allowed_tool_names.contains("schedule_task") {
        internal_tools.push(Box::new(ScheduleTaskTool::new(
            schedule_service.clone(),
            subscriptions.clone(),
            conversation_id,
            profile_name.clone(),
        )));
    }
    if allowed_tool_names.contains("cancel_scheduled_task") {
        internal_tools.push(Box::new(CancelScheduledTaskTool {
            schedule_service: schedule_service.clone(),
            subscriptions: subscriptions.clone(),
            conversation_id,
        }));
    }
    if allowed_tool_names.contains("store_memory") {
        internal_tools.push(Box::new(StoreMemoryTool {
            storage: storage.clone(),
            embedding_provider: embedding_provider.clone(),
            subscriptions: subscriptions.clone(),
            conversation_id,
        }));
    }
    if allowed_tool_names.contains("search_memory") {
        internal_tools.push(Box::new(SearchMemoryTool {
            storage: storage.clone(),
            subscriptions: subscriptions.clone(),
            conversation_id,
        }));
    }
    if allowed_tool_names.contains("search_memory_by_category") {
        internal_tools.push(Box::new(SearchMemoryByCategoryTool {
            storage: storage.clone(),
            subscriptions: subscriptions.clone(),
            conversation_id,
        }));
    }
    if allowed_tool_names.contains("delete_memory") {
        internal_tools.push(Box::new(DeleteMemoryTool {
            storage: storage.clone(),
            subscriptions: subscriptions.clone(),
            conversation_id,
        }));
    }

    Ok((mcp_servers, internal_tools))
}
