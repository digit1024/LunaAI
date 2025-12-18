use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension, Result as SqliteResult};
use serde::{Deserialize, Serialize};
use serde_json::{self, Value};
use std::path::Path;
use std::time::Duration;
use uuid::Uuid;

use crate::{config::ServerConfig, llm::ToolCall};

/// Represents a conversation in the database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub id: String,
    pub title: String,
    pub created_at: i64,
    pub title_generated: bool,
    pub profile_name: Option<String>,
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
    pub is_summarized: bool, // True if this message has been summarized (should be excluded from LLM payload)
    pub summarized_message_ids: Option<Vec<i64>>, // IDs of messages that were summarized
    pub summarized_count: Option<usize>, // Count of messages summarized
}

/// Represents a search snippet from FTS5
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snippet {
    pub conversation_id: String,
    pub content: String,
    pub timestamp: i64,
    pub rank: f64,
}

/// SQLite-based storage implementation
pub struct SqliteStorage {
    conn: Connection,
}

#[derive(Debug, Clone)]
pub struct SqliteSettings {
    pub wal_enabled: bool,
    pub wal_autocheckpoint: u32,
    pub busy_timeout_ms: u64,
}

impl Default for SqliteSettings {
    fn default() -> Self {
        Self {
            wal_enabled: true,
            wal_autocheckpoint: 200,
            busy_timeout_ms: 5_000,
        }
    }
}

impl From<&ServerConfig> for SqliteSettings {
    fn from(cfg: &ServerConfig) -> Self {
        Self {
            wal_enabled: cfg.wal_enabled,
            wal_autocheckpoint: cfg.wal_autocheckpoint,
            busy_timeout_ms: cfg.sqlite_busy_timeout_ms,
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
        let conn = Connection::open(db_path)?;
        Self::configure_connection(&conn, settings)?;
        let storage = Self { conn };
        storage.init_database()?;
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
    fn init_database(&self) -> SqliteResult<()> {
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
                profile_name TEXT
            )",
            [],
        )?;
        
        // Migrate existing conversations: add profile_name column if it doesn't exist
        let _ = self.conn.execute(
            "ALTER TABLE conversations ADD COLUMN profile_name TEXT",
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
        let _ = self.conn.execute(
            "ALTER TABLE messages ADD COLUMN reasoning_content TEXT",
            [],
        );

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

        Ok(())
    }

    /// Insert a new conversation
    pub fn insert_conversation(&self, title: &str) -> SqliteResult<String> {
        self.insert_conversation_with_profile(title, None)
    }
    
    /// Insert a new conversation with profile
    pub fn insert_conversation_with_profile(&self, title: &str, profile_name: Option<&str>) -> SqliteResult<String> {
        let id = Uuid::new_v4().to_string();
        let created_at = Utc::now().timestamp();

        self.conn.execute(
            "INSERT INTO conversations (id, title, created_at, title_generated, profile_name) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, title, created_at, 0, profile_name],
        )?;

        Ok(id)
    }

    /// Insert a new message
    pub fn insert_message(
        &self,
        conversation_id: &str,
        role: &str,
        content: &str,
        embedding: Option<&[f32]>,
    ) -> SqliteResult<()> {
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
    ) -> SqliteResult<()> {
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

        self.conn.execute(
            "INSERT INTO messages (conversation_id, role, content, embedding, created_at, tool_calls, tool_call_id, tool_name, tool_status, tool_params_json, tool_result_json, reasoning_content, is_summary, is_summarized, summarized_message_ids, summarized_count) 
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
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
            ],
        )?;

        Ok(())
    }

    /// Load all messages for a conversation
    pub fn load_conversation(&self, conversation_id: &str) -> SqliteResult<Vec<Message>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, conversation_id, role, content, embedding, created_at, tool_calls, tool_call_id, tool_name, tool_status, tool_params_json, tool_result_json, reasoning_content, is_summary, is_summarized, summarized_message_ids, summarized_count
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
                        log::warn!("Failed to deserialize tool_calls JSON: {}", err);
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

        let snippet_iter = stmt.query_map(params![query, limit], |row| {
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
    pub fn update_profile(&self, conversation_id: &str, profile_name: Option<&str>) -> SqliteResult<bool> {
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

        Ok(())
    }

    /// Delete messages by IDs (used during summarization)
    pub fn delete_messages(&self, message_ids: &[i64]) -> SqliteResult<usize> {
        if message_ids.is_empty() {
            return Ok(0);
        }

        // Build placeholders for IN clause
        let placeholders: String = message_ids.iter()
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(",");

        let sql = format!("DELETE FROM messages WHERE id IN ({})", placeholders);
        let changes = self.conn.execute(&sql, rusqlite::params_from_iter(message_ids.iter()))?;

        Ok(changes)
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

        // Get the earliest timestamp from messages being summarized
        let earliest_timestamp = messages_to_summarize
            .iter()
            .map(|m| m.created_at)
            .min()
            .unwrap_or_else(|| chrono::Utc::now().timestamp());

        // Mark messages as summarized instead of deleting them
        let placeholders: String = message_ids.iter()
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(",");
        let update_sql = format!("UPDATE messages SET is_summarized = 1 WHERE id IN ({})", placeholders);
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
                earliest_timestamp,
                1, // is_summary = true
                0, // is_summarized = false (summary messages are not themselves summarized)
                summarized_ids_json,
                summarized_count as i64,
            ],
        )?;

        transaction.commit()?;
        Ok(())
    }

    /// Get conversation by ID
    pub fn get_conversation(&self, conversation_id: &str) -> SqliteResult<Option<Conversation>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, title, created_at, title_generated, profile_name FROM conversations WHERE id = ?1")?;

        stmt.query_row(params![conversation_id], |row| {
            Ok(Conversation {
                id: row.get(0)?,
                title: row.get(1)?,
                created_at: row.get(2)?,
                title_generated: row.get::<_, i32>(3)? != 0,
                profile_name: row.get(4)?,
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
        let mut query = "SELECT id, title, created_at, title_generated, profile_name FROM conversations ORDER BY created_at DESC".to_string();
        
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

    /// Get conversations without generated titles
    pub fn get_conversations_without_title(&self) -> SqliteResult<Vec<Conversation>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, title, created_at, title_generated, profile_name FROM conversations WHERE title_generated = 0 ORDER BY created_at ASC")?;

        let conversation_iter = stmt.query_map([], |row| {
            Ok(Conversation {
                id: row.get(0)?,
                title: row.get(1)?,
                created_at: row.get(2)?,
                title_generated: row.get::<_, i32>(3)? != 0,
                profile_name: row.get(4)?,
            })
        })?;

        let mut conversations = Vec::new();
        for conversation in conversation_iter {
            conversations.push(conversation?);
        }

        Ok(conversations)
    }

    /// Update conversation title and set title_generated flag
    pub fn update_conversation_title_and_flag(&self, conversation_id: &str, title: &str) -> SqliteResult<bool> {
        let changes = self.conn.execute(
            "UPDATE conversations SET title = ?1, title_generated = 1 WHERE id = ?2",
            params![title, conversation_id],
        )?;

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
        assert!(conversation.is_some());
        assert_eq!(conversation.unwrap().title, "Updated Title");

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
        assert!(messages[0].embedding.is_some());
        assert_eq!(messages[0].embedding.as_ref().unwrap(), &embedding);

        let _ = fs::remove_file(&db_path);
        Ok(())
    }

    #[test]
    fn wal_mode_enabled_when_requested() -> SqliteResult<()> {
        let db_path = std::env::temp_dir()
            .join(format!("wal_mode_test_{}.db", Uuid::new_v4()));
        let settings = SqliteSettings {
            wal_enabled: true,
            wal_autocheckpoint: 5,
            busy_timeout_ms: 1_000,
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
        }
    }
}

impl SqliteStorage {
    fn read_json_value(raw: Option<String>) -> Option<Value> {
        raw.and_then(|json| match serde_json::from_str(&json) {
            Ok(value) => Some(value),
            Err(err) => {
                log::warn!("Failed to deserialize JSON payload: {}", err);
                None
            }
        })
    }
}
