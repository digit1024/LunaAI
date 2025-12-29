//! Tool message handlers
//!
//! Handles tool-related messages: ToolCallStarted, ToolCallCompleted, ToolCallError, etc.

use cosmic::app;

use crate::ui::app::{CosmicLlmApp, Message, ToolCallInfo, ToolCallStatus};

/// Handle tool-related messages
pub fn handle_tool_messages(
    app: &mut CosmicLlmApp,
    message: Message,
) -> Option<app::Task<Message>> {
    match message {
        Message::ToolCallStarted(tool_name, parameters) => {
            // Add tool call to active list
            app.tool_call_state.active_tool_calls.push(ToolCallInfo {
                id: None,
                tool_name: tool_name.clone(),
                parameters,
                status: ToolCallStatus::Started,
                result: None,
                error: None,
            });
            None
        }
        Message::ToolCallCompleted(tool_name, result) => {
            // Update tool call status
            if let Some(tool_call) = app
                .tool_call_state.active_tool_calls
                .iter_mut()
                .find(|tc| tc.tool_name == tool_name)
            {
                tool_call.status = ToolCallStatus::Completed;
                tool_call.result = Some(result);
            }
            None
        }
        Message::ToolCallError(tool_name, error) => {
            // Update tool call status
            if let Some(tool_call) = app
                .tool_call_state.active_tool_calls
                .iter_mut()
                .find(|tc| tc.tool_name == tool_name)
            {
                tool_call.status = ToolCallStatus::Error;
                tool_call.error = Some(error);
            }
            None
        }
        Message::ToolCallWidgetMessage(_index, _message) => {
            // Tool call widget messages are handled by the widget itself
            // This is just a placeholder
            None
        }
        Message::ToggleToolSummary(message_idx, summary_id) => {
            app.tool_call_state.toggle_tool_summary(message_idx, summary_id);
            None
        }
        _ => None, // Not a tool message
    }
}

