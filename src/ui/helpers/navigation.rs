//! Navigation helpers
//!
//! Helper functions for navigation model management, extracted from app.rs.

use crate::ui::app::{NavigationPage, NavItem};
use crate::ui::state::ConversationState;
use crate::storage::Storage;
use cosmic::widget;

/// Update the nav model with current conversation title and recent conversations
pub fn update_nav_model(
    nav_model: &mut widget::segmented_button::SingleSelectModel,
    conversation_state: &ConversationState,
    storage: &Storage,
) {
    // Clear and rebuild the nav model
    let mut model = widget::segmented_button::ModelBuilder::default().build();
    
    let current_conv_id = conversation_state.current_conversation_id;
    
    // Add "New Chat" as first item when there's no active conversation
    if current_conv_id.is_none() {
        model
            .insert()
            .text("New Chat")
            .icon(crate::ui::icons::get_icon("chat-symbolic", 16))
            .data(NavItem::Page(NavigationPage::Chat));
    }
    
    // Ensure active conversation is always visible (in case it's not in top 11 yet)
    // We'll add it first if it's not in the recent list
    let mut added_conv_ids = std::collections::HashSet::new();
    let active_conv_title = if let Some(active_conv_id) = current_conv_id {
        // Check if active conversation is in recent list
        let is_in_recent = conversation_state.recent_conversations.iter()
            .any(|(id, _)| *id == active_conv_id);
        
        if !is_in_recent {
            // Fetch the active conversation's title from storage
            storage.get_conversation(&active_conv_id)
                .ok()
                .flatten()
                .map(|conv| (active_conv_id, conv.title))
        } else {
            None
        }
    } else {
        None
    };
    
    // Add active conversation first if it wasn't in recent list
    if let Some((active_conv_id, active_title)) = active_conv_title {
        added_conv_ids.insert(active_conv_id);
        model
            .insert()
            .text(active_title)
            .icon(crate::ui::icons::get_icon("chat-bubble-text-symbolic", 16))
            .data(NavItem::Conversation(active_conv_id));
    }
    
    // Add all recent conversations (including active one if it was in the list, up to 11 items)
    for (conv_id, title) in &conversation_state.recent_conversations {
        if !added_conv_ids.contains(conv_id) {
            added_conv_ids.insert(*conv_id);
            model
                .insert()
                .text(title.clone())
                .icon(crate::ui::icons::get_icon("chat-bubble-text-symbolic", 16))
                .data(NavItem::Conversation(*conv_id));
        }
    }
    
    // Add "More history" (replaces History)
    model
        .insert()
        .text("More history")
        .icon(crate::ui::icons::get_icon("list-large-symbolic", 16))
        .data(NavItem::Page(NavigationPage::History))
        .divider_above(true);
    
    // Add MCP Config
    model
        .insert()
        .text("MCP Config")
        .icon(crate::ui::icons::get_icon("configure-symbolic", 16))
        .data(NavItem::Page(NavigationPage::MCPConfig));
    
    // Add Settings
    model
        .insert()
        .text("Settings")
        .icon(crate::ui::icons::get_icon("settings-symbolic", 16))
        .data(NavItem::Page(NavigationPage::Settings))
        .divider_above(true);
    
    // Activate the current conversation or "New Chat" if no active conversation
    let mut active_entity_opt = None;
    let mut first_entity_opt = None;
    
    for entity in model.iter() {
        if first_entity_opt.is_none() {
            first_entity_opt = Some(entity);
        }
        
        if let Some(nav_item) = model.data::<NavItem>(entity) {
            match nav_item {
                NavItem::Conversation(id) => {
                    if let Some(conv_id) = current_conv_id {
                        if id == &conv_id {
                            active_entity_opt = Some(entity);
                            break; // Found the active conversation, no need to continue
                        }
                    }
                }
                NavItem::Page(NavigationPage::Chat) => {
                    // "New Chat" item - activate if no active conversation
                    if current_conv_id.is_none() && active_entity_opt.is_none() {
                        active_entity_opt = Some(entity);
                    }
                }
                _ => {}
            }
        }
    }
    
    // Activate the found entity or fallback to first
    if let Some(entity) = active_entity_opt {
        model.activate(entity);
    } else if let Some(entity) = first_entity_opt {
        model.activate(entity);
    }
    
    *nav_model = model;
}

/// Load recent conversations from storage (last 11 to accommodate active conversation)
pub fn load_recent_conversations(
    conversation_state: &mut ConversationState,
    storage: &Storage,
) {
    match storage.list_conversations_paginated(None, Some(11)) {
        Ok(conversations) => {
            conversation_state.recent_conversations = conversations
                .into_iter()
                .map(|c| (c.id, c.title))
                .collect();
        }
        Err(e) => {
            tracing::error!(error = %e, "Failed to load recent conversations");
            conversation_state.recent_conversations.clear();
        }
    }
}

