//! History search/rename and memories page message handlers

use cosmic::app;
use crate::server::dto::ClientCommand;
use crate::ui::app::{LunaThinApp, MemoryDraft, Message};

pub fn handle_history_memories_messages(
    app: &mut LunaThinApp,
    message: Message,
) -> Option<app::Task<Message>> {
    match message {
        Message::HistorySearchChanged(query) => {
            app.history_search = query.clone();
            if query.trim().is_empty() {
                app.history_search_results.clear();
                app.list_conversations();
            } else if app.connection_status == crate::ui::app::ConnectionStatus::Connected {
                app.search_conversations(query.trim());
            }
            None
        }
        Message::BeginRenameConversation(conv_id) => {
            let title = app
                .conversations
                .iter()
                .find(|c| c.id == conv_id)
                .map(|c| c.title.clone())
                .unwrap_or_default();
            app.renaming_conversation = Some((conv_id, title));
            None
        }
        Message::RenameDraftChanged(draft) => {
            if let Some((id, _)) = app.renaming_conversation.take() {
                app.renaming_conversation = Some((id, draft));
            }
            None
        }
        Message::ConfirmRenameConversation => {
            if let Some((conversation_id, title)) = app.renaming_conversation.clone() {
                let trimmed = title.trim().to_string();
                if !trimmed.is_empty() {
                    app.send_command(ClientCommand::RenameConversation {
                        conversation_id,
                        title: trimmed,
                    });
                }
            }
            None
        }
        Message::CancelRenameConversation => {
            app.renaming_conversation = None;
            None
        }
        Message::LoadMemories => {
            if app.connection_status == crate::ui::app::ConnectionStatus::Connected {
                let query = if app.memories_search.trim().is_empty() {
                    None
                } else {
                    Some(app.memories_search.clone())
                };
                app.list_memories(query);
            }
            None
        }
        Message::MemoriesSearchChanged(query) => {
            app.memories_search = query.clone();
            if app.connection_status == crate::ui::app::ConnectionStatus::Connected {
                let q = if query.trim().is_empty() {
                    None
                } else {
                    Some(query.trim().to_string())
                };
                app.list_memories(q);
            }
            None
        }
        Message::BeginEditMemory(id) => {
            if let Some(memory) = app.memories.iter().find(|m| m.id == id) {
                app.editing_memory = Some(MemoryDraft {
                    id: memory.id,
                    content: memory.content.clone(),
                    category: memory.category.clone().unwrap_or_default(),
                    importance: memory.importance.to_string(),
                });
            }
            None
        }
        Message::MemoryDraftContentChanged(content) => {
            if let Some(draft) = app.editing_memory.as_mut() {
                draft.content = content;
            }
            None
        }
        Message::MemoryDraftCategoryChanged(category) => {
            if let Some(draft) = app.editing_memory.as_mut() {
                draft.category = category;
            }
            None
        }
        Message::MemoryDraftImportanceChanged(importance) => {
            if let Some(draft) = app.editing_memory.as_mut() {
                draft.importance = importance;
            }
            None
        }
        Message::ConfirmEditMemory => {
            if let Some(draft) = app.editing_memory.clone() {
                let content = draft.content.trim().to_string();
                if content.is_empty() {
                    app.inline_error = Some("Memory content cannot be empty".to_string());
                    return None;
                }
                let category = if draft.category.trim().is_empty() {
                    Some(String::new())
                } else {
                    Some(draft.category.trim().to_string())
                };
                let importance = draft.importance.trim().parse::<i32>().ok();
                app.send_command(ClientCommand::UpdateMemory {
                    id: draft.id,
                    content: Some(content),
                    category,
                    importance,
                });
            }
            None
        }
        Message::CancelEditMemory => {
            app.editing_memory = None;
            None
        }
        Message::DeleteMemory(id) => {
            app.send_command(ClientCommand::DeleteMemory { id });
            None
        }
        _ => None,
    }
}
