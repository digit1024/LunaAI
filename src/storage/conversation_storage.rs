use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Turn {
    pub id: Uuid,
    pub iteration: u32,
    pub text: String,
    pub complete: bool,
    pub tools: Vec<ToolCallInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallInfo {
    pub id: Option<String>,
    pub tool_name: String,
    pub parameters: String,
    pub status: ToolCallStatus,
    pub result: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ToolCallStatus {
    Started,
    Completed,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub id: Uuid,
    pub title: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub messages: Vec<StoredMessage>,
    pub turns: Vec<Turn>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredMessage {
    pub id: Uuid,
    pub role: String,
    pub content: String,
    pub timestamp: DateTime<Utc>,
}

impl Conversation {
    #[allow(dead_code)]
    pub fn new(title: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            title,
            created_at: now,
            updated_at: now,
            messages: Vec::new(),
            turns: Vec::new(),
        }
    }

    #[allow(dead_code)]
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

    #[allow(dead_code)]
    pub fn add_turn(&mut self, turn: Turn) {
        self.turns.push(turn);
        self.updated_at = Utc::now();
    }

    #[allow(dead_code)]
    pub fn rebuild_llm_messages(&self) -> Vec<crate::llm::Message> {
        let mut llm_messages = Vec::new();
        
        // Add user messages
        for msg in &self.messages {
            if msg.role == "user" {
                llm_messages.push(crate::llm::Message::new(
                    crate::llm::Role::User,
                    msg.content.clone()
                ));
            }
        }
        
        // Add assistant turns with tool calls and results
        for turn in &self.turns {
            if !turn.text.trim().is_empty() {
                llm_messages.push(crate::llm::Message::new(
                    crate::llm::Role::Assistant,
                    turn.text.clone()
                ));
            }
            
            // Add tool results for this turn
            for tool in &turn.tools {
                if let Some(tool_id) = &tool.id {
                    let content = if let Some(result) = &tool.result {
                        result.clone()
                    } else if let Some(error) = &tool.error {
                        format!("Error: {}", error)
                    } else {
                        continue;
                    };
                    
                    llm_messages.push(crate::llm::Message::new_tool_result(
                        tool_id.clone(),
                        content,
                        tool.status == ToolCallStatus::Error
                    ));
                }
            }
        }
        
        llm_messages
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
    #[allow(dead_code)]
    conversations: HashMap<Uuid, Conversation>,
    #[allow(dead_code)]
    conversations_dir: PathBuf,
    #[allow(dead_code)]
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
    #[allow(dead_code)]
    pub fn new() -> Self {
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

    #[allow(dead_code)]
    fn conversation_file_path(&self, conversation_id: &Uuid) -> PathBuf {
        self.conversations_dir.join(format!("{}.json", conversation_id))
    }

    #[allow(dead_code)]
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

    #[allow(dead_code)]
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

    #[allow(dead_code)]
    fn save_conversation_index(&self) {
        println!("💾 Saving conversation index...");
        let index: Vec<ConversationIndex> = self.conversations
            .values()
            .map(|conv| ConversationIndex {
                id: conv.id,
                title: conv.title.clone(),
                created_at: conv.created_at,
                updated_at: conv.updated_at,
            })
            .collect();
        
        println!("📝 Index contains {} conversations", index.len());
        for conv in &index {
            println!("  - {}: '{}'", conv.id, conv.title);
        }
        
        if let Some(parent) = self.index_file.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(data) = serde_json::to_string_pretty(&index) {
            match fs::write(&self.index_file, data) {
                Ok(_) => println!("✅ Index file saved successfully to {:?}", self.index_file),
                Err(e) => println!("❌ Failed to save index file: {}", e),
            }
        } else {
            println!("❌ Failed to serialize index data");
        }
    }

    #[allow(dead_code)]
    fn save_conversation(&self, conversation: &Conversation) {
        let file_path = self.conversation_file_path(&conversation.id);
        if let Some(parent) = file_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(data) = serde_json::to_string_pretty(conversation) {
            let _ = fs::write(&file_path, data);
        }
    }

    #[allow(dead_code)]
    pub fn create_conversation(&mut self, title: String) -> Uuid {
        let conversation = Conversation::new(title);
        let id = conversation.id;
        self.conversations.insert(id, conversation.clone());
        self.save_conversation(&conversation);
        self.save_conversation_index();
        id
    }

    #[allow(dead_code)]
    pub fn get_conversation(&self, id: &Uuid) -> Option<&Conversation> {
        self.conversations.get(id)
    }

    #[allow(dead_code)]
    pub fn get_conversation_mut(&mut self, id: &Uuid) -> Option<&mut Conversation> {
        self.conversations.get_mut(id)
    }

    #[allow(dead_code)]
    pub fn list_conversations(&self) -> Vec<&Conversation> {
        let mut conversations: Vec<&Conversation> = self.conversations.values().collect();
        conversations.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        conversations
    }

    #[allow(dead_code)]
    pub fn list_conversations_from_index(&self) -> Vec<ConversationIndex> {
        let mut index = self.load_conversation_index();
        index.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        index
    }

    #[allow(dead_code)]
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

    #[allow(dead_code)]
    pub fn add_message_to_conversation(&mut self, conversation_id: &Uuid, role: String, content: String) {
        if let Some(conversation) = self.conversations.get_mut(conversation_id) {
            conversation.add_message(role, content);
            // Clone the conversation to avoid borrowing issues
            let conversation_clone = conversation.clone();
            self.save_conversation(&conversation_clone);
            self.save_conversation_index();
        }
    }

    #[allow(dead_code)]
    pub fn add_turn_to_conversation(&mut self, conversation_id: &Uuid, turn: Turn) {
        if let Some(conversation) = self.conversations.get_mut(conversation_id) {
            conversation.add_turn(turn);
            // Clone the conversation to avoid borrowing issues
            let conversation_clone = conversation.clone();
            self.save_conversation(&conversation_clone);
            self.save_conversation_index();
        }
    }

    #[allow(dead_code)]
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
