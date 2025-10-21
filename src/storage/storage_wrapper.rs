use chrono::{DateTime, Utc};
use rusqlite::Result as SqliteResult;
use std::path::Path;
use uuid::Uuid;

use super::sqlite_storage_simple::SqliteStorage;
use super::conversation_storage::{Conversation as FileConversation, StoredMessage, Turn};

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

    /// Create a new storage instance with default database path
    pub fn new_default() -> SqliteResult<Self> {
        let db_path = Self::default_db_path();
        Self::new(db_path)
    }

    fn default_db_path() -> std::path::PathBuf {
        dirs::data_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("cosmic_llm")
            .join("conversations.db")
    }

    /// Create a new conversation
    pub fn create_conversation(&self, title: String) -> SqliteResult<Uuid> {
        let id_str = self.sqlite.insert_conversation(&title)?;
        Uuid::parse_str(&id_str)
            .map_err(|e| rusqlite::Error::InvalidParameterName(format!("Invalid UUID: {}", e)))
    }

    /// Get a conversation by ID
    pub fn get_conversation(&self, id: &Uuid) -> SqliteResult<Option<FileConversation>> {
        let id_str = id.to_string();
        if let Some(db_conv) = self.sqlite.get_conversation(&id_str)? {
            let messages = self.sqlite.load_conversation(&id_str)?;
            
            let stored_messages: Vec<StoredMessage> = messages.into_iter().map(|msg| {
                StoredMessage {
                    id: Uuid::parse_str(&msg.id.to_string()).unwrap_or_else(|_| Uuid::new_v4()),
                    role: msg.role,
                    content: msg.content,
                    timestamp: DateTime::from_timestamp(msg.created_at, 0).unwrap_or_else(Utc::now),
                }
            }).collect();

            let conversation = FileConversation {
                id: *id,
                title: db_conv.title,
                created_at: DateTime::from_timestamp(db_conv.created_at, 0).unwrap_or_else(Utc::now),
                updated_at: DateTime::from_timestamp(db_conv.created_at, 0).unwrap_or_else(Utc::now), // SQLite doesn't track updated_at yet
                messages: stored_messages,
                turns: Vec::new(), // Turns are not yet migrated to SQLite
            };

            Ok(Some(conversation))
        } else {
            Ok(None)
        }
    }

    /// Get a mutable reference to a conversation
    #[allow(dead_code)]
    pub fn get_conversation_mut(&mut self, _id: &Uuid) -> Option<&mut FileConversation> {
        // Note: This is not easily implementable with SQLite without loading all data
        // For now, return None - this method would need to be refactored in the calling code
        None
    }

    /// List all conversations
    pub fn list_conversations(&self) -> SqliteResult<Vec<FileConversation>> {
        let db_conversations = self.sqlite.list_conversations()?;
        let mut conversations = Vec::new();

        for db_conv in db_conversations {
            let id = Uuid::parse_str(&db_conv.id)
                .map_err(|e| rusqlite::Error::InvalidParameterName(format!("Invalid UUID: {}", e)))?;
            
            let messages = self.sqlite.load_conversation(&db_conv.id)?;
            let stored_messages: Vec<StoredMessage> = messages.into_iter().map(|msg| {
                StoredMessage {
                    id: Uuid::parse_str(&msg.id.to_string()).unwrap_or_else(|_| Uuid::new_v4()),
                    role: msg.role,
                    content: msg.content,
                    timestamp: DateTime::from_timestamp(msg.created_at, 0).unwrap_or_else(Utc::now),
                }
            }).collect();

            let conversation = FileConversation {
                id,
                title: db_conv.title,
                created_at: DateTime::from_timestamp(db_conv.created_at, 0).unwrap_or_else(Utc::now),
                updated_at: DateTime::from_timestamp(db_conv.created_at, 0).unwrap_or_else(Utc::now),
                messages: stored_messages,
                turns: Vec::new(), // Turns are not yet migrated to SQLite
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

    /// Add a message to a conversation
    pub fn add_message_to_conversation(&self, conversation_id: &Uuid, role: String, content: String) -> SqliteResult<()> {
        let id_str = conversation_id.to_string();
        self.sqlite.insert_message(&id_str, &role, &content, None)
    }

    /// Add a turn to a conversation (not yet implemented in SQLite)
    pub fn add_turn_to_conversation(&self, _conversation_id: &Uuid, _turn: Turn) -> SqliteResult<()> {
        // TODO: Implement turn storage in SQLite
        Ok(())
    }

    /// Delete a conversation
    pub fn delete_conversation(&self, conversation_id: &Uuid) -> SqliteResult<bool> {
        let id_str = conversation_id.to_string();
        self.sqlite.delete_conversation(&id_str)
    }

    /// Search conversation history
    pub fn search_history(&self, query: &str, limit: usize) -> SqliteResult<Vec<super::sqlite_storage_simple::Snippet>> {
        self.sqlite.search_history(query, limit)
    }

    /// List conversations from index (compatibility method)
    pub fn list_conversations_from_index(&self) -> SqliteResult<Vec<super::conversation_storage::ConversationIndex>> {
        let db_conversations = self.sqlite.list_conversations()?;
        let mut index = Vec::new();
        
        for db_conv in db_conversations {
            let id = Uuid::parse_str(&db_conv.id)
                .map_err(|e| rusqlite::Error::InvalidParameterName(format!("Invalid UUID: {}", e)))?;
            
            index.push(super::conversation_storage::ConversationIndex {
                id,
                title: db_conv.title,
                created_at: DateTime::from_timestamp(db_conv.created_at, 0).unwrap_or_else(Utc::now),
                updated_at: DateTime::from_timestamp(db_conv.created_at, 0).unwrap_or_else(Utc::now),
            });
        }
        
        Ok(index)
    }
}

impl Default for Storage {
    fn default() -> Self {
        Self::new_default().unwrap_or_else(|e| {
            eprintln!("Failed to initialize SQLite storage: {}", e);
            // Fallback to a temporary database
            Self::new(std::env::temp_dir().join("cosmic_llm_temp.db"))
                .expect("Failed to create temporary database")
        })
    }
}
