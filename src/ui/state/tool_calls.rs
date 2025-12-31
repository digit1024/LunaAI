//! Tool call state management
//!
//! Manages tool call-related state:
//! - Active tool calls (in progress)
//! - Archived tool calls (completed, anchored to messages)
//! - Expanded tool call UI state
//! - Tool runtime context

use crate::llm::ToolCall;
use crate::ui::app::{AnchoredToolCall, ToolCallInfo, ToolCallStatus, ToolRuntimeContext};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use tracing::debug;

/// Tool call state
pub struct ToolCallState {
    /// Active tool calls (currently executing)
    pub active_tool_calls: Vec<ToolCallInfo>,
    
    /// Anchors tool calls under the AI message that executed them
    pub current_ai_message_index: Option<usize>,
    
    /// Archived tool calls (completed, anchored to messages)
    pub archived_tool_calls: Vec<AnchoredToolCall>,
    
    /// Expanded tool calls (for UI display)
    pub expanded_tool_calls: HashSet<usize>,
    
    /// Expanded tool summaries (for UI display)
    pub expanded_tool_summaries: HashSet<(usize, String)>,
    
    /// Pending tool calls for persistence
    pub pending_tool_calls_for_history: Vec<ToolCall>,
    
    /// Tool runtime context (for tool execution)
    pub tool_runtime_context: HashMap<String, ToolRuntimeContext>,
}

impl ToolCallState {
    /// Create a new tool call state
    pub fn new() -> Self {
        Self {
            active_tool_calls: Vec::new(),
            current_ai_message_index: None,
            archived_tool_calls: Vec::new(),
            expanded_tool_calls: HashSet::new(),
            expanded_tool_summaries: HashSet::new(),
            pending_tool_calls_for_history: Vec::new(),
            tool_runtime_context: HashMap::new(),
        }
    }

    /// Add an active tool call
    pub fn add_active_tool_call(&mut self, tool_call: ToolCallInfo) {
        debug!(
            tool_name = %tool_call.tool_name,
            "Adding active tool call"
        );
        self.active_tool_calls.push(tool_call);
    }

    /// Complete a tool call (move from active to archived)
    pub fn complete_tool_call(
        &mut self,
        tool_call_id: &str,
        result: String,
        is_error: bool,
    ) {
        // Find and remove from active
        if let Some(index) = self
            .active_tool_calls
            .iter()
            .position(|tc| tc.id.as_deref() == Some(tool_call_id))
        {
            let mut tool_call = self.active_tool_calls.remove(index);
            tool_call.status = if is_error {
                ToolCallStatus::Error
            } else {
                ToolCallStatus::Completed
            };
            if is_error {
                tool_call.error = Some(result);
            } else {
                tool_call.result = Some(result);
            }

            // Archive it if we have an anchor
            if let Some(anchor_index) = self.current_ai_message_index {
                self.archived_tool_calls.push(AnchoredToolCall {
                    anchor_index,
                    tool_call,
                });
                debug!(
                    tool_call_id = tool_call_id,
                    anchor_index = anchor_index,
                    "Archived tool call"
                );
            }
        }
    }

    /// Archive a tool call at a specific message index
    pub fn archive_tool_call(&mut self, anchor_index: usize, tool_call: ToolCallInfo) {
        self.archived_tool_calls.push(AnchoredToolCall {
            anchor_index,
            tool_call,
        });
    }

    /// Set the current AI message index (anchor for tool calls)
    pub fn set_current_ai_message_index(&mut self, index: Option<usize>) {
        self.current_ai_message_index = index;
    }

    /// Toggle expansion of a tool call
    pub fn toggle_tool_call(&mut self, index: usize) {
        if self.expanded_tool_calls.contains(&index) {
            self.expanded_tool_calls.remove(&index);
        } else {
            self.expanded_tool_calls.insert(index);
        }
    }

    /// Toggle expansion of a tool summary
    pub fn toggle_tool_summary(&mut self, message_index: usize, summary_id: String) {
        let key = (message_index, summary_id);
        if self.expanded_tool_summaries.contains(&key) {
            self.expanded_tool_summaries.remove(&key);
        } else {
            self.expanded_tool_summaries.insert(key);
        }
    }

    /// Check if a tool call is expanded
    pub fn is_tool_call_expanded(&self, index: usize) -> bool {
        self.expanded_tool_calls.contains(&index)
    }

    /// Check if a tool summary is expanded
    pub fn is_tool_summary_expanded(&self, message_index: usize, summary_id: &str) -> bool {
        self.expanded_tool_summaries
            .contains(&(message_index, summary_id.to_string()))
    }

    /// Add a pending tool call for history persistence
    pub fn add_pending_tool_call(&mut self, tool_call: ToolCall) {
        self.pending_tool_calls_for_history.push(tool_call);
    }

    /// Clear pending tool calls
    pub fn clear_pending_tool_calls(&mut self) {
        self.pending_tool_calls_for_history.clear();
    }

    /// Set tool runtime context
    pub fn set_tool_runtime_context(
        &mut self,
        tool_call_id: String,
        anchor_index: usize,
        params: Option<Value>,
    ) {
        self.tool_runtime_context.insert(
            tool_call_id,
            ToolRuntimeContext {
                anchor_index,
                params,
            },
        );
    }

    /// Get tool runtime context
    pub fn get_tool_runtime_context(&self, tool_call_id: &str) -> Option<&ToolRuntimeContext> {
        self.tool_runtime_context.get(tool_call_id)
    }

    /// Clear all tool call state
    pub fn clear(&mut self) {
        self.active_tool_calls.clear();
        self.current_ai_message_index = None;
        self.archived_tool_calls.clear();
        self.expanded_tool_calls.clear();
        self.expanded_tool_summaries.clear();
        self.pending_tool_calls_for_history.clear();
        self.tool_runtime_context.clear();
    }
}

impl Default for ToolCallState {
    fn default() -> Self {
        Self::new()
    }
}




