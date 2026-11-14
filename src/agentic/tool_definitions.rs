//! Tool definitions for the built-in internal tools.
//!
//! Split out of `loop_engine.rs` (SMELL-3): pure data constructors shared by
//! the streaming and non-streaming dispatch paths via `dispatch_internal_tool`
//! and `build_available_tools`.

use crate::llm::ToolDefinition;

/// Merge tool definitions by `name`. Later entries replace earlier ones (internal tools win over MCP).
pub fn upsert_tool_definition(defs: &mut Vec<ToolDefinition>, tool: ToolDefinition) {
    if let Some(i) = defs.iter().position(|d| d.name == tool.name) {
        defs[i] = tool;
    } else {
        defs.push(tool);
    }
}

pub fn schedule_task_tool_definition() -> ToolDefinition {
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

pub fn store_memory_tool_definition() -> ToolDefinition {
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

pub fn search_memory_tool_definition() -> ToolDefinition {
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

pub fn search_memory_by_category_tool_definition() -> ToolDefinition {
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

pub fn search_attachment_chunks_tool_definition() -> ToolDefinition {
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

pub fn vision_analyze_tool_definition() -> ToolDefinition {
    ToolDefinition {
        name: "vision_analyze".to_string(),
        description: "Analyze an image on demand with the auxiliary vision model. Provide an \
uploaded attachment id or a workspace file path, and an optional instruction describing what \
to look for (e.g. 'How many fingers are shown?', 'What color is the button?'). Use when you \
need details from an image beyond the description already provided. Requires [vision] to be \
enabled with a profile."
            .to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "attachment_id": {
                    "type": "string",
                    "description": "Uploaded attachment id (uid) from the current conversation"
                },
                "path": {
                    "type": "string",
                    "description": "Workspace image path (within the configured [files].root)"
                },
                "instruction": {
                    "type": "string",
                    "description": "Optional specific question or instruction for the vision model (default: a general description)"
                }
            },
            "required": []
        }),
    }
}

pub fn delete_memory_tool_definition() -> ToolDefinition {    ToolDefinition {
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

pub fn search_history_tool_definition() -> ToolDefinition {
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

pub fn spawn_worker_tool_definition() -> ToolDefinition {
    ToolDefinition {
        name: "spawn_worker".to_string(),
        description: "Delegate a self-contained task to an isolated sub-agent that runs to \
            completion and returns its final response. Use when a task is independent of the \
            current conversation flow — e.g. deep research, writing a document, running a \
            multi-step analysis, or any work where intermediate steps would clutter this \
            conversation. The worker uses the current profile's tools policy; a different \
            profile may be specified only if the server configuration allows it \
            ([agentic].worker_profiles). Workers cannot spawn further workers beyond the \
            configured depth limit. The worker's internal reasoning and tool calls are stored \
            separately and never shown in this conversation. Only the final result is returned \
            here. Do NOT use for short tasks you can handle directly.".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "task": {
                    "type": "string",
                    "description": "Complete, self-contained description of the work to do. Include all context needed — the worker cannot access this conversation's history."
                },
                "profile": {
                    "type": "string",
                    "description": "Optional. Name of an allowed profile for this worker. If omitted (or overrides are not permitted), the current session profile is used."
                }
            },
            "required": ["task"]
        }),
    }
}

pub fn cancel_scheduled_task_tool_definition() -> ToolDefinition {
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

pub fn list_scheduled_tasks_tool_definition() -> ToolDefinition {
    ToolDefinition {
        name: "list_scheduled_tasks".to_string(),
        description: "List scheduled tasks (reminders and recurring jobs). Use when the user asks what tasks are scheduled, wants to see upcoming reminders, or needs job IDs to cancel something. Defaults to showing only pending jobs.".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "status": {
                    "type": "string",
                    "enum": ["pending", "running", "completed", "failed", "all"],
                    "description": "Filter by job status. 'pending' (default) = upcoming/active, 'all' = everything including completed/failed."
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of results (default: 20, max: 100)."
                }
            },
            "required": []
        }),
    }
}

pub fn list_conversations_tool_definition() -> ToolDefinition {
    ToolDefinition {
        name: "list_conversations".to_string(),
        description: "List past conversations with their titles, dates, and IDs. Use when the user asks to see recent chats, browse history, or wants a conversation ID. Results are ordered by most recent activity first.".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of conversations to return (default: 20, max: 100)."
                },
                "after_date": {
                    "type": "string",
                    "description": "Optional ISO 8601 date (e.g. '2026-01-01' or '2026-01-01T00:00:00Z'). Only return conversations created on or after this date."
                }
            },
            "required": []
        }),
    }
}

pub fn get_conversation_messages_tool_definition() -> ToolDefinition {
    ToolDefinition {
        name: "get_conversation_messages".to_string(),
        description: "Fetch messages from a specific past conversation by its ID. Use after list_conversations to read what was discussed. Returns messages in chronological order, most recent `limit` messages only.".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "conversation_id": {
                    "type": "string",
                    "description": "UUID of the conversation to fetch (as returned by list_conversations or search_history)."
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of messages to return, counting from the end (default: 50, max: 200)."
                }
            },
            "required": ["conversation_id"]
        }),
    }
}

pub fn publish_image_tool_definition() -> ToolDefinition {
    ToolDefinition {
        name: "publish_image".to_string(),
        description: "Publish a local image file so it can be embedded in a markdown reply to the user. Copies the file into static storage for the current conversation and returns a token-free luna-static: marker plus ready markdown. Never construct /api/static/ URLs yourself — embed the returned markdown verbatim.".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Absolute path to an existing local image file (jpg, jpeg, png, gif, webp, bmp, or svg)."
                },
                "alt": {
                    "type": "string",
                    "description": "Optional alt text for the markdown image (default: 'image')."
                }
            },
            "required": ["path"]
        }),
    }
}
