use chrono::{DateTime, Utc};
use rusqlite::Result as SqliteResult;
use std::path::Path;
use tracing;
use uuid::Uuid;

use super::conversation_storage::{Conversation as FileConversation, StoredMessage};
use super::sqlite_storage_simple::{MessageMetadata, ScheduledJob, SqliteSettings, SqliteStorage};

/// Wrapper that provides compatibility with the existing file-based storage API
pub struct Storage {
    sqlite: SqliteStorage,
}

impl Storage {
    /// Create a new storage instance with SQLite backend
    pub fn new<P: AsRef<Path>>(db_path: P) -> SqliteResult<Self> {
        let sqlite = SqliteStorage::new(db_path)?;
        Ok(Self { sqlite })
    }

    /// Create a new storage instance with custom SQLite settings
    pub fn new_with_settings<P: AsRef<Path>>(
        db_path: P,
        settings: SqliteSettings,
    ) -> SqliteResult<Self> {
        let sqlite = SqliteStorage::new_with_settings(db_path, &settings)?;
        Ok(Self { sqlite })
    }

    /// Create a new storage instance with default database path
    pub fn new_default() -> SqliteResult<Self> {
        let db_path = Self::default_db_path();
        Self::new(db_path)
    }

    pub fn new_default_with_settings(settings: SqliteSettings) -> SqliteResult<Self> {
        let db_path = Self::default_db_path();
        Self::new_with_settings(db_path, settings)
    }

    fn default_db_path() -> std::path::PathBuf {
        dirs::data_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("cosmic_llm")
            .join("conversations.db")
    }

    /// Create a new conversation with profile
    pub fn create_conversation_with_profile(
        &self,
        title: String,
        profile_name: Option<&str>,
    ) -> SqliteResult<Uuid> {
        let id_str = self
            .sqlite
            .insert_conversation_with_profile(&title, profile_name)?;
        Uuid::parse_str(&id_str)
            .map_err(|e| rusqlite::Error::InvalidParameterName(format!("Invalid UUID: {}", e)))
    }

    /// Create an internal worker conversation (hidden from UI and deep sleep).
    pub fn create_internal_conversation(
        &self,
        title: String,
        profile_name: Option<&str>,
    ) -> SqliteResult<Uuid> {
        self.sqlite
            .create_internal_conversation(&title, profile_name)
    }

    /// Get a conversation by ID
    pub fn get_conversation(&self, id: &Uuid) -> SqliteResult<Option<FileConversation>> {
        let id_str = id.to_string();
        if let Some(db_conv) = self.sqlite.get_conversation(&id_str)? {
            let messages = self.sqlite.load_conversation(&id_str)?;

            let stored_messages: Vec<StoredMessage> = messages
                .into_iter()
                .map(|msg| StoredMessage {
                    // Generate deterministic UUID from rowid to ensure stable IDs across loads
                    id: {
                        let id_str = msg.id.to_string();
                        Uuid::parse_str(&id_str).unwrap_or_else(|_| {
                            // Format: 00000000-0000-0000-0000-{rowid:012x} (12 hex digits)
                            let hex_id = format!("{:012x}", msg.id.min(0xffffffffffff));
                            let uuid_str = format!("00000000-0000-0000-0000-{}", hex_id);
                            uuid_str.parse().unwrap_or_else(|_| Uuid::new_v4())
                        })
                    },
                    role: msg.role,
                    content: msg.content,
                    timestamp: DateTime::from_timestamp(msg.created_at, 0).unwrap_or_else(Utc::now),
                    tool_calls: msg.tool_calls,
                    tool_call_id: msg.tool_call_id,
                    tool_name: msg.tool_name,
                    tool_status: msg.tool_status,
                    tool_params_json: msg.tool_params_json.clone(),
                    tool_result_json: msg.tool_result_json.clone(),
                    reasoning_content: msg.reasoning_content.clone(),
                    is_summary: msg.is_summary,
                    is_summarized: msg.is_summarized,
                    summarized_count: msg.summarized_count,
                    attachments: msg.attachments.clone(),
                })
                .collect();

            let updated_at_secs = db_conv.last_message.unwrap_or(db_conv.created_at);
            let conversation = FileConversation {
                id: *id,
                title: db_conv.title,
                created_at: DateTime::from_timestamp(db_conv.created_at, 0)
                    .unwrap_or_else(Utc::now),
                updated_at: DateTime::from_timestamp(updated_at_secs, 0).unwrap_or_else(Utc::now),
                messages: stored_messages,
                turns: Vec::new(), // Turns are not yet migrated to SQLite
                profile_name: db_conv.profile_name.clone(),
            };

            Ok(Some(conversation))
        } else {
            Ok(None)
        }
    }

    /// List conversations with pagination
    pub fn list_conversations_paginated(
        &self,
        offset: Option<usize>,
        limit: Option<usize>,
    ) -> SqliteResult<Vec<FileConversation>> {
        let db_conversations = self.sqlite.list_conversations_paginated(offset, limit)?;
        let mut conversations = Vec::new();

        for db_conv in db_conversations {
            let id = Uuid::parse_str(&db_conv.id).map_err(|e| {
                rusqlite::Error::InvalidParameterName(format!("Invalid UUID: {}", e))
            })?;

            let messages = self.sqlite.load_conversation(&db_conv.id)?;
            let stored_messages: Vec<StoredMessage> = messages
                .into_iter()
                .map(|msg| StoredMessage {
                    // Generate deterministic UUID from rowid to ensure stable IDs across loads
                    id: {
                        let id_str = msg.id.to_string();
                        Uuid::parse_str(&id_str).unwrap_or_else(|_| {
                            // Format: 00000000-0000-0000-0000-{rowid:012x} (12 hex digits)
                            let hex_id = format!("{:012x}", msg.id.min(0xffffffffffff));
                            let uuid_str = format!("00000000-0000-0000-0000-{}", hex_id);
                            uuid_str.parse().unwrap_or_else(|_| Uuid::new_v4())
                        })
                    },
                    role: msg.role,
                    content: msg.content,
                    timestamp: DateTime::from_timestamp(msg.created_at, 0).unwrap_or_else(Utc::now),
                    tool_calls: msg.tool_calls,
                    tool_call_id: msg.tool_call_id,
                    tool_name: msg.tool_name,
                    tool_status: msg.tool_status,
                    tool_params_json: msg.tool_params_json.clone(),
                    tool_result_json: msg.tool_result_json.clone(),
                    reasoning_content: msg.reasoning_content.clone(),
                    is_summary: msg.is_summary,
                    is_summarized: msg.is_summarized,
                    summarized_count: msg.summarized_count,
                    attachments: msg.attachments.clone(),
                })
                .collect();

            let updated_at_secs = db_conv.last_message.unwrap_or(db_conv.created_at);
            let conversation = FileConversation {
                id,
                title: db_conv.title,
                created_at: DateTime::from_timestamp(db_conv.created_at, 0)
                    .unwrap_or_else(Utc::now),
                updated_at: DateTime::from_timestamp(updated_at_secs, 0).unwrap_or_else(Utc::now),
                messages: stored_messages,
                turns: Vec::new(), // Turns are not yet migrated to SQLite
                profile_name: db_conv.profile_name.clone(),
            };

            conversations.push(conversation);
        }

        Ok(conversations)
    }

    /// Update conversation title
    pub fn update_conversation_title(&self, id: &Uuid, title: String) -> SqliteResult<bool> {
        let id_str = id.to_string();
        self.sqlite.update_title(&id_str, &title)
    }

    /// Update conversation profile
    pub fn update_conversation_profile(
        &self,
        id: &Uuid,
        profile_name: Option<&str>,
    ) -> SqliteResult<bool> {
        let id_str = id.to_string();
        self.sqlite.update_profile(&id_str, profile_name)
    }

    /// Add a message to a conversation. Returns the new message's ID (as Uuid, same format as in get_conversation).
    pub fn add_message_to_conversation(
        &self,
        conversation_id: &Uuid,
        role: String,
        content: String,
    ) -> SqliteResult<Uuid> {
        self.add_message_with_metadata(
            conversation_id,
            role,
            content,
            None,
            MessageMetadata::default(),
        )
    }

    /// Add a message with metadata. Returns the new message's ID (as Uuid).
    pub fn add_message_with_metadata(
        &self,
        conversation_id: &Uuid,
        role: String,
        content: String,
        embedding: Option<&[f32]>,
        metadata: MessageMetadata<'_>,
    ) -> SqliteResult<Uuid> {
        let id_str = conversation_id.to_string();
        let rowid = self
            .sqlite
            .insert_message_with_metadata(&id_str, &role, &content, embedding, &metadata)?;
        // Same deterministic UUID format as in get_conversation / truncate
        let hex_id = format!("{:012x}", rowid.min(0xffffffffffff));
        let uuid_str = format!("00000000-0000-0000-0000-{}", hex_id);
        Uuid::parse_str(&uuid_str)
            .map_err(|e| rusqlite::Error::InvalidParameterName(format!("Invalid UUID: {}", e)))
    }

    /// Delete a conversation
    /// Delete messages by database row ids (e.g. incomplete tool-tail repair).
    pub fn delete_messages(&self, message_ids: &[i64]) -> SqliteResult<usize> {
        self.sqlite.delete_messages(message_ids)
    }

    pub fn delete_conversation(&self, conversation_id: &Uuid) -> SqliteResult<bool> {
        let id_str = conversation_id.to_string();
        self.sqlite.delete_conversation(&id_str)
    }

    /// Truncate conversation: delete all messages up to and including the specified message
    pub fn truncate_conversation(
        &self,
        conversation_id: &Uuid,
        message_id: &Uuid,
    ) -> SqliteResult<usize> {
        let conv_id_str = conversation_id.to_string();
        let msg_id_str = message_id.to_string();

        tracing::debug!(
            "Truncating conversation {} at message {}",
            conv_id_str,
            msg_id_str
        );

        // Load all messages to find which one matches the UUID
        // (UUIDs are generated from i64 rowid in get_conversation)
        let messages = self.sqlite.load_conversation(&conv_id_str)?;

        tracing::debug!("Loaded {} messages from conversation", messages.len());

        // Find the target message by matching UUID
        // Since UUIDs are generated from i64 rowid, we need to convert each message's rowid to UUID
        // and compare with the target message_id
        let target_msg = messages.iter().find(|msg| {
            // Convert rowid to UUID using the same logic as in get_conversation
            let id_str = msg.id.to_string();
            let msg_uuid = Uuid::parse_str(&id_str).unwrap_or_else(|_| {
                // Generate deterministic UUID from rowid (format: 00000000-0000-0000-0000-{rowid:012x})
                let hex_id = format!("{:012x}", msg.id.min(0xffffffffffff));
                let uuid_str = format!("00000000-0000-0000-0000-{}", hex_id);
                uuid_str.parse().unwrap_or_else(|_| Uuid::new_v4())
            });
            msg_uuid == *message_id
        });

        if let Some(target) = target_msg {
            // Use the rowid directly for deletion
            let target_rowid = target.id;
            tracing::info!(
                "Found target message with rowid: {}, timestamp: {}",
                target_rowid,
                target.created_at
            );

            // Delete all messages including and AFTER this one using the sqlite method
            let changes = self
                .sqlite
                .truncate_conversation_by_rowid(&conv_id_str, target_rowid)?;

            tracing::info!(
                "Deleted {} messages from rowid {} (inclusive)",
                changes,
                target_rowid
            );
            Ok(changes)
        } else {
            tracing::warn!(
                "Message with UUID {} not found in conversation {} (checked {} messages)",
                msg_id_str,
                conv_id_str,
                messages.len()
            );

            // Debug: log all message IDs to help troubleshoot
            for (idx, msg) in messages.iter().enumerate() {
                let id_str = msg.id.to_string();
                let uuid_str = match Uuid::parse_str(&id_str) {
                    Ok(uuid) => uuid.to_string(),
                    Err(_) => format!("<invalid-uuid:{}>", id_str),
                };
                tracing::debug!("  Message {}: rowid={}, uuid={}", idx, msg.id, uuid_str);
            }

            Ok(0)
        }
    }

    /// Search conversation history
    pub fn search_history(
        &self,
        query: &str,
        limit: usize,
    ) -> SqliteResult<Vec<super::sqlite_storage_simple::Snippet>> {
        self.sqlite.search_history(query, limit)
    }

    /// Load the most recent `limit` messages from a conversation (oldest-first order).
    pub fn load_conversation_messages_limited(
        &self,
        conversation_id: &str,
        limit: usize,
    ) -> SqliteResult<Vec<super::sqlite_storage_simple::Message>> {
        self.sqlite
            .load_conversation_messages_limited(conversation_id, limit)
    }

    /// List conversations (metadata only — no messages loaded), optionally filtered by
    /// a minimum creation date expressed as a unix timestamp.
    pub fn list_conversations_metadata(
        &self,
        limit: usize,
        after_date_secs: Option<i64>,
    ) -> SqliteResult<Vec<super::sqlite_storage_simple::Conversation>> {
        self.sqlite.list_conversations_metadata(limit, after_date_secs)
    }

    /// List scheduled jobs, optionally filtered by status (pass `None` for all statuses).
    pub fn list_scheduled_jobs(
        &self,
        status: Option<&str>,
        limit: usize,
    ) -> SqliteResult<Vec<super::sqlite_storage_simple::ScheduledJob>> {
        self.sqlite.list_scheduled_jobs(status, limit)
    }

    /// Get conversations without generated titles
    pub fn get_conversations_without_title(&self) -> SqliteResult<Vec<Uuid>> {
        let db_conversations = self.sqlite.get_conversations_without_title()?;
        let mut conversation_ids = Vec::new();

        for db_conv in db_conversations {
            let id = Uuid::parse_str(&db_conv.id).map_err(|e| {
                rusqlite::Error::InvalidParameterName(format!("Invalid UUID: {}", e))
            })?;
            conversation_ids.push(id);
        }

        Ok(conversation_ids)
    }

    /// Update conversation title and set title_generated flag
    pub fn update_conversation_title_and_flag(&self, id: &Uuid, title: &str) -> SqliteResult<bool> {
        let id_str = id.to_string();
        self.sqlite
            .update_conversation_title_and_flag(&id_str, title)
    }

    /// Load messages for a conversation (exposed for title generation)
    pub fn load_conversation_messages(
        &self,
        conversation_id: &str,
    ) -> SqliteResult<Vec<super::sqlite_storage_simple::Message>> {
        self.sqlite.load_conversation(conversation_id)
    }

    /// Perform summarization: delete old messages and insert summary
    pub fn perform_summarization(
        &self,
        conversation_id: &str,
        messages_to_summarize: &[super::sqlite_storage_simple::Message],
        summary_content: &str,
        summary_created_at: i64,
    ) -> SqliteResult<()> {
        self.sqlite.perform_summarization(
            conversation_id,
            messages_to_summarize,
            summary_content,
            summary_created_at,
        )
    }

    // ── Long-term memory methods ──

    /// Store a memory entry
    pub fn store_memory(
        &self,
        content: &str,
        category: Option<&str>,
        importance: Option<i32>,
    ) -> SqliteResult<super::sqlite_storage_simple::MemoryEntry> {
        self.sqlite.store_memory(content, category, importance)
    }

    /// Search memory via FTS5
    pub fn search_memory(
        &self,
        keywords: &[String],
        limit: usize,
    ) -> SqliteResult<Vec<super::sqlite_storage_simple::MemoryEntry>> {
        self.sqlite.search_memory(keywords, limit)
    }

    /// Search memory via FTS5 with pagination.
    pub fn search_memory_paginated(
        &self,
        keywords: &[String],
        limit: usize,
        offset: usize,
    ) -> SqliteResult<Vec<super::sqlite_storage_simple::MemoryEntry>> {
        self.sqlite.search_memory_paginated(keywords, limit, offset)
    }

    /// Search memory by category
    pub fn search_memory_by_category(
        &self,
        category: &str,
        limit: usize,
    ) -> SqliteResult<Vec<super::sqlite_storage_simple::MemoryEntry>> {
        self.sqlite.search_memory_by_category(category, limit)
    }

    /// Delete a memory entry by ID
    pub fn delete_memory(&self, memory_id: i64) -> SqliteResult<bool> {
        self.sqlite.delete_memory(memory_id)
    }

    /// List all memory entries
    pub fn list_memory(
        &self,
        limit: usize,
    ) -> SqliteResult<Vec<super::sqlite_storage_simple::MemoryEntry>> {
        self.sqlite.list_memory(limit)
    }

    /// List memory entries with pagination.
    pub fn list_memory_paginated(
        &self,
        limit: usize,
        offset: usize,
    ) -> SqliteResult<Vec<super::sqlite_storage_simple::MemoryEntry>> {
        self.sqlite.list_memory_paginated(limit, offset)
    }

    /// Fetch a single memory entry by ID.
    pub fn get_memory_by_id(
        &self,
        memory_id: i64,
    ) -> SqliteResult<Option<super::sqlite_storage_simple::MemoryEntry>> {
        self.sqlite.get_memory_by_id(memory_id)
    }

    /// Return (id, title) pairs for the given conversation IDs.
    pub fn get_conversation_titles(
        &self,
        ids: &[String],
    ) -> SqliteResult<Vec<(String, String)>> {
        self.sqlite.get_conversation_titles(ids)
    }

    /// Update a memory entry
    pub fn update_memory(
        &self,
        memory_id: i64,
        content: &str,
        category: Option<&str>,
        importance: i32,
    ) -> SqliteResult<bool> {
        self.sqlite
            .update_memory(memory_id, content, category, importance)
    }

    /// Insert a row into memory_vec (vector index). Call after store_memory when embedding is enabled.
    pub fn insert_memory_vec_row(&self, memory_id: i64, embedding: &[f32]) -> SqliteResult<()> {
        self.sqlite.insert_memory_vec_row(memory_id, embedding)
    }

    /// Update a row in memory_vec. Call after update_memory when embedding is enabled.
    pub fn update_memory_vec_row(&self, memory_id: i64, embedding: &[f32]) -> SqliteResult<()> {
        self.sqlite.update_memory_vec_row(memory_id, embedding)
    }

    /// Search memories by vector similarity (KNN).
    /// If max_distance is Some, only returns entries with distance <= max_distance.
    pub fn search_memory_by_vector(
        &self,
        embedding: &[f32],
        limit: usize,
        max_distance: Option<f32>,
    ) -> SqliteResult<Vec<super::sqlite_storage_simple::MemoryEntry>> {
        self.sqlite
            .search_memory_by_vector(embedding, limit, max_distance)
    }

    /// Delete all rows from memory_vec. Used when reorganizing the vector index.
    pub fn delete_all_memory_vec_rows(&self) -> SqliteResult<()> {
        self.sqlite.delete_all_memory_vec_rows()
    }

    pub fn delete_attachment_doc_chunks_for(
        &self,
        conversation_id: &str,
        attachment_uid: &str,
    ) -> SqliteResult<()> {
        self.sqlite
            .delete_attachment_doc_chunks_for(conversation_id, attachment_uid)
    }

    pub fn insert_attachment_doc_chunk(
        &self,
        chunk: super::sqlite_storage_simple::AttachmentDocChunk<'_>,
    ) -> SqliteResult<()> {
        self.sqlite.insert_attachment_doc_chunk(chunk)
    }

    pub fn search_attachment_chunks_by_vector(
        &self,
        conversation_id: &str,
        embedding: &[f32],
        limit: usize,
        max_distance: Option<f32>,
    ) -> SqliteResult<Vec<super::sqlite_storage_simple::AttachmentDocSearchHit>> {
        self.sqlite
            .search_attachment_chunks_by_vector(conversation_id, embedding, limit, max_distance)
    }

    /// Get a deep sleep state value
    pub fn get_deep_sleep_state(&self, key: &str) -> SqliteResult<Option<String>> {
        self.sqlite.get_deep_sleep_state(key)
    }

    /// Set a deep sleep state value (upsert)
    pub fn set_deep_sleep_state(&self, key: &str, value: &str) -> SqliteResult<()> {
        self.sqlite.set_deep_sleep_state(key, value)
    }

    /// Get conversations that have messages after a given message ID
    pub fn get_conversations_with_messages_after(
        &self,
        message_id: i64,
        limit: usize,
    ) -> SqliteResult<Vec<super::sqlite_storage_simple::Conversation>> {
        self.sqlite
            .get_conversations_with_messages_after(message_id, limit)
    }

    /// Record memories recalled (injected) in this conversation
    pub fn record_memory_recalls(
        &self,
        conversation_id: &str,
        memory_ids: &[i64],
    ) -> SqliteResult<()> {
        self.sqlite
            .record_memory_recalls(conversation_id, memory_ids)
    }

    /// Insert a scheduled job
    pub fn insert_scheduled_job(&self, job: &ScheduledJob) -> SqliteResult<()> {
        self.sqlite.insert_scheduled_job(job)
    }

    /// Insert one row into the LLM call audit log.
    pub fn insert_llm_call(&self, rec: &crate::llm::LlmCallRecord) -> SqliteResult<()> {
        self.sqlite.insert_llm_call(rec)
    }

    /// Get due scheduled jobs
    pub fn get_due_scheduled_jobs(
        &self,
        now_utc_secs: i64,
        limit: u32,
    ) -> SqliteResult<Vec<ScheduledJob>> {
        self.sqlite.get_due_scheduled_jobs(now_utc_secs, limit)
    }

    /// Mark scheduled job as running
    pub fn set_scheduled_job_running(&self, id: &str, now_utc_secs: i64) -> SqliteResult<bool> {
        self.sqlite.set_scheduled_job_running(id, now_utc_secs)
    }

    /// Mark scheduled job completed or failed
    pub fn set_scheduled_job_completed(
        &self,
        id: &str,
        now_utc_secs: i64,
        failed: bool,
        error_message: Option<&str>,
    ) -> SqliteResult<()> {
        self.sqlite
            .set_scheduled_job_completed(id, now_utc_secs, failed, error_message)
    }

    /// Set next run for recurring scheduled job
    pub fn set_scheduled_job_next_run(
        &self,
        id: &str,
        next_run_utc_secs: i64,
        now_utc_secs: i64,
    ) -> SqliteResult<()> {
        self.sqlite
            .set_scheduled_job_next_run(id, next_run_utc_secs, now_utc_secs)
    }

    /// Delete a scheduled job by id
    pub fn delete_scheduled_job(&self, id: &str) -> SqliteResult<bool> {
        self.sqlite.delete_scheduled_job(id)
    }
}

impl Default for Storage {
    fn default() -> Self {
        Self::new_default().unwrap_or_else(|e| {
            tracing::error!(
                error = %e,
                "Failed to initialize SQLite storage"
            );
            // Fallback to a temporary database
            Self::new(std::env::temp_dir().join("cosmic_llm_temp.db")).unwrap_or_else(|e| {
                tracing::error!(error = %e, "Failed to create temporary database");
                std::process::exit(1);
            })
        })
    }
}
