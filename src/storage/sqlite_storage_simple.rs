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
                created_at INTEGER NOT NULL
            )",
            [],
        )?;

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
                FOREIGN KEY (conversation_id) REFERENCES conversations (id) ON DELETE CASCADE
            )",
            [],
        )?;

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
        let id = Uuid::new_v4().to_string();
        let created_at = Utc::now().timestamp();

        self.conn.execute(
            "INSERT INTO conversations (id, title, created_at) VALUES (?1, ?2, ?3)",
            params![id, title, created_at],
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
            "INSERT INTO messages (conversation_id, role, content, embedding, created_at, tool_calls, tool_call_id, tool_name, tool_status, tool_params_json, tool_result_json) 
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
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
                tool_result_json
            ],
        )?;

        Ok(())
    }

    /// Load all messages for a conversation
    pub fn load_conversation(&self, conversation_id: &str) -> SqliteResult<Vec<Message>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, conversation_id, role, content, embedding, created_at, tool_calls, tool_call_id, tool_name, tool_status, tool_params_json, tool_result_json 
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

    /// Get conversation by ID
    pub fn get_conversation(&self, conversation_id: &str) -> SqliteResult<Option<Conversation>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, title, created_at FROM conversations WHERE id = ?1")?;

        stmt.query_row(params![conversation_id], |row| {
            Ok(Conversation {
                id: row.get(0)?,
                title: row.get(1)?,
                created_at: row.get(2)?,
            })
        })
        .optional()
    }

    /// List all conversations ordered by creation date (newest first)
    pub fn list_conversations(&self) -> SqliteResult<Vec<Conversation>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, title, created_at FROM conversations ORDER BY created_at DESC")?;

        let conversation_iter = stmt.query_map([], |row| {
            Ok(Conversation {
                id: row.get(0)?,
                title: row.get(1)?,
                created_at: row.get(2)?,
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
