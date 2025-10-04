use crate::config::AppConfig;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub id: Uuid,
    pub title: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub messages: Vec<StoredMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredMessage {
    pub id: Uuid,
    pub role: String,
    pub content: String,
    pub timestamp: DateTime<Utc>,
}

impl Conversation {
    pub fn new(title: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            title,
            created_at: now,
            updated_at: now,
            messages: Vec::new(),
        }
    }

    pub fn add_message(&mut self, role: String, content: String) {
        let message = StoredMessage {
            id: Uuid::new_v4(),
            role,
            content,
            timestamp: Utc::now(),
        };
        self.messages.push(message);
        self.updated_at = Utc::now();
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationIndex {
    pub id: Uuid,
    pub title: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct Storage {
    conversations: HashMap<Uuid, Conversation>,
    conversations_dir: PathBuf,
    index_file: PathBuf,
}

impl Default for Storage {
    fn default() -> Self {
        Self {
            conversations: HashMap::new(),
            conversations_dir: Self::default_conversations_dir(),
            index_file: Self::default_index_file(),
        }
    }
}

impl Storage {
    pub fn new(_config: AppConfig) -> Self {
        let conversations_dir = Self::default_conversations_dir();
        let index_file = Self::default_index_file();
        let mut storage = Self {
            conversations: HashMap::new(),
            conversations_dir,
            index_file,
        };
        storage.load_conversations();
        storage
    }

    fn default_conversations_dir() -> PathBuf {
        dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("cosmic_llm")
            .join("conversations")
    }

    fn default_index_file() -> PathBuf {
        dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("cosmic_llm")
            .join("conversations_index.json")
    }

    fn conversation_file_path(&self, conversation_id: &Uuid) -> PathBuf {
        self.conversations_dir.join(format!("{}.json", conversation_id))
    }

    fn load_conversations(&mut self) {
        // Create conversations directory if it doesn't exist
        if let Err(e) = fs::create_dir_all(&self.conversations_dir) {
            eprintln!("Failed to create conversations directory: {}", e);
            return;
        }

        // Load conversation index
        let index = self.load_conversation_index();
        
        // Load each conversation from its individual file
        for conv_index in index {
            let file_path = self.conversation_file_path(&conv_index.id);
            if file_path.exists() {
                if let Ok(data) = fs::read_to_string(&file_path) {
                    if let Ok(conversation) = serde_json::from_str::<Conversation>(&data) {
                        self.conversations.insert(conversation.id, conversation);
                    }
                }
            }
        }
    }

    fn load_conversation_index(&self) -> Vec<ConversationIndex> {
        if self.index_file.exists() {
            if let Ok(data) = fs::read_to_string(&self.index_file) {
                if let Ok(index) = serde_json::from_str::<Vec<ConversationIndex>>(&data) {
                    return index;
                }
            }
        }
        Vec::new()
    }

    fn save_conversation_index(&self) {
        let index: Vec<ConversationIndex> = self.conversations
            .values()
            .map(|conv| ConversationIndex {
                id: conv.id,
                title: conv.title.clone(),
                created_at: conv.created_at,
                updated_at: conv.updated_at,
            })
            .collect();
        
        if let Some(parent) = self.index_file.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(data) = serde_json::to_string_pretty(&index) {
            let _ = fs::write(&self.index_file, data);
        }
    }

    fn save_conversation(&self, conversation: &Conversation) {
        let file_path = self.conversation_file_path(&conversation.id);
        if let Some(parent) = file_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(data) = serde_json::to_string_pretty(conversation) {
            let _ = fs::write(&file_path, data);
        }
    }

    pub fn create_conversation(&mut self, title: String) -> Uuid {
        let conversation = Conversation::new(title);
        let id = conversation.id;
        self.conversations.insert(id, conversation.clone());
        self.save_conversation(&conversation);
        self.save_conversation_index();
        id
    }

    pub fn get_conversation(&self, id: &Uuid) -> Option<&Conversation> {
        self.conversations.get(id)
    }

    pub fn get_conversation_mut(&mut self, id: &Uuid) -> Option<&mut Conversation> {
        self.conversations.get_mut(id)
    }

    pub fn list_conversations(&self) -> Vec<&Conversation> {
        let mut conversations: Vec<&Conversation> = self.conversations.values().collect();
        conversations.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        conversations
    }

    pub fn update_conversation_title(&mut self, id: &Uuid, title: String) -> bool {
        if let Some(conversation) = self.conversations.get_mut(id) {
            conversation.title = title;
            conversation.updated_at = Utc::now();
            // Clone the conversation to avoid borrowing issues
            let conversation_clone = conversation.clone();
            self.save_conversation(&conversation_clone);
            self.save_conversation_index();
            true
        } else {
            false
        }
    }

    pub fn add_message_to_conversation(&mut self, conversation_id: &Uuid, role: String, content: String) {
        if let Some(conversation) = self.conversations.get_mut(conversation_id) {
            conversation.add_message(role, content);
            // Clone the conversation to avoid borrowing issues
            let conversation_clone = conversation.clone();
            self.save_conversation(&conversation_clone);
            self.save_conversation_index();
        }
    }

    pub fn delete_conversation(&mut self, conversation_id: &Uuid) -> bool {
        // Remove from memory
        if self.conversations.remove(conversation_id).is_some() {
            // Delete the file
            let file_path = self.conversation_file_path(conversation_id);
            if file_path.exists() {
                let _ = fs::remove_file(&file_path);
            }
            // Update the index
            self.save_conversation_index();
            true
        } else {
            false
        }
    }

}
