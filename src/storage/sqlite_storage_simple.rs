use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension, Result as SqliteResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;
use std::time::Duration;
use uuid::Uuid;

use crate::{
    config::ServerConfig,
    llm::{Attachment, ToolCall},
};

// Load sqlite-vec extension for memory vector search (must be called before opening any connection)
fn load_sqlite_vec_extension() {
    unsafe {
        use rusqlite::auto_extension::register_auto_extension;
        type RawExt = unsafe extern "C" fn(
            *mut rusqlite::ffi::sqlite3,
            *mut *mut std::ffi::c_char,
            *const rusqlite::ffi::sqlite3_api_routines,
        ) -> std::ffi::c_int;
        let init_fn =
            std::mem::transmute::<*const (), RawExt>(sqlite_vec::sqlite3_vec_init as *const ());
        if let Err(e) = register_auto_extension(init_fn) {
            tracing::warn!(error = %e, "Failed to load sqlite-vec extension; memory vector search disabled");
        }
    }
}

/// Represents a conversation in the database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub id: String,
    pub title: String,
    pub created_at: i64,
    pub title_generated: bool,
    pub profile_name: Option<String>,
    pub last_message: Option<i64>,
}

/// Represents a message in the database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: i64,
    pub conversation_id: String,
    pub role: String,
    pub content: String,
    pub embedding: Option<Vec<f32>>,
    pub created_at: i64,
    pub tool_calls: Option<Vec<ToolCall>>,
    pub tool_call_id: Option<String>,
    pub tool_name: Option<String>,
    pub tool_status: Option<String>,
    pub tool_params_json: Option<Value>,
    pub tool_result_json: Option<Value>,
    pub reasoning_content: Option<String>, // For DeepSeek thinking/reasoning content
    #[serde(default)]
    pub is_summary: bool, // True if this message is a summary of previous messages
    #[serde(default)]
    #[allow(dead_code)] // Field used for serialization/storage
    pub is_summarized: bool, // True if this message has been summarized (should be excluded from LLM payload)
    pub summarized_message_ids: Option<Vec<i64>>, // IDs of messages that were summarized
    pub summarized_count: Option<usize>,          // Count of messages summarized
    /// JSON-serialized `Vec<Attachment>` for user messages with files; omit image `content` when persisted.
    #[serde(default)]
    pub attachments: Option<Vec<Attachment>>,
}

/// Represents a search snippet from FTS5
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snippet {
    pub conversation_id: String,
    pub content: String,
    pub timestamp: i64,
    pub rank: f64,
}

/// Vector search hit over chunked attachment text (same conversation only).
#[derive(Debug, Clone)]
pub struct AttachmentDocSearchHit {
    pub attachment_uid: String,
    pub file_name: String,
    pub chunk_index: i32,
    pub text: String,
    pub distance: f32,
}

/// A long-term memory entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: i64,
    pub content: String,
    pub category: Option<String>,
    pub importance: i32,
    pub created_at: i64,
    pub updated_at: Option<i64>,
}

/// A scheduled job (one-shot or recurring via cron)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledJob {
    pub id: String,
    pub conversation_id: Option<String>,
    pub run_at_utc_secs: i64,
    pub message: String,
    pub profile_name: Option<String>,
    pub title: Option<String>,
    pub status: String,
    pub created_at_utc_secs: i64,
    pub updated_at_utc_secs: i64,
    pub error_message: Option<String>,
    pub schedule: Option<String>,
}

/// SQLite-based storage implementation
pub struct SqliteStorage {
    conn: Connection,
    /// Dimension of memory embeddings; None means vec search is disabled.
    embedding_dimension: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct SqliteSettings {
    pub wal_enabled: bool,
    pub wal_autocheckpoint: u32,
    pub busy_timeout_ms: u64,
    /// When Some, enables memory_vec table for vector search. Loads sqlite-vec extension.
    pub embedding_dimension: Option<usize>,
}

impl Default for SqliteSettings {
    fn default() -> Self {
        Self {
            wal_enabled: true,
            wal_autocheckpoint: 200,
            busy_timeout_ms: 5_000,
            embedding_dimension: None,
        }
    }
}

impl From<&ServerConfig> for SqliteSettings {
    fn from(cfg: &ServerConfig) -> Self {
        Self {
            wal_enabled: cfg.wal_enabled,
            wal_autocheckpoint: cfg.wal_autocheckpoint,
            busy_timeout_ms: cfg.sqlite_busy_timeout_ms,
            embedding_dimension: None,
        }
    }
}

impl SqliteStorage {
    /// Create a new SQLite storage instance
    pub fn new<P: AsRef<Path>>(db_path: P) -> SqliteResult<Self> {
        Self::new_with_settings(db_path, &SqliteSettings::default())
    }

    /// Create a new SQLite storage instance with explicit settings
    pub fn new_with_settings<P: AsRef<Path>>(
        db_path: P,
        settings: &SqliteSettings,
    ) -> SqliteResult<Self> {
        if settings.embedding_dimension.is_some() {
            load_sqlite_vec_extension();
        }
        let conn = Connection::open(db_path)?;
        Self::configure_connection(&conn, settings)?;
        let storage = Self {
            conn,
            embedding_dimension: settings.embedding_dimension,
        };
        storage.init_database(settings.embedding_dimension)?;
        Ok(storage)
    }

    fn configure_connection(conn: &Connection, settings: &SqliteSettings) -> SqliteResult<()> {
        if settings.wal_enabled {
            conn.pragma_update(None, "journal_mode", &"WAL")?;
            conn.pragma_update(None, "wal_autocheckpoint", &settings.wal_autocheckpoint)?;
        } else {
            conn.pragma_update(None, "journal_mode", &"DELETE")?;
        }

        conn.busy_timeout(Duration::from_millis(settings.busy_timeout_ms))?;
        Ok(())
    }

    /// Initialize the database schema
    fn init_database(&self, embedding_dimension: Option<usize>) -> SqliteResult<()> {
        // Enable FTS5 extension (this is just a check, we don't need the results)
        let _: Vec<String> = self
            .conn
            .prepare("PRAGMA compile_options")?
            .query_map([], |row| row.get(0))?
            .collect::<Result<Vec<String>, _>>()?;

        // Create conversations table
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS conversations (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                title_generated INTEGER NOT NULL DEFAULT 0,
                profile_name TEXT,
                last_message INTEGER
            )",
            [],
        )?;

        // Migrate existing conversations: add profile_name column if it doesn't exist
        let _ = self
            .conn
            .execute("ALTER TABLE conversations ADD COLUMN profile_name TEXT", []);

        // Migrate existing conversations: add last_message column if it doesn't exist
        let _ = self.conn.execute(
            "ALTER TABLE conversations ADD COLUMN last_message INTEGER",
            [],
        );

        // Create messages table
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS messages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                conversation_id TEXT NOT NULL,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                embedding BLOB,
                created_at INTEGER NOT NULL,
                tool_calls TEXT,
                tool_call_id TEXT,
                tool_name TEXT,
                tool_status TEXT,
                tool_params_json TEXT,
                tool_result_json TEXT,
                reasoning_content TEXT,
                FOREIGN KEY (conversation_id) REFERENCES conversations (id) ON DELETE CASCADE
            )",
            [],
        )?;

        // Migrate existing messages: add reasoning_content column if it doesn't exist
        let _ = self
            .conn
            .execute("ALTER TABLE messages ADD COLUMN reasoning_content TEXT", []);

        // Migrate existing messages: add summarization columns if they don't exist
        let _ = self.conn.execute(
            "ALTER TABLE messages ADD COLUMN is_summary INTEGER NOT NULL DEFAULT 0",
            [],
        );
        let _ = self.conn.execute(
            "ALTER TABLE messages ADD COLUMN is_summarized INTEGER NOT NULL DEFAULT 0",
            [],
        );
        let _ = self.conn.execute(
            "ALTER TABLE messages ADD COLUMN summarized_message_ids TEXT",
            [],
        );
        let _ = self.conn.execute(
            "ALTER TABLE messages ADD COLUMN summarized_count INTEGER",
            [],
        );

        let _ = self
            .conn
            .execute("ALTER TABLE messages ADD COLUMN attachments_json TEXT", []);

        // Create FTS5 virtual table for full-text search
        self.conn.execute(
            "CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(
                conversation_id,
                content,
                content = 'messages',
                content_rowid = 'id'
            )",
            [],
        )?;

        // Create trigger to automatically index new messages into FTS5
        self.conn.execute(
            "CREATE TRIGGER IF NOT EXISTS messages_ai AFTER INSERT ON messages BEGIN
                INSERT INTO messages_fts(rowid, conversation_id, content)
                VALUES (new.id, new.conversation_id, new.content);
            END",
            [],
        )?;

        // Create trigger to update FTS5 when messages are updated
        self.conn.execute(
            "CREATE TRIGGER IF NOT EXISTS messages_au AFTER UPDATE ON messages BEGIN
                UPDATE messages_fts SET conversation_id = new.conversation_id, content = new.content
                WHERE rowid = new.id;
            END",
            [],
        )?;

        // Create trigger to delete from FTS5 when messages are deleted
        self.conn.execute(
            "CREATE TRIGGER IF NOT EXISTS messages_ad AFTER DELETE ON messages BEGIN
                DELETE FROM messages_fts WHERE rowid = old.id;
            END",
            [],
        )?;

        // Create indexes for better performance
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_messages_conversation_id ON messages(conversation_id)",
            [],
        )?;

        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_messages_created_at ON messages(created_at)",
            [],
        )?;

        // Scheduled jobs (cron + once)
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS scheduled_jobs (
                id TEXT PRIMARY KEY,
                conversation_id TEXT,
                run_at_utc_secs INTEGER NOT NULL,
                message TEXT NOT NULL,
                profile_name TEXT,
                title TEXT,
                status TEXT NOT NULL,
                created_at_utc_secs INTEGER NOT NULL,
                updated_at_utc_secs INTEGER NOT NULL,
                error_message TEXT,
                schedule TEXT
            )",
            [],
        )?;

        // Long-term memory storage (ported from mcp_luna_memory)
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS memory (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                content TEXT NOT NULL,
                category TEXT,
                importance INTEGER DEFAULT 5,
                created_at INTEGER,
                updated_at INTEGER
            )",
            [],
        )?;

        // Migration: add updated_at column if missing (existing DBs)
        let _ = self
            .conn
            .execute("ALTER TABLE memory ADD COLUMN updated_at INTEGER", []);

        // Deep sleep state (key-value store for tracking progress)
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS deep_sleep_state (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            )",
            [],
        )?;

        // Which memories were recalled (injected) in which conversation
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS conversation_memory_recalls (
                conversation_id TEXT NOT NULL,
                memory_id INTEGER NOT NULL,
                recalled_at INTEGER NOT NULL,
                PRIMARY KEY (conversation_id, memory_id),
                FOREIGN KEY (memory_id) REFERENCES memory(id) ON DELETE CASCADE
            )",
            [],
        )?;
        let _ = self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_recalls_memory_id ON conversation_memory_recalls(memory_id)",
            [],
        );

        // FTS5 virtual table for memory full-text search
        self.conn.execute(
            "CREATE VIRTUAL TABLE IF NOT EXISTS memory_fts USING fts5(
                content,
                content='memory',
                content_rowid='id'
            )",
            [],
        )?;

        // Trigger: auto-sync FTS index on memory insert
        self.conn.execute(
            "CREATE TRIGGER IF NOT EXISTS memory_ai AFTER INSERT ON memory BEGIN
                INSERT INTO memory_fts(rowid, content) VALUES (new.id, new.content);
            END",
            [],
        )?;

        // Trigger: auto-sync FTS index on memory delete
        self.conn.execute(
            "CREATE TRIGGER IF NOT EXISTS memory_ad AFTER DELETE ON memory BEGIN
                INSERT INTO memory_fts(memory_fts, rowid, content) VALUES('delete', old.id, old.content);
            END",
            [],
        )?;

        // Rebuild FTS index from content table (syncs pre-existing rows not covered by triggers)
        self.conn
            .execute("INSERT INTO memory_fts(memory_fts) VALUES('rebuild')", [])?;

        // Create memory_vec virtual table for vector search when embedding is enabled
        if let Some(dim) = embedding_dimension {
            if dim > 0 {
                let sql = format!(
                    "CREATE VIRTUAL TABLE IF NOT EXISTS memory_vec USING vec0(embedding float[{}])",
                    dim
                );
                if let Err(e) = self.conn.execute(&sql, []) {
                    tracing::warn!(error = %e, dim, "Failed to create memory_vec table");
                }

                // Chunked attachment text for semantic search (same embedding dimension as memory)
                self.conn.execute(
                    "CREATE TABLE IF NOT EXISTS attachment_doc_chunk (
                        id INTEGER PRIMARY KEY AUTOINCREMENT,
                        conversation_id TEXT NOT NULL,
                        attachment_uid TEXT NOT NULL,
                        file_name TEXT NOT NULL,
                        chunk_index INTEGER NOT NULL,
                        text TEXT NOT NULL,
                        content_hash TEXT NOT NULL
                    )",
                    [],
                )?;
                self.conn.execute(
                    "CREATE INDEX IF NOT EXISTS idx_attachment_chunk_conv ON attachment_doc_chunk(conversation_id)",
                    [],
                )?;
                self.conn.execute(
                    "CREATE INDEX IF NOT EXISTS idx_attachment_chunk_uid ON attachment_doc_chunk(conversation_id, attachment_uid)",
                    [],
                )?;

                let sql_doc = format!(
                    "CREATE VIRTUAL TABLE IF NOT EXISTS attachment_doc_vec USING vec0(embedding float[{}])",
                    dim
                );
                if let Err(e) = self.conn.execute(&sql_doc, []) {
                    tracing::warn!(error = %e, dim, "Failed to create attachment_doc_vec table");
                }
            }
        }

        Ok(())
    }

    /// Insert a new conversation
    pub fn insert_conversation(&self, title: &str) -> SqliteResult<String> {
        self.insert_conversation_with_profile(title, None)
    }

    /// Insert a new conversation with profile
    pub fn insert_conversation_with_profile(
        &self,
        title: &str,
        profile_name: Option<&str>,
    ) -> SqliteResult<String> {
        let id = Uuid::new_v4().to_string();
        let created_at = Utc::now().timestamp();

        self.conn.execute(
            "INSERT INTO conversations (id, title, created_at, title_generated, profile_name, last_message) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, title, created_at, 0, profile_name, None::<i64>],
        )?;

        Ok(id)
    }

    /// Insert a new message (returns rowid; use insert_message_with_metadata for full control)
    pub fn insert_message(
        &self,
        conversation_id: &str,
        role: &str,
        content: &str,
        embedding: Option<&[f32]>,
    ) -> SqliteResult<i64> {
        self.insert_message_with_metadata(
            conversation_id,
            role,
            content,
            embedding,
            &MessageMetadata::default(),
        )
    }

    pub fn insert_message_with_metadata(
        &self,
        conversation_id: &str,
        role: &str,
        content: &str,
        embedding: Option<&[f32]>,
        metadata: &MessageMetadata<'_>,
    ) -> SqliteResult<i64> {
        let created_at = Utc::now().timestamp();

        // Convert embedding to bytes if provided
        let embedding_bytes = if let Some(emb) = embedding {
            Some(
                emb.iter()
                    .flat_map(|&f| f.to_le_bytes())
                    .collect::<Vec<u8>>(),
            )
        } else {
            None
        };

        let tool_calls_json = if let Some(calls) = metadata.tool_calls {
            Some(
                serde_json::to_string(calls)
                    .map_err(|e| rusqlite::Error::InvalidParameterName(e.to_string()))?,
            )
        } else {
            None
        };

        let tool_params_json = if let Some(params) = metadata.tool_params_json {
            Some(
                serde_json::to_string(params)
                    .map_err(|e| rusqlite::Error::InvalidParameterName(e.to_string()))?,
            )
        } else {
            None
        };

        let tool_result_json = if let Some(result) = metadata.tool_result_json {
            Some(
                serde_json::to_string(result)
                    .map_err(|e| rusqlite::Error::InvalidParameterName(e.to_string()))?,
            )
        } else {
            None
        };

        let attachments_json = if let Some(atts) = metadata.attachments {
            Some(
                serde_json::to_string(atts)
                    .map_err(|e| rusqlite::Error::InvalidParameterName(e.to_string()))?,
            )
        } else {
            None
        };

        self.conn.execute(
            "INSERT INTO messages (conversation_id, role, content, embedding, created_at, tool_calls, tool_call_id, tool_name, tool_status, tool_params_json, tool_result_json, reasoning_content, is_summary, is_summarized, summarized_message_ids, summarized_count, attachments_json) 
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
            params![
                conversation_id,
                role,
                content,
                embedding_bytes,
                created_at,
                tool_calls_json,
                metadata.tool_call_id,
                metadata.tool_name,
                metadata.tool_status,
                tool_params_json,
                tool_result_json,
                metadata.reasoning_content,
                0, // is_summary = false for regular messages
                0, // is_summarized = false for regular messages
                None::<String>, // summarized_message_ids
                None::<i64>, // summarized_count
                attachments_json,
            ],
        )?;

        let rowid = self.conn.last_insert_rowid();

        // Update conversation's last_message timestamp
        self.conn.execute(
            "UPDATE conversations SET last_message = ?1 WHERE id = ?2",
            params![created_at, conversation_id],
        )?;

        Ok(rowid)
    }

    /// Load all messages for a conversation
    pub fn load_conversation(&self, conversation_id: &str) -> SqliteResult<Vec<Message>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, conversation_id, role, content, embedding, created_at, tool_calls, tool_call_id, tool_name, tool_status, tool_params_json, tool_result_json, reasoning_content, is_summary, is_summarized, summarized_message_ids, summarized_count, attachments_json
             FROM messages 
             WHERE conversation_id = ?1 
             ORDER BY created_at ASC"
        )?;

        let message_iter = stmt.query_map(params![conversation_id], |row| {
            let embedding_bytes: Option<Vec<u8>> = row.get(4)?;
            let embedding = if let Some(bytes) = embedding_bytes {
                Some(
                    bytes
                        .chunks_exact(4)
                        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                        .collect(),
                )
            } else {
                None
            };

            let tool_calls_json: Option<String> = row.get(6)?;
            let tool_calls = tool_calls_json
                .as_deref()
                .map(|json| {
                    serde_json::from_str(json).map_err(|err| {
                        tracing::warn!("Failed to deserialize tool_calls JSON: {}", err);
                        err
                    })
                })
                .transpose()
                .unwrap_or(None);

            let tool_params_json = Self::read_json_value(row.get(10)?);
            let tool_result_json = Self::read_json_value(row.get(11)?);
            let reasoning_content: Option<String> = row.get(12)?;

            // Read summarization fields
            let is_summary: i64 = row.get(13).unwrap_or(0);
            let is_summarized: i64 = row.get(14).unwrap_or(0);
            let summarized_message_ids_json: Option<String> = row.get(15)?;
            let summarized_message_ids = summarized_message_ids_json
                .as_deref()
                .and_then(|json| serde_json::from_str::<Vec<i64>>(json).ok());
            let summarized_count: Option<i64> = row.get(16)?;
            let summarized_count_usize = summarized_count.map(|c| c as usize);

            let attachments_json: Option<String> = row.get(17)?;
            let attachments = attachments_json
                .as_deref()
                .and_then(|json| serde_json::from_str::<Vec<Attachment>>(json).ok());

            Ok(Message {
                id: row.get(0)?,
                conversation_id: row.get(1)?,
                role: row.get(2)?,
                content: row.get(3)?,
                embedding,
                created_at: row.get(5)?,
                tool_calls,
                tool_call_id: row.get(7)?,
                tool_name: row.get(8)?,
                tool_status: row.get(9)?,
                tool_params_json,
                tool_result_json,
                reasoning_content,
                is_summary: is_summary != 0,
                is_summarized: is_summarized != 0,
                summarized_message_ids,
                summarized_count: summarized_count_usize,
                attachments,
            })
        })?;

        let mut messages = Vec::new();
        for message in message_iter {
            messages.push(message?);
        }

        Ok(messages)
    }

    /// Search messages using FTS5
    pub fn search_history(&self, query: &str, limit: usize) -> SqliteResult<Vec<Snippet>> {
        let mut stmt = self.conn.prepare(
            "SELECT 
                m.conversation_id,
                m.content,
                m.created_at,
                rank
             FROM messages_fts fts
             JOIN messages m ON fts.rowid = m.id
             WHERE messages_fts MATCH ?1
             ORDER BY rank
             LIMIT ?2",
        )?;

        let snippet_iter = stmt.query_map(params![query, limit as i64], |row| {
            Ok(Snippet {
                conversation_id: row.get(0)?,
                content: row.get(1)?,
                timestamp: row.get(2)?,
                rank: row.get(3)?,
            })
        })?;

        let mut snippets = Vec::new();
        for snippet in snippet_iter {
            snippets.push(snippet?);
        }

        Ok(snippets)
    }

    /// Update conversation title
    pub fn update_title(&self, conversation_id: &str, title: &str) -> SqliteResult<bool> {
        let changes = self.conn.execute(
            "UPDATE conversations SET title = ?1 WHERE id = ?2",
            params![title, conversation_id],
        )?;

        Ok(changes > 0)
    }

    /// Update conversation profile
    pub fn update_profile(
        &self,
        conversation_id: &str,
        profile_name: Option<&str>,
    ) -> SqliteResult<bool> {
        let changes = self.conn.execute(
            "UPDATE conversations SET profile_name = ?1 WHERE id = ?2",
            params![profile_name, conversation_id],
        )?;

        Ok(changes > 0)
    }

    /// Insert a summary message (replaces old messages)
    pub fn insert_summary_message(
        &self,
        conversation_id: &str,
        summary_content: &str,
        summarized_message_ids: &[i64],
        earliest_timestamp: i64,
    ) -> SqliteResult<()> {
        let summarized_count = summarized_message_ids.len();
        let summarized_ids_json = serde_json::to_string(summarized_message_ids)
            .map_err(|e| rusqlite::Error::InvalidParameterName(e.to_string()))?;

        self.conn.execute(
            "INSERT INTO messages (conversation_id, role, content, created_at, is_summary, summarized_message_ids, summarized_count) 
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                conversation_id,
                "system", // Summary messages use "system" role
                summary_content,
                earliest_timestamp,
                1, // is_summary = true
                summarized_ids_json,
                summarized_count as i64,
            ],
        )?;

        // Update conversation's last_message timestamp to now (when summary is created)
        let now = Utc::now().timestamp();
        self.conn.execute(
            "UPDATE conversations SET last_message = ?1 WHERE id = ?2",
            params![now, conversation_id],
        )?;

        Ok(())
    }

    /// Delete messages by IDs (used during summarization)
    pub fn delete_messages(&self, message_ids: &[i64]) -> SqliteResult<usize> {
        if message_ids.is_empty() {
            return Ok(0);
        }

        // Build placeholders for IN clause
        let placeholders: String = message_ids
            .iter()
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(",");

        let sql = format!("DELETE FROM messages WHERE id IN ({})", placeholders);
        let changes = self
            .conn
            .execute(&sql, rusqlite::params_from_iter(message_ids.iter()))?;

        Ok(changes)
    }

    /// Truncate conversation by rowid: delete all messages including and AFTER the specified rowid
    /// This is called from storage_wrapper after UUID matching
    pub fn truncate_conversation_by_rowid(
        &self,
        conversation_id: &str,
        target_rowid: i64,
    ) -> SqliteResult<usize> {
        tracing::debug!(
            "Truncating conversation {} from rowid {} (inclusive)",
            conversation_id,
            target_rowid
        );

        // Delete all messages in this conversation with id >= target_rowid
        // (since id is rowid and messages are inserted in order)
        let changes = self.conn.execute(
            "DELETE FROM messages WHERE conversation_id = ?1 AND id >= ?2",
            params![conversation_id, target_rowid],
        )?;

        tracing::debug!(
            "Deleted {} messages from rowid {} (inclusive)",
            changes,
            target_rowid
        );
        Ok(changes)
    }

    /// Legacy truncate - deprecated, use truncate_conversation_by_rowid via storage_wrapper
    #[allow(dead_code)]
    pub fn truncate_conversation(
        &self,
        _conversation_id: &str,
        _message_id: &str,
    ) -> SqliteResult<usize> {
        // This should not be called - truncate should go through storage_wrapper which handles UUID matching
        Ok(0)
    }

    /// Perform summarization: mark old messages as summarized and insert summary
    pub fn perform_summarization(
        &self,
        conversation_id: &str,
        messages_to_summarize: &[Message],
        summary_content: &str,
    ) -> SqliteResult<()> {
        let transaction = self.conn.unchecked_transaction()?;

        // Collect IDs of messages to be marked as summarized
        let message_ids: Vec<i64> = messages_to_summarize.iter().map(|m| m.id).collect();
        let summarized_count = message_ids.len();

        if summarized_count == 0 {
            return Ok(());
        }

        // Timestamp for the summary message and conversation update (time of summarization)
        let now = Utc::now().timestamp();

        // Mark messages as summarized instead of deleting them
        let placeholders: String = message_ids
            .iter()
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(",");
        let update_sql = format!(
            "UPDATE messages SET is_summarized = 1 WHERE id IN ({})",
            placeholders
        );
        transaction.execute(&update_sql, rusqlite::params_from_iter(message_ids.iter()))?;

        // Insert the summary message
        let summarized_ids_json = serde_json::to_string(&message_ids)
            .map_err(|e| rusqlite::Error::InvalidParameterName(e.to_string()))?;

        transaction.execute(
            "INSERT INTO messages (conversation_id, role, content, created_at, is_summary, is_summarized, summarized_message_ids, summarized_count) 
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                conversation_id,
                "system",
                summary_content,
                now,
                1, // is_summary = true
                0, // is_summarized = false (summary messages are not themselves summarized)
                summarized_ids_json,
                summarized_count as i64,
            ],
        )?;

        // Update conversation's last_message timestamp to now (when summary is created)
        transaction.execute(
            "UPDATE conversations SET last_message = ?1 WHERE id = ?2",
            params![now, conversation_id],
        )?;

        transaction.commit()?;
        Ok(())
    }

    /// Get conversation by ID
    pub fn get_conversation(&self, conversation_id: &str) -> SqliteResult<Option<Conversation>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, title, created_at, title_generated, profile_name, last_message FROM conversations WHERE id = ?1")?;

        stmt.query_row(params![conversation_id], |row| {
            Ok(Conversation {
                id: row.get(0)?,
                title: row.get(1)?,
                created_at: row.get(2)?,
                title_generated: row.get::<_, i32>(3)? != 0,
                profile_name: row.get(4)?,
                last_message: row.get(5)?,
            })
        })
        .optional()
    }

    /// List all conversations ordered by creation date (newest first)
    pub fn list_conversations(&self) -> SqliteResult<Vec<Conversation>> {
        self.list_conversations_paginated(None, None)
    }

    /// List conversations with pagination (offset, limit)
    pub fn list_conversations_paginated(
        &self,
        offset: Option<usize>,
        limit: Option<usize>,
    ) -> SqliteResult<Vec<Conversation>> {
        // Order by last_message DESC first (most recently updated), then created_at DESC
        // Conversations with NULL last_message (no messages yet) appear at the end
        let mut query = "SELECT id, title, created_at, title_generated, profile_name, last_message FROM conversations ORDER BY (last_message IS NULL), last_message DESC, created_at DESC".to_string();

        if let Some(lim) = limit {
            query.push_str(&format!(" LIMIT {}", lim));
            if let Some(off) = offset {
                query.push_str(&format!(" OFFSET {}", off));
            }
        }

        let mut stmt = self.conn.prepare(&query)?;

        let conversation_iter = stmt.query_map([], |row| {
            Ok(Conversation {
                id: row.get(0)?,
                title: row.get(1)?,
                created_at: row.get(2)?,
                title_generated: row.get::<_, i32>(3)? != 0,
                profile_name: row.get(4)?,
                last_message: row.get(5)?,
            })
        })?;

        let mut conversations = Vec::new();
        for conversation in conversation_iter {
            conversations.push(conversation?);
        }

        Ok(conversations)
    }

    /// Delete a conversation and all its messages
    pub fn delete_conversation(&self, conversation_id: &str) -> SqliteResult<bool> {
        let changes = self.conn.execute(
            "DELETE FROM conversations WHERE id = ?1",
            params![conversation_id],
        )?;

        Ok(changes > 0)
    }

    /// Get the database connection (for advanced operations)
    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    /// Get conversations without generated titles (only those that have at least one message)
    pub fn get_conversations_without_title(&self) -> SqliteResult<Vec<Conversation>> {
        let mut stmt = self.conn.prepare(
            "SELECT c.id, c.title, c.created_at, c.title_generated, c.profile_name, c.last_message
                 FROM conversations c
                 WHERE c.title_generated = 0
                   AND EXISTS (SELECT 1 FROM messages m WHERE m.conversation_id = c.id)
                 ORDER BY c.created_at ASC",
        )?;

        let conversation_iter = stmt.query_map([], |row| {
            Ok(Conversation {
                id: row.get(0)?,
                title: row.get(1)?,
                created_at: row.get(2)?,
                title_generated: row.get::<_, i32>(3)? != 0,
                profile_name: row.get(4)?,
                last_message: row.get(5)?,
            })
        })?;

        let mut conversations = Vec::new();
        for conversation in conversation_iter {
            conversations.push(conversation?);
        }

        Ok(conversations)
    }

    /// Update conversation title and set title_generated flag
    pub fn update_conversation_title_and_flag(
        &self,
        conversation_id: &str,
        title: &str,
    ) -> SqliteResult<bool> {
        let changes = self.conn.execute(
            "UPDATE conversations SET title = ?1, title_generated = 1 WHERE id = ?2",
            params![title, conversation_id],
        )?;

        Ok(changes > 0)
    }

    // ── Long-term memory methods (ported from mcp_luna_memory) ──

    /// Store a memory entry. Returns the new entry with its assigned ID.
    pub fn store_memory(
        &self,
        content: &str,
        category: Option<&str>,
        importance: Option<i32>,
    ) -> SqliteResult<MemoryEntry> {
        let importance = importance.unwrap_or(5);
        let now = Utc::now().timestamp();
        self.conn.execute(
            "INSERT INTO memory (content, category, importance, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![content, category, importance, now, now],
        )?;
        let id = self.conn.last_insert_rowid();
        Ok(MemoryEntry {
            id,
            content: content.to_string(),
            category: category.map(|s| s.to_string()),
            importance,
            created_at: now,
            updated_at: Some(now),
        })
    }

    /// Search memory via FTS5 full-text search. Keywords are OR-joined.
    pub fn search_memory(
        &self,
        keywords: &[String],
        limit: usize,
    ) -> SqliteResult<Vec<MemoryEntry>> {
        let fts_query: String = keywords
            .iter()
            .filter(|s| !s.is_empty())
            .cloned()
            .collect::<Vec<_>>()
            .join(" OR ");
        if fts_query.is_empty() {
            return Ok(Vec::new());
        }
        let mut stmt = self.conn.prepare(
            "SELECT m.id, m.content, m.category, m.importance, m.created_at, m.updated_at
             FROM memory m
             JOIN memory_fts ON m.id = memory_fts.rowid
             WHERE memory_fts MATCH ?1
             ORDER BY bm25(memory_fts) ASC
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![fts_query, limit as i64], |row| {
            Ok(MemoryEntry {
                id: row.get(0)?,
                content: row.get(1)?,
                category: row.get(2)?,
                importance: row.get(3)?,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
            })
        })?;
        rows.collect()
    }

    /// Search memory entries by category, ordered by importance then recency.
    pub fn search_memory_by_category(
        &self,
        category: &str,
        limit: usize,
    ) -> SqliteResult<Vec<MemoryEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, content, category, importance, created_at, updated_at
             FROM memory
             WHERE category = ?1
             ORDER BY importance DESC, created_at DESC
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![category, limit as i64], |row| {
            Ok(MemoryEntry {
                id: row.get(0)?,
                content: row.get(1)?,
                category: row.get(2)?,
                importance: row.get(3)?,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
            })
        })?;
        rows.collect()
    }

    /// Delete a memory entry by ID. Returns true if a row was deleted.
    pub fn delete_memory(&self, memory_id: i64) -> SqliteResult<bool> {
        if self.embedding_dimension.is_some() {
            let _ = self.conn.execute(
                "DELETE FROM memory_vec WHERE rowid = ?1",
                params![memory_id],
            );
        }
        let changes = self
            .conn
            .execute("DELETE FROM memory WHERE id = ?1", params![memory_id])?;
        Ok(changes > 0)
    }

    /// Insert a row into memory_vec (vector index). Call after store_memory when embedding is enabled.
    pub fn insert_memory_vec_row(&self, memory_id: i64, embedding: &[f32]) -> SqliteResult<()> {
        if self.embedding_dimension.is_none() {
            return Ok(());
        }
        let json = serde_json::to_string(embedding)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
        self.conn.execute(
            "INSERT INTO memory_vec(rowid, embedding) VALUES (?1, ?2)",
            params![memory_id, json],
        )?;
        Ok(())
    }

    /// Update a row in memory_vec. Call after update_memory when embedding is enabled.
    pub fn update_memory_vec_row(&self, memory_id: i64, embedding: &[f32]) -> SqliteResult<()> {
        if self.embedding_dimension.is_none() {
            return Ok(());
        }
        let _ = self.conn.execute(
            "DELETE FROM memory_vec WHERE rowid = ?1",
            params![memory_id],
        );
        self.insert_memory_vec_row(memory_id, embedding)
    }

    /// Delete all rows from memory_vec. Used when reorganizing/rebuilding the vector index.
    pub fn delete_all_memory_vec_rows(&self) -> SqliteResult<()> {
        if self.embedding_dimension.is_none() {
            return Ok(());
        }
        self.conn.execute("DELETE FROM memory_vec", [])?;
        Ok(())
    }

    /// Search memories by vector similarity (KNN). Returns MemoryEntry sorted by distance.
    /// If max_distance is Some, only returns entries with distance <= max_distance.
    pub fn search_memory_by_vector(
        &self,
        embedding: &[f32],
        limit: usize,
        max_distance: Option<f32>,
    ) -> SqliteResult<Vec<MemoryEntry>> {
        if self.embedding_dimension.is_none() {
            return Ok(Vec::new());
        }

        let json = serde_json::to_string(embedding)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;

        let validated_max_distance = match max_distance {
            Some(dist) => {
                if !dist.is_finite() || dist < 0.0 || dist > 2.0 {
                    tracing::warn!(
                        max_distance = dist,
                        "Invalid max_distance, ignoring filter (must be 0.0-2.0)"
                    );
                    None
                } else {
                    Some(dist)
                }
            }
            None => None,
        };

        let sql = match validated_max_distance {
            Some(max_dist) => {
                format!(
                    "SELECT rowid, distance FROM memory_vec WHERE embedding MATCH ?1 AND distance <= {} ORDER BY distance LIMIT ?2",
                    max_dist
                )
            }
            None => {
                "SELECT rowid, distance FROM memory_vec WHERE embedding MATCH ?1 ORDER BY distance LIMIT ?2".to_string()
            }
        };

        let mut stmt = self.conn.prepare(&sql)?;
        let rowids: Vec<i64> = stmt
            .query_map(params![json, limit as i64], |row| Ok(row.get::<_, i64>(0)?))?
            .collect::<Result<Vec<_>, _>>()?;
        if rowids.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders = std::iter::repeat("?")
            .take(rowids.len())
            .collect::<Vec<_>>()
            .join(", ");
        let sql2 = format!(
            "SELECT id, content, category, importance, created_at, updated_at FROM memory WHERE id IN ({})",
            placeholders
        );
        let mut stmt2 = self.conn.prepare(&sql2)?;
        let entries: Vec<MemoryEntry> = stmt2
            .query_map(rusqlite::params_from_iter(rowids.iter().copied()), |row| {
                Ok(MemoryEntry {
                    id: row.get(0)?,
                    content: row.get(1)?,
                    category: row.get(2)?,
                    importance: row.get(3)?,
                    created_at: row.get(4)?,
                    updated_at: row.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(entries)
    }

    /// Remove indexed chunks for one uploaded file (before re-index).
    pub fn delete_attachment_doc_chunks_for(
        &self,
        conversation_id: &str,
        attachment_uid: &str,
    ) -> SqliteResult<()> {
        if self.embedding_dimension.is_none() {
            return Ok(());
        }
        let mut stmt = self.conn.prepare(
            "SELECT id FROM attachment_doc_chunk WHERE conversation_id = ?1 AND attachment_uid = ?2",
        )?;
        let ids: Vec<i64> = stmt
            .query_map(params![conversation_id, attachment_uid], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;
        for id in ids {
            let _ = self
                .conn
                .execute("DELETE FROM attachment_doc_vec WHERE rowid = ?1", params![id]);
        }
        self.conn.execute(
            "DELETE FROM attachment_doc_chunk WHERE conversation_id = ?1 AND attachment_uid = ?2",
            params![conversation_id, attachment_uid],
        )?;
        Ok(())
    }

    /// Insert one text chunk and its embedding row (rowid = chunk id).
    pub fn insert_attachment_doc_chunk_with_embedding(
        &self,
        conversation_id: &str,
        attachment_uid: &str,
        file_name: &str,
        chunk_index: i32,
        text: &str,
        content_hash: &str,
        embedding: &[f32],
    ) -> SqliteResult<()> {
        if self.embedding_dimension.is_none() {
            return Ok(());
        }
        self.conn.execute(
            "INSERT INTO attachment_doc_chunk (conversation_id, attachment_uid, file_name, chunk_index, text, content_hash) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                conversation_id,
                attachment_uid,
                file_name,
                chunk_index,
                text,
                content_hash
            ],
        )?;
        let rowid = self.conn.last_insert_rowid();
        let json = serde_json::to_string(embedding)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
        self.conn.execute(
            "INSERT INTO attachment_doc_vec(rowid, embedding) VALUES (?1, ?2)",
            params![rowid, json],
        )?;
        Ok(())
    }

    /// Semantic search over attachment chunks in one conversation.
    pub fn search_attachment_chunks_by_vector(
        &self,
        conversation_id: &str,
        embedding: &[f32],
        limit: usize,
        max_distance: Option<f32>,
    ) -> SqliteResult<Vec<AttachmentDocSearchHit>> {
        if self.embedding_dimension.is_none() {
            return Ok(Vec::new());
        }

        let json = serde_json::to_string(embedding)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;

        let validated_max_distance = match max_distance {
            Some(dist) => {
                if !dist.is_finite() || dist < 0.0 || dist > 2.0 {
                    None
                } else {
                    Some(dist)
                }
            }
            None => None,
        };

        let fetch_limit = ((limit as i64).saturating_mul(8)).max(limit as i64);
        let sql = match validated_max_distance {
            Some(max_dist) => format!(
                "SELECT rowid, distance FROM attachment_doc_vec WHERE embedding MATCH ?1 AND distance <= {} ORDER BY distance LIMIT ?2",
                max_dist
            ),
            None => {
                "SELECT rowid, distance FROM attachment_doc_vec WHERE embedding MATCH ?1 ORDER BY distance LIMIT ?2"
                    .to_string()
            }
        };

        let mut stmt = self.conn.prepare(&sql)?;
        let candidates: Vec<(i64, f32)> = stmt
            .query_map(params![json, fetch_limit], |row| {
                let dist: f64 = row.get(1)?;
                Ok((row.get::<_, i64>(0)?, dist as f32))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let mut hits = Vec::new();
        for (rowid, distance) in candidates {
            let row_result = self.conn.query_row(
                "SELECT attachment_uid, file_name, chunk_index, text FROM attachment_doc_chunk WHERE id = ?1 AND conversation_id = ?2",
                params![rowid, conversation_id],
                |row| {
                    Ok(AttachmentDocSearchHit {
                        attachment_uid: row.get(0)?,
                        file_name: row.get(1)?,
                        chunk_index: row.get(2)?,
                        text: row.get(3)?,
                        distance,
                    })
                },
            );
            if let Ok(h) = row_result {
                hits.push(h);
                if hits.len() >= limit {
                    break;
                }
            }
        }
        Ok(hits)
    }

    /// List all memory entries, ordered by importance then recency.
    pub fn list_memory(&self, limit: usize) -> SqliteResult<Vec<MemoryEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, content, category, importance, created_at, updated_at
             FROM memory
             ORDER BY importance DESC, created_at DESC
             LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], |row| {
            Ok(MemoryEntry {
                id: row.get(0)?,
                content: row.get(1)?,
                category: row.get(2)?,
                importance: row.get(3)?,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
            })
        })?;
        rows.collect()
    }

    /// Update a memory entry's content, category, importance, and set updated_at = now.
    pub fn update_memory(
        &self,
        memory_id: i64,
        content: &str,
        category: Option<&str>,
        importance: i32,
    ) -> SqliteResult<bool> {
        let now = Utc::now().timestamp();
        // Read old content first so we can properly delete the old FTS entry
        let old_content: Option<String> = self
            .conn
            .query_row(
                "SELECT content FROM memory WHERE id = ?1",
                params![memory_id],
                |row| row.get(0),
            )
            .ok();

        let changes = self.conn.execute(
            "UPDATE memory SET content = ?1, category = ?2, importance = ?3, updated_at = ?4 WHERE id = ?5",
            params![content, category, importance, now, memory_id],
        )?;
        if changes > 0 {
            // Manually sync FTS: delete old entry, insert new
            if let Some(old) = old_content {
                let _ = self.conn.execute(
                    "INSERT INTO memory_fts(memory_fts, rowid, content) VALUES('delete', ?1, ?2)",
                    params![memory_id, old],
                );
            }
            let _ = self.conn.execute(
                "INSERT INTO memory_fts(rowid, content) VALUES(?1, ?2)",
                params![memory_id, content],
            );
        }
        Ok(changes > 0)
    }

    /// Get a deep sleep state value by key.
    pub fn get_deep_sleep_state(&self, key: &str) -> SqliteResult<Option<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT value FROM deep_sleep_state WHERE key = ?1")?;
        let result = stmt.query_row(params![key], |row| row.get(0));
        match result {
            Ok(val) => Ok(Some(val)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Set a deep sleep state value (upsert).
    pub fn set_deep_sleep_state(&self, key: &str, value: &str) -> SqliteResult<()> {
        self.conn.execute(
            "INSERT INTO deep_sleep_state (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    /// Get conversations that have at least one message with id > message_id.
    /// Returns up to `limit` conversations, ordered by oldest first (ASC) so the
    /// watermark advances naturally through the backlog.
    pub fn get_conversations_with_messages_after(
        &self,
        message_id: i64,
        limit: usize,
    ) -> SqliteResult<Vec<Conversation>> {
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT c.id, c.title, c.created_at, c.title_generated, c.profile_name, c.last_message
             FROM conversations c
             JOIN messages m ON m.conversation_id = c.id
             WHERE m.id > ?1
             ORDER BY c.last_message ASC
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![message_id, limit as i64], |row| {
            Ok(Conversation {
                id: row.get(0)?,
                title: row.get(1)?,
                created_at: row.get(2)?,
                title_generated: row.get::<_, i32>(3)? != 0,
                profile_name: row.get(4)?,
                last_message: row.get(5)?,
            })
        })?;
        rows.collect()
    }

    /// Get the maximum message ID in the database.
    pub fn get_max_message_id(&self) -> SqliteResult<i64> {
        self.conn
            .query_row("SELECT COALESCE(MAX(id), 0) FROM messages", [], |row| {
                row.get(0)
            })
    }

    /// Record that the given memories were recalled (injected) in this conversation.
    pub fn record_memory_recalls(
        &self,
        conversation_id: &str,
        memory_ids: &[i64],
    ) -> SqliteResult<()> {
        if memory_ids.is_empty() {
            return Ok(());
        }
        let now = Utc::now().timestamp();
        let mut stmt = self.conn.prepare(
            "INSERT OR IGNORE INTO conversation_memory_recalls (conversation_id, memory_id, recalled_at) VALUES (?1, ?2, ?3)",
        )?;
        for &mid in memory_ids {
            stmt.execute(params![conversation_id, mid, now])?;
        }
        Ok(())
    }

    /// Get memory IDs that were previously recalled in this conversation (for dedup across restarts).
    pub fn get_recalled_memory_ids(&self, conversation_id: &str) -> SqliteResult<Vec<i64>> {
        let mut stmt = self.conn.prepare(
            "SELECT memory_id FROM conversation_memory_recalls WHERE conversation_id = ?1 ORDER BY recalled_at ASC",
        )?;
        let rows = stmt.query_map(params![conversation_id], |row| row.get(0))?;
        rows.collect()
    }

    /// Insert a scheduled job
    pub fn insert_scheduled_job(&self, job: &ScheduledJob) -> SqliteResult<()> {
        self.conn.execute(
            "INSERT INTO scheduled_jobs (id, conversation_id, run_at_utc_secs, message, profile_name, title, status, created_at_utc_secs, updated_at_utc_secs, error_message, schedule) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                job.id,
                job.conversation_id,
                job.run_at_utc_secs,
                job.message,
                job.profile_name,
                job.title,
                job.status,
                job.created_at_utc_secs,
                job.updated_at_utc_secs,
                job.error_message,
                job.schedule,
            ],
        )?;
        Ok(())
    }

    /// Get due scheduled jobs (pending, run_at <= now)
    pub fn get_due_scheduled_jobs(
        &self,
        now_utc_secs: i64,
        limit: u32,
    ) -> SqliteResult<Vec<ScheduledJob>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, conversation_id, run_at_utc_secs, message, profile_name, title, status, created_at_utc_secs, updated_at_utc_secs, error_message, schedule FROM scheduled_jobs WHERE status = 'pending' AND run_at_utc_secs <= ?1 ORDER BY run_at_utc_secs ASC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![now_utc_secs, limit as i64], |row| {
            Ok(ScheduledJob {
                id: row.get(0)?,
                conversation_id: row.get(1)?,
                run_at_utc_secs: row.get(2)?,
                message: row.get(3)?,
                profile_name: row.get(4)?,
                title: row.get(5)?,
                status: row.get(6)?,
                created_at_utc_secs: row.get(7)?,
                updated_at_utc_secs: row.get(8)?,
                error_message: row.get(9)?,
                schedule: row.get(10)?,
            })
        })?;
        rows.collect()
    }

    /// Mark job as running; returns true if the row was updated (idempotent take)
    pub fn set_scheduled_job_running(&self, id: &str, now_utc_secs: i64) -> SqliteResult<bool> {
        let changes = self.conn.execute(
            "UPDATE scheduled_jobs SET status = 'running', updated_at_utc_secs = ?1 WHERE id = ?2 AND status = 'pending'",
            params![now_utc_secs, id],
        )?;
        Ok(changes > 0)
    }

    /// Mark job completed or failed
    pub fn set_scheduled_job_completed(
        &self,
        id: &str,
        now_utc_secs: i64,
        failed: bool,
        error_message: Option<&str>,
    ) -> SqliteResult<()> {
        let status = if failed { "failed" } else { "completed" };
        self.conn.execute(
            "UPDATE scheduled_jobs SET status = ?1, updated_at_utc_secs = ?2, error_message = ?3 WHERE id = ?4",
            params![status, now_utc_secs, error_message, id],
        )?;
        Ok(())
    }

    /// Set next run for recurring job and set status back to pending
    pub fn set_scheduled_job_next_run(
        &self,
        id: &str,
        next_run_utc_secs: i64,
        now_utc_secs: i64,
    ) -> SqliteResult<()> {
        self.conn.execute(
            "UPDATE scheduled_jobs SET run_at_utc_secs = ?1, status = 'pending', updated_at_utc_secs = ?2 WHERE id = ?3",
            params![next_run_utc_secs, now_utc_secs, id],
        )?;
        Ok(())
    }

    /// Delete a scheduled job by id. Returns true if a row was deleted.
    pub fn delete_scheduled_job(&self, id: &str) -> SqliteResult<bool> {
        let changes = self
            .conn
            .execute("DELETE FROM scheduled_jobs WHERE id = ?1", params![id])?;
        Ok(changes > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_sqlite_storage() -> SqliteResult<()> {
        // Create a temporary database
        let temp_dir = std::env::temp_dir();
        let db_path = temp_dir.join("test_cosmic_llm.db");

        // Remove existing test database
        let _ = fs::remove_file(&db_path);

        // Create storage
        let storage = SqliteStorage::new(&db_path)?;

        // Test conversation creation
        let conv_id = storage.insert_conversation("Test Conversation")?;
        assert!(!conv_id.is_empty());

        // Test message insertion
        storage.insert_message(&conv_id, "user", "Hello, world!", None)?;
        storage.insert_message(&conv_id, "assistant", "Hi there!", None)?;

        // Test loading conversation
        let messages = storage.load_conversation(&conv_id)?;
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, "user");
        assert_eq!(messages[0].content, "Hello, world!");
        assert_eq!(messages[1].role, "assistant");
        assert_eq!(messages[1].content, "Hi there!");

        // Test search
        let snippets = storage.search_history("Hello", 10)?;
        assert_eq!(snippets.len(), 1);
        assert_eq!(snippets[0].content, "Hello, world!");

        // Test title update
        let updated = storage.update_title(&conv_id, "Updated Title")?;
        assert!(updated);

        // Test conversation retrieval
        let conversation = storage.get_conversation(&conv_id)?;
        let conv = conversation.ok_or_else(|| {
            rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_INTERNAL),
                Some("Conversation should exist after update".to_string()),
            )
        })?;
        assert_eq!(conv.title, "Updated Title");

        // Test conversation listing
        let conversations = storage.list_conversations()?;
        assert_eq!(conversations.len(), 1);

        // Test conversation deletion
        let deleted = storage.delete_conversation(&conv_id)?;
        assert!(deleted);

        // Verify deletion
        let conversations_after = storage.list_conversations()?;
        assert_eq!(conversations_after.len(), 0);

        // Clean up
        let _ = fs::remove_file(&db_path);

        Ok(())
    }

    #[test]
    fn test_embedding_storage() -> SqliteResult<()> {
        let temp_dir = std::env::temp_dir();
        let db_path = temp_dir.join("test_embeddings.db");
        let _ = fs::remove_file(&db_path);

        let storage = SqliteStorage::new(&db_path)?;
        let conv_id = storage.insert_conversation("Embedding Test")?;

        // Test with embedding
        let embedding = vec![0.1, 0.2, 0.3, 0.4];
        storage.insert_message(&conv_id, "user", "Test with embedding", Some(&embedding))?;

        let messages = storage.load_conversation(&conv_id)?;
        assert_eq!(messages.len(), 1);
        let msg_embedding = messages[0].embedding.as_ref().ok_or_else(|| {
            rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_INTERNAL),
                Some("Message embedding should exist".to_string()),
            )
        })?;
        assert_eq!(msg_embedding, &embedding);

        let _ = fs::remove_file(&db_path);
        Ok(())
    }

    #[test]
    fn wal_mode_enabled_when_requested() -> SqliteResult<()> {
        let db_path = std::env::temp_dir().join(format!("wal_mode_test_{}.db", Uuid::new_v4()));
        let settings = SqliteSettings {
            wal_enabled: true,
            wal_autocheckpoint: 5,
            busy_timeout_ms: 1_000,
            embedding_dimension: None,
        };
        let storage = SqliteStorage::new_with_settings(&db_path, &settings)?;
        let mode: String = storage
            .conn
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))?;
        assert_eq!(mode.to_lowercase(), "wal");
        let _ = fs::remove_file(&db_path);
        Ok(())
    }
}

pub struct MessageMetadata<'a> {
    pub tool_calls: Option<&'a [ToolCall]>,
    pub tool_call_id: Option<&'a str>,
    pub tool_name: Option<&'a str>,
    pub tool_status: Option<&'a str>,
    pub tool_params_json: Option<&'a Value>,
    pub tool_result_json: Option<&'a Value>,
    pub reasoning_content: Option<&'a str>, // For DeepSeek thinking/reasoning content
    pub attachments: Option<&'a [Attachment]>,
}

impl<'a> Default for MessageMetadata<'a> {
    fn default() -> Self {
        Self {
            tool_calls: None,
            tool_call_id: None,
            tool_name: None,
            tool_status: None,
            tool_params_json: None,
            tool_result_json: None,
            reasoning_content: None,
            attachments: None,
        }
    }
}

impl SqliteStorage {
    fn read_json_value(raw: Option<String>) -> Option<Value> {
        raw.and_then(|json| match serde_json::from_str(&json) {
            Ok(value) => Some(value),
            Err(err) => {
                tracing::warn!("Failed to deserialize JSON payload: {}", err);
                None
            }
        })
    }
}
