use super::protocol::{AgentUpdate, PlannedTool, RunContext};
use crate::llm::{ChatStreamEvent, LlmClient, LlmError, Message, Role, ToolCall, ToolDefinition, ToolResult};
use crate::mcp::conversions::{tool_call_to_params, tools_to_definitions};
use crate::services::ScheduleService;
use agentic_loop::mcp_servers_registry::MCPServerRegistry;
use anyhow::{Context, Result};
use futures::StreamExt;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{timeout, Duration};

/// Merge tool definitions by `name`. Later entries replace earlier ones (internal tools win over MCP).
fn upsert_tool_definition(defs: &mut Vec<ToolDefinition>, tool: ToolDefinition) {
    if let Some(i) = defs.iter().position(|d| d.name == tool.name) {
        defs[i] = tool;
    } else {
        defs.push(tool);
    }
}

fn schedule_task_tool_definition() -> ToolDefinition {
    ToolDefinition {
        name: "schedule_task".to_string(),
        description: "Schedule a task to run at a later time, once or repeatedly. Use for reminders, 'do X in 1 hour', 'every day at 9am', or 'every morning start a fresh conversation with prompt X'.".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "run_at": {
                    "type": "string",
                    "description": "When to run (first time): relative e.g. 'in 30 minutes', or ISO 8601 e.g. '2025-02-01T09:00:00Z'. For recurring, this is the first run."
                },
                "message": {
                    "type": "string",
                    "description": "In-conversation: short task/reminder (e.g. 'Call John'). New conversation: full initial user prompt (e.g. 'What is on my calendar today?')."
                },
                "schedule": {
                    "type": "string",
                    "description": "Optional. 'once' or omit = run once at run_at. For recurring, use 5-field cron (min hr dom mon dow, UTC): e.g. '0 * * * *' (every hour), '0 9 * * *' (daily 9am), '0 9 * * 1' (Monday 9am)."
                },
                "new_conversation": {
                    "type": "boolean",
                    "description": "If true, at run time create a fresh conversation and use message as the first user prompt. If false or omitted, inject into the current conversation as a reminder."
                },
                "title": {
                    "type": "string",
                    "description": "Optional. For new_conversation only: title of the new conversation (e.g. 'Daily calendar digest')."
                }
            },
            "required": ["run_at", "message"]
        }),
    }
}

fn store_memory_tool_definition() -> ToolDefinition {
    ToolDefinition {
        name: "store_memory".to_string(),
        description: "Store important facts, preferences, or relevant information in long-term memory. Use this to remember things across conversations. To update a memory, delete the old one first, then store the new version.".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "content": {
                    "type": "string",
                    "description": "The fact or information to remember"
                },
                "category": {
                    "type": "string",
                    "description": "A tag for grouping (e.g. 'workflow', 'personal', 'work', 'security')"
                },
                "importance": {
                    "type": "integer",
                    "description": "Priority score 1-10 (default: 5)"
                }
            },
            "required": ["content"]
        }),
    }
}

fn search_memory_tool_definition() -> ToolDefinition {
    ToolDefinition {
        name: "search_memory".to_string(),
        description: "Search long-term memory using full-text search. Use this to recall stored knowledge, user preferences, technical setups, or important facts from previous conversations. Results are ranked by relevance.".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "keywords": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Keywords to search in memory (OR semantics)"
                }
            },
            "required": ["keywords"]
        }),
    }
}

fn search_memory_by_category_tool_definition() -> ToolDefinition {
    ToolDefinition {
        name: "search_memory_by_category".to_string(),
        description: "Search long-term memory entries by category. Returns all entries in the given category, ordered by importance and recency.".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "category": {
                    "type": "string",
                    "description": "Category to filter (e.g. 'work', 'personal', 'security')"
                }
            },
            "required": ["category"]
        }),
    }
}

fn search_attachment_chunks_tool_definition() -> ToolDefinition {
    ToolDefinition {
        name: "search_attachment_chunks".to_string(),
        description: "Semantic search over large documents attached in the current conversation (chunk index). Use when the user asks about uploaded files that were indexed instead of fully inlined. Requires embeddings to be enabled.".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "What to find in the indexed attachment text"
                }
            },
            "required": ["query"]
        }),
    }
}

fn delete_memory_tool_definition() -> ToolDefinition {
    ToolDefinition {
        name: "delete_memory".to_string(),
        description: "Delete a memory entry by its ID. Use this to remove outdated or incorrect information from long-term memory.".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "memory_id": {
                    "type": "integer",
                    "description": "The ID of the memory entry to remove"
                }
            },
            "required": ["memory_id"]
        }),
    }
}

fn search_history_tool_definition() -> ToolDefinition {
    ToolDefinition {
        name: "search_history".to_string(),
        description: "Full-text search over raw conversation history. Use this to recall specific conversations, find what was discussed on a topic, or look up past exchanges. Results include the matching message snippet, its conversation ID, and timestamp — ranked by relevance (BM25). Unlike search_memory which searches curated facts, this searches the actual raw message text.".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Full-text search query (e.g. 'deployment pipeline', 'user authentication issue')"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of results to return (default: 20, max: 100)"
                }
            },
            "required": ["query"]
        }),
    }
}

fn cancel_scheduled_task_tool_definition() -> ToolDefinition {
    ToolDefinition {
        name: "cancel_scheduled_task".to_string(),
        description: "Cancel (delete) a scheduled task by its id. Use when the user asks to cancel, remove, or stop a scheduled reminder or recurring task. The job id was returned when the task was scheduled (e.g. 'Scheduled for ... (id: abc-123)').".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "job_id": {
                    "type": "string",
                    "description": "UUID of the scheduled job to cancel, as returned when the task was scheduled."
                }
            },
            "required": ["job_id"]
        }),
    }
}

pub struct AgenticLoop {
    pub mcp_registry: Arc<RwLock<MCPServerRegistry>>,
    pub llm_client: Arc<dyn LlmClient>,
    pub schedule_service: Option<Arc<ScheduleService>>,
    pub storage: Option<Arc<tokio::sync::Mutex<crate::storage::Storage>>>,
    pub tool_call_timeout_secs: u64,
}

impl AgenticLoop {
    pub fn new(
        mcp_registry: Arc<RwLock<MCPServerRegistry>>,
        llm_client: Arc<dyn LlmClient>,
        schedule_service: Option<Arc<ScheduleService>>,
        tool_call_timeout_secs: u64,
    ) -> Self {
        Self {
            mcp_registry,
            llm_client,
            schedule_service,
            storage: None,
            tool_call_timeout_secs,
        }
    }

    pub fn with_storage(mut self, storage: Arc<tokio::sync::Mutex<crate::storage::Storage>>) -> Self {
        self.storage = Some(storage);
        self
    }

    /// Build the list of available tools for this run.
    ///
    /// MCP tools and internal tools are both filtered by the run's `allowed_tool_names`
    /// (snapshotted from the profile policy at task spawn). The shared `MCPServerRegistry`
    /// is treated as a passive catalogue — its `tools_white_list` is intentionally not
    /// consulted, so concurrent profile switches in other sessions cannot leak tools into
    /// or out of this run.
    ///
    /// When `run_context` is `None` (legacy callers without a policy snapshot), all
    /// connected MCP tools and internal tools are exposed — preserving the previous
    /// permissive default for that path.
    async fn build_available_tools(
        &self,
        run_context: &Option<RunContext>,
    ) -> Result<Vec<crate::llm::ToolDefinition>> {
        let allow = |name: &str| -> bool {
            run_context
                .as_ref()
                .map(|r| r.allowed_tool_names.contains(name))
                .unwrap_or(true)
        };
        let registry = self.mcp_registry.read().await;
        let tools = registry
            .get_all_tools()
            .await
            .context("Failed to get MCP tools")?;
        let mut defs = Vec::new();
        for d in tools_to_definitions(&tools) {
            if !allow(&d.name) {
                continue;
            }
            upsert_tool_definition(&mut defs, d);
        }
        if allow("schedule_task") {
            upsert_tool_definition(&mut defs, schedule_task_tool_definition());
        }
        if allow("cancel_scheduled_task") {
            upsert_tool_definition(&mut defs, cancel_scheduled_task_tool_definition());
        }
        if self.storage.is_some() {
            if allow("store_memory") {
                upsert_tool_definition(&mut defs, store_memory_tool_definition());
            }
            if allow("search_memory") {
                upsert_tool_definition(&mut defs, search_memory_tool_definition());
            }
            if allow("search_memory_by_category") {
                upsert_tool_definition(&mut defs, search_memory_by_category_tool_definition());
            }
            if allow("delete_memory") {
                upsert_tool_definition(&mut defs, delete_memory_tool_definition());
            }
            if allow("search_attachment_chunks") {
                upsert_tool_definition(&mut defs, search_attachment_chunks_tool_definition());
            }
            if allow("search_history") {
                upsert_tool_definition(&mut defs, search_history_tool_definition());
            }
        }
        tracing::debug!(tool_count = defs.len(), "Enabled tools");
        Ok(defs)
    }

    /// Whether the current run is permitted to call `tool_name`. Used to reject MCP calls
    /// the model invents/hallucinates outside the snapshotted allow-set (defense in depth).
    fn is_tool_allowed(run_context: Option<&RunContext>, tool_name: &str) -> bool {
        match run_context {
            Some(ctx) => ctx.allowed_tool_names.contains(tool_name),
            None => true,
        }
    }

    pub async fn process_message(
        &mut self,
        mut messages: Vec<Message>,
        agent_tx: Option<tokio::sync::mpsc::UnboundedSender<AgentUpdate>>,
        run_context: Option<RunContext>,
    ) -> Result<String> {
        loop {
            let available_tools = self.build_available_tools(&run_context).await?;

            let mut stream = match self
                .llm_client
                .send_message_stream_with_tools(messages.clone(), available_tools, None, None)
                .await
            {
                Ok(stream) => stream,
                Err(LlmError::Config(e)) => {
                    tracing::warn!(
                        "Tool streaming unsupported for backend, falling back to non-streaming mode: {}",
                        e
                    );
                    return self.process_non_streaming(messages, agent_tx, run_context).await;
                }
                Err(e) => {
                    tracing::error!(error = %e, "LLM streaming call failed");
                    if let Some(tx) = agent_tx.as_ref() {
                        let _ = tx.send(AgentUpdate::ModelError {
                            error: format!("Model communication failed: {}", e),
                        });
                    }
                    return Err(e).context("LLM call failed");
                }
            };

            if let Some(tx) = agent_tx.as_ref() {
                let _ = tx.send(AgentUpdate::AssistantStreamingStarted);
            }

            let run_context_clone = run_context.clone();
            let mut assistant_response = String::new();
            let mut reasoning_content = String::new();
            let mut planned_tools: Vec<ToolCall> = Vec::new();
            let mut seq: u64 = 0;

            while let Some(event) = stream.next().await {
                match event {
                    Ok(ChatStreamEvent::ContentDelta(chunk)) => {
                        if chunk.is_empty() {
                            continue;
                        }
                        assistant_response.push_str(&chunk);
                        seq += 1;
                        if let Some(tx) = agent_tx.as_ref() {
                            let _ = tx.send(AgentUpdate::AssistantDelta {
                                text_chunk: chunk,
                                seq,
                            });
                        }
                    }
                    Ok(ChatStreamEvent::ReasoningContentDelta(chunk)) => {
                        if !chunk.is_empty() {
                            reasoning_content.push_str(&chunk);
                            // Send reasoning content delta during streaming
                            if let Some(tx) = agent_tx.as_ref() {
                                let _ = tx.send(AgentUpdate::ReasoningContentDelta {
                                    chunk: chunk.clone(),
                                });
                            }
                        }
                    }
                    Ok(ChatStreamEvent::ToolCallDelta(tool_call)) => {
                        if let Some(tx) = agent_tx.as_ref() {
                            let planned = PlannedTool {
                                id: tool_call.id.clone(),
                                name: tool_call.name.clone(),
                                params_json: serde_json::to_string(&tool_call.parameters)
                                    .unwrap_or_default(),
                            };
                            let _ = tx.send(AgentUpdate::ToolPlanned {
                                plan_items: vec![planned],
                            });
                        }
                        planned_tools.push(tool_call);
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "Streaming event error");
                        if let Some(tx) = agent_tx.as_ref() {
                            let _ = tx.send(AgentUpdate::ModelError {
                                error: format!("Streaming error: {}", e),
                            });
                        }
                        return Err(e).context("Streaming error");
                    }
                }
            }

            if let Some(tx) = agent_tx.as_ref() {
                let _ = tx.send(AgentUpdate::AssistantComplete {
                    full_text: assistant_response.clone(),
                    reasoning_content: if reasoning_content.is_empty() { None } else { Some(reasoning_content.clone()) },
                });
            }

            if planned_tools.is_empty() {
                
                if let Some(tx) = agent_tx.as_ref() {
                    let _ = tx.send(AgentUpdate::ConversationComplete {
                        final_text: assistant_response.clone(),
                    });
                }
                return Ok(assistant_response);
            }

            let mut assistant_msg = Message::new_with_tool_calls(
                Role::Assistant,
                assistant_response.clone(),
                planned_tools.clone(),
            );
            assistant_msg.reasoning_content = if reasoning_content.is_empty() { None } else { Some(reasoning_content.clone()) };
            messages.push(assistant_msg);

            for tool_call in planned_tools {
                if let Some(tx) = agent_tx.as_ref() {
                    let _ = tx.send(AgentUpdate::ToolStarted {
                        tool_call_id: tool_call.id.clone(),
                        name: tool_call.name.clone(),
                        params_json: serde_json::to_string(&tool_call.parameters)
                            .unwrap_or_default(),
                    });
                }

                let result = if !Self::is_tool_allowed(run_context_clone.as_ref(), &tool_call.name) {
                    tracing::warn!(
                        tool = %tool_call.name,
                        "Rejecting tool call: not in run's allowed_tool_names (policy violation)"
                    );
                    ToolResult {
                        content: format!(
                            "Tool '{}' is not allowed by the current profile's tools policy.",
                            tool_call.name
                        ),
                        is_error: true,
                    }
                } else if tool_call.name == "schedule_task" {
                    self.execute_schedule_task(&tool_call, run_context_clone.as_ref()).await
                } else if tool_call.name == "cancel_scheduled_task" {
                    self.execute_cancel_scheduled_task(&tool_call).await
                } else if tool_call.name == "store_memory"
                    || tool_call.name == "search_memory"
                    || tool_call.name == "search_memory_by_category"
                    || tool_call.name == "delete_memory"
                {
                    self.execute_memory_tool(&tool_call, run_context_clone.as_ref()).await
                } else if tool_call.name == "search_attachment_chunks" {
                    self.execute_attachment_search_tool(&tool_call, run_context_clone.as_ref())
                        .await
                } else if tool_call.name == "search_history" {
                    self.execute_search_history_tool(&tool_call).await
                } else {
                    self.execute_tool_with_retry(tool_call.clone(), agent_tx.as_ref()).await
                };

                if let Some(tx) = agent_tx.as_ref() {
                    let _ = tx.send(AgentUpdate::ToolResult {
                        tool_call_id: tool_call.id.clone(),
                        name: tool_call.name.clone(),
                        result_json: result.content.clone(),
                    });
                }

                messages.push(Message::new_tool_result(
                    tool_call.id.clone(),
                    result.content,
                    result.is_error,
                ));
            }
        }
    }

    async fn execute_memory_tool(&self, tool_call: &ToolCall, run_context: Option<&RunContext>) -> ToolResult {
        let Some(storage) = &self.storage else {
            return ToolResult {
                content: "Memory tools are not available (no storage)".to_string(),
                is_error: true,
            };
        };
        let params = &tool_call.parameters;
        let guard = storage.lock().await;

        match tool_call.name.as_str() {
            "store_memory" => {
                let content = params.get("content").and_then(|v| v.as_str()).unwrap_or("").to_string();
                if content.is_empty() {
                    return ToolResult { content: "store_memory requires non-empty 'content'".to_string(), is_error: true };
                }
                let category = params.get("category").and_then(|v| v.as_str());
                let importance = params.get("importance").and_then(|v| v.as_i64()).map(|v| v as i32);
                match guard.store_memory(&content, category, importance) {
                    Ok(entry) => {
                        // Insert into memory_vec when embedding provider is configured
                        if let Some(ctx) = run_context {
                            if let Some(provider) = &ctx.embedding_provider {
                                if let Ok(embedding) = provider.embed(&content).await {
                                    if let Err(e) = guard.insert_memory_vec_row(entry.id, &embedding) {
                                        tracing::warn!(error = %e, "Failed to insert memory vector");
                                    }
                                }
                            }
                        }
                        ToolResult {
                            content: serde_json::to_string(&entry).unwrap_or_else(|_| format!("Stored memory (id: {})", entry.id)),
                            is_error: false,
                        }
                    }
                    Err(e) => ToolResult { content: format!("Failed to store memory: {}", e), is_error: true },
                }
            }
            "search_memory" => {
                let keywords: Vec<String> = params.get("keywords")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                    .unwrap_or_default();
                if keywords.is_empty() {
                    return ToolResult { content: "search_memory requires non-empty 'keywords'".to_string(), is_error: true };
                }
                match guard.search_memory(&keywords, 10) {
                    Ok(entries) => ToolResult {
                        content: serde_json::to_string(&entries).unwrap_or_else(|_| "[]".to_string()),
                        is_error: false,
                    },
                    Err(e) => ToolResult { content: format!("Failed to search memory: {}", e), is_error: true },
                }
            }
            "search_memory_by_category" => {
                let category = params.get("category").and_then(|v| v.as_str()).unwrap_or("");
                if category.is_empty() {
                    return ToolResult { content: "search_memory_by_category requires non-empty 'category'".to_string(), is_error: true };
                }
                match guard.search_memory_by_category(category, 50) {
                    Ok(entries) => ToolResult {
                        content: serde_json::to_string(&entries).unwrap_or_else(|_| "[]".to_string()),
                        is_error: false,
                    },
                    Err(e) => ToolResult { content: format!("Failed to search memory by category: {}", e), is_error: true },
                }
            }
            "delete_memory" => {
                let memory_id = params.get("memory_id").and_then(|v| v.as_i64()).unwrap_or(0);
                if memory_id == 0 {
                    return ToolResult { content: "delete_memory requires non-zero 'memory_id'".to_string(), is_error: true };
                }
                match guard.delete_memory(memory_id) {
                    Ok(true) => ToolResult {
                        content: format!("Memory {} deleted.", memory_id),
                        is_error: false,
                    },
                    Ok(false) => ToolResult {
                        content: format!("No memory found with id {}.", memory_id),
                        is_error: false,
                    },
                    Err(e) => ToolResult { content: format!("Failed to delete memory: {}", e), is_error: true },
                }
            }
            _ => ToolResult {
                content: format!("Unknown memory tool: {}", tool_call.name),
                is_error: true,
            },
        }
    }

    async fn execute_search_history_tool(&self, tool_call: &ToolCall) -> ToolResult {
        let Some(storage) = &self.storage else {
            return ToolResult {
                content: "search_history requires storage".to_string(),
                is_error: true,
            };
        };
        let params = &tool_call.parameters;
        let query = params.get("query").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
        if query.is_empty() {
            return ToolResult {
                content: "search_history requires a non-empty 'query'".to_string(),
                is_error: true,
            };
        }
        let limit = params
            .get("limit")
            .and_then(|v| v.as_u64())
            .map(|n| (n as usize).min(100))
            .unwrap_or(20);

        let guard = storage.lock().await;
        match guard.search_history(&query, limit) {
            Ok(snippets) if snippets.is_empty() => ToolResult {
                content: "No conversations found matching your search.".to_string(),
                is_error: false,
            },
            Ok(snippets) => {
                let lines: Vec<String> = snippets
                    .into_iter()
                    .map(|s| {
                        let ts = chrono::DateTime::from_timestamp(s.timestamp, 0)
                            .map(|dt| dt.format("%Y-%m-%d %H:%M UTC").to_string())
                            .unwrap_or_else(|| "unknown time".to_string());
                        format!(
                            "conversation_id: {}\ntime: {}\ncontent: {}\nrelevance_rank: {:.3}",
                            s.conversation_id, ts, s.content, s.rank
                        )
                    })
                    .collect();
                ToolResult {
                    content: lines.join("\n\n---\n\n"),
                    is_error: false,
                }
            }
            Err(e) => ToolResult {
                content: format!("search_history failed: {}", e),
                is_error: true,
            },
        }
    }

    async fn execute_schedule_task(
        &self,
        tool_call: &ToolCall,
        run_context: Option<&RunContext>,
    ) -> ToolResult {
        let Some(svc) = &self.schedule_service else {
            return ToolResult {
                content: "schedule_task is not available (no schedule service)".to_string(),
                is_error: true,
            };
        };
        let Some(ctx) = run_context else {
            return ToolResult {
                content: "schedule_task requires run context (conversation_id and profile_name)".to_string(),
                is_error: true,
            };
        };
        let params = &tool_call.parameters;
        let run_at = params.get("run_at").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let message = params.get("message").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let schedule = params.get("schedule").and_then(|v| v.as_str()).map(String::from);
        let new_conversation = params.get("new_conversation").and_then(|v| v.as_bool()).unwrap_or(false);
        let title = params.get("title").and_then(|v| v.as_str()).map(String::from);

        if message.is_empty() {
            return ToolResult {
                content: "schedule_task requires non-empty 'message'".to_string(),
                is_error: true,
            };
        }

        match svc
            .schedule_task(
                ctx.conversation_id,
                message,
                &run_at,
                schedule,
                Some(ctx.profile_name.clone()),
                title,
                new_conversation,
            )
            .await
        {
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
                ToolResult {
                    content: format!(
                        "Scheduled for {} (id: {}){}",
                        run_at_utc,
                        job.id,
                        schedule_desc
                    ),
                    is_error: false,
                }
            }
            Err(e) => ToolResult {
                content: format!("Failed to schedule: {}", e),
                is_error: true,
            },
        }
    }

    async fn execute_cancel_scheduled_task(&self, tool_call: &ToolCall) -> ToolResult {
        let Some(svc) = &self.schedule_service else {
            return ToolResult {
                content: "cancel_scheduled_task is not available (no schedule service)".to_string(),
                is_error: true,
            };
        };
        let job_id = tool_call
            .parameters
            .get("job_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        if job_id.is_empty() {
            return ToolResult {
                content: "cancel_scheduled_task requires non-empty 'job_id'".to_string(),
                is_error: true,
            };
        }
        match svc.cancel_scheduled_task(job_id).await {
            Ok(true) => ToolResult {
                content: format!("Scheduled task {} has been cancelled.", job_id),
                is_error: false,
            },
            Ok(false) => ToolResult {
                content: format!(
                    "No scheduled task found with id '{}'. It may have already run or been cancelled.",
                    job_id
                ),
                is_error: false,
            },
            Err(e) => ToolResult {
                content: format!("Failed to cancel scheduled task: {}", e),
                is_error: true,
            },
        }
    }

    async fn execute_tool_with_retry(
        &self,
        tool_call: ToolCall,
        agent_tx: Option<&tokio::sync::mpsc::UnboundedSender<AgentUpdate>>,
    ) -> ToolResult {
        let mut attempt: u8 = 0;
        let max_retries: u8 = 2;
        let per_call_timeout = Duration::from_secs(self.tool_call_timeout_secs);

        loop {
            attempt += 1;
            let tool_name = tool_call.name.clone();
            let call_future = async {
                let mut registry = self.mcp_registry.write().await;
                let params = tool_call_to_params(&tool_call);
                registry.call_tool(tool_name, params.arguments.unwrap_or_default()).await
            };
            match timeout(per_call_timeout, call_future).await {
                Ok(Ok(sdk_result)) => {
                    return ToolResult::from(&sdk_result);
                }
                Ok(Err(e)) => {
                    if let Some(tx) = agent_tx {
                        let _ = tx.send(AgentUpdate::ToolError {
                            tool_call_id: tool_call.id.clone(),
                            name: tool_call.name.clone(),
                            error: e.to_string(),
                            retryable: attempt <= max_retries,
                        });
                    }
                    if attempt > max_retries {
                        return ToolResult {
                            content: format!("Error: {}", e),
                            is_error: true,
                        };
                    }
                }
                Err(_) => {
                    let err_msg = format!("Timeout after {:?}", per_call_timeout);
                    if let Some(tx) = agent_tx {
                        let _ = tx.send(AgentUpdate::ToolError {
                            tool_call_id: tool_call.id.clone(),
                            name: tool_call.name.clone(),
                            error: err_msg.clone(),
                            retryable: attempt <= max_retries,
                        });
                    }
                    if attempt > max_retries {
                        return ToolResult {
                            content: "Timeout".to_string(),
                            is_error: true,
                        };
                    }
                }
            }
        }
    }

    async fn process_non_streaming(
        &mut self,
        mut messages: Vec<Message>,
        agent_tx: Option<tokio::sync::mpsc::UnboundedSender<AgentUpdate>>,
        run_context: Option<RunContext>,
    ) -> Result<String> {
        loop {
            let available_tools = self.build_available_tools(&run_context).await?;

            let response = match self
                .llm_client
                .send_message_with_tools(messages.clone(), available_tools, None, None)
                .await
            {
                Ok(resp) => resp,
                Err(e) => {
                    tracing::error!(error = %e, "Non-streaming LLM call failed");
                    if let Some(tx) = agent_tx.as_ref() {
                        let _ = tx.send(AgentUpdate::ModelError {
                            error: format!("Model communication failed: {}", e),
                        });
                    }
                    return Err(e).context("LLM call failed");
                }
            };

            if let Some(tx) = agent_tx.as_ref() {
                let _ = tx.send(AgentUpdate::AssistantStreamingStarted);
                if !response.content.is_empty() {
                    let _ = tx.send(AgentUpdate::AssistantDelta {
                        text_chunk: response.content.clone(),
                        seq: 1,
                    });
                }
                let _ = tx.send(AgentUpdate::AssistantComplete {
                    full_text: response.content.clone(),
                    reasoning_content: response.reasoning_content.clone(),
                });
            }

            if response.tool_calls.is_empty() {
                
                if let Some(tx) = agent_tx.as_ref() {
                    let _ = tx.send(AgentUpdate::ConversationComplete {
                        final_text: response.content.clone(),
                    });
                }
                return Ok(response.content);
            }

            if let Some(tx) = agent_tx.as_ref() {
                let plan_items: Vec<PlannedTool> = response
                    .tool_calls
                    .iter()
                    .map(|tc| PlannedTool {
                        id: tc.id.clone(),
                        name: tc.name.clone(),
                        params_json: serde_json::to_string(&tc.parameters).unwrap_or_default(),
                    })
                    .collect();
                if !plan_items.is_empty() {
                    let _ = tx.send(AgentUpdate::ToolPlanned { plan_items });
                }
            }

            let mut assistant_msg = Message::new_with_tool_calls(
                Role::Assistant,
                response.content.clone(),
                response.tool_calls.clone(),
            );
            assistant_msg.reasoning_content = response.reasoning_content.clone();
            messages.push(assistant_msg);

            for tool_call in response.tool_calls {
                if let Some(tx) = agent_tx.as_ref() {
                    let _ = tx.send(AgentUpdate::ToolStarted {
                        tool_call_id: tool_call.id.clone(),
                        name: tool_call.name.clone(),
                        params_json: serde_json::to_string(&tool_call.parameters)
                            .unwrap_or_default(),
                    });
                }

                let result = if !Self::is_tool_allowed(run_context.as_ref(), &tool_call.name) {
                    tracing::warn!(
                        tool = %tool_call.name,
                        "Rejecting tool call: not in run's allowed_tool_names (policy violation)"
                    );
                    ToolResult {
                        content: format!(
                            "Tool '{}' is not allowed by the current profile's tools policy.",
                            tool_call.name
                        ),
                        is_error: true,
                    }
                } else if tool_call.name == "schedule_task" {
                    self.execute_schedule_task(&tool_call, run_context.as_ref()).await
                } else if tool_call.name == "cancel_scheduled_task" {
                    self.execute_cancel_scheduled_task(&tool_call).await
                } else if tool_call.name == "store_memory"
                    || tool_call.name == "search_memory"
                    || tool_call.name == "search_memory_by_category"
                    || tool_call.name == "delete_memory"
                {
                    self.execute_memory_tool(&tool_call, run_context.as_ref()).await
                } else if tool_call.name == "search_attachment_chunks" {
                    self.execute_attachment_search_tool(&tool_call, run_context.as_ref())
                        .await
                } else if tool_call.name == "search_history" {
                    self.execute_search_history_tool(&tool_call).await
                } else {
                    self.execute_tool_with_retry(tool_call.clone(), agent_tx.as_ref()).await
                };

                if let Some(tx) = agent_tx.as_ref() {
                    let _ = tx.send(AgentUpdate::ToolResult {
                        tool_call_id: tool_call.id.clone(),
                        name: tool_call.name.clone(),
                        result_json: result.content.clone(),
                    });
                }

                messages.push(Message::new_tool_result(
                    tool_call.id.clone(),
                    result.content,
                    result.is_error,
                ));
            }
        }
    }

    async fn execute_attachment_search_tool(
        &self,
        tool_call: &ToolCall,
        run_context: Option<&RunContext>,
    ) -> ToolResult {
        let Some(storage) = &self.storage else {
            return ToolResult {
                content: "search_attachment_chunks requires storage".to_string(),
                is_error: true,
            };
        };
        let Some(ctx) = run_context else {
            return ToolResult {
                content: "search_attachment_chunks requires run context".to_string(),
                is_error: true,
            };
        };
        let Some(conv) = ctx.conversation_id else {
            return ToolResult {
                content: "search_attachment_chunks requires an active conversation".to_string(),
                is_error: true,
            };
        };
        let Some(provider) = ctx.embedding_provider.as_ref() else {
            return ToolResult {
                content: "search_attachment_chunks requires embedding to be enabled (same setup as memory vector search).".to_string(),
                is_error: true,
            };
        };
        let query = tool_call
            .parameters
            .get("query")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        if query.is_empty() {
            return ToolResult {
                content: "search_attachment_chunks requires non-empty 'query'".to_string(),
                is_error: true,
            };
        }
        let emb = match provider.embed(query).await {
            Ok(e) => e,
            Err(e) => {
                return ToolResult {
                    content: format!("embedding failed: {}", e),
                    is_error: true,
                };
            }
        };
        let guard = storage.lock().await;
        match guard.search_attachment_chunks_by_vector(
            &conv.to_string(),
            &emb,
            ctx.attachment_rag.search_limit,
            ctx.attachment_rag.max_distance,
        ) {
            Ok(hits) => {
                #[derive(serde::Serialize)]
                struct HitSer {
                    attachment_uid: String,
                    file_name: String,
                    chunk_index: i32,
                    text: String,
                    distance: f32,
                }
                let payload: Vec<HitSer> = hits
                    .into_iter()
                    .map(|h| HitSer {
                        attachment_uid: h.attachment_uid,
                        file_name: h.file_name,
                        chunk_index: h.chunk_index,
                        text: h.text,
                        distance: h.distance,
                    })
                    .collect();
                ToolResult {
                    content: serde_json::to_string(&payload).unwrap_or_else(|_| "[]".to_string()),
                    is_error: false,
                }
            }
            Err(e) => ToolResult {
                content: format!("search failed: {}", e),
                is_error: true,
            },
        }
    }
}
