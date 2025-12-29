//! Chat page module
//!
//! Full libcosmic module for the chat interface.
//! Manages chat-specific state and messages.

use cosmic::{
    app,
    widget::{self, text_editor},
    Element,
};
use std::sync::Arc;

use crate::{
    agentic::protocol::AgentUpdate,
    ui::widgets::ToolCallMessage,
};

/// Chat page state
pub struct Page {
    /// Input text
    pub input: String,
    
    /// Input content (text editor)
    pub input_content: text_editor::Content,
    
    /// Input widget ID
    pub input_id: cosmic::widget::Id,
    
    /// Scrollable widget ID
    pub scrollable_id: cosmic::widget::Id,
    
    /// Last user message (for retry)
    pub last_user_message: Option<String>,
    
    /// Typing indicator animation progress (0.0 to 1.0)
    pub typing_indicator_progress: f32,
    
    /// Typing indicator start time
    pub typing_indicator_start_time: Option<cosmic::iced::time::Instant>,
    
    /// Current error message
    pub current_error: Option<String>,
}

impl Page {
    /// Create a new chat page
    pub fn new() -> Self {
        Self {
            input: String::new(),
            input_content: text_editor::Content::new(),
            input_id: cosmic::widget::Id::unique(),
            scrollable_id: cosmic::widget::Id::unique(),
            last_user_message: None,
            typing_indicator_progress: 0.0,
            typing_indicator_start_time: None,
            current_error: None,
        }
    }
}

impl Default for Page {
    fn default() -> Self {
        Self::new()
    }
}

/// Chat page messages
#[derive(Debug, Clone)]
pub enum Message {
    /// Input text changed
    InputChanged(String),
    
    /// Input action performed (text editor action)
    InputActionPerformed(text_editor::Action),
    
    /// Send message
    SendMessage,
    
    /// Stop streaming message
    StopMessage,
    
    /// Retry last message
    RetryMessage,
    
    /// Attach file
    AttachFile,
    
    /// File selected (file path)
    FileSelected(String),
    
    /// Remove file (file path)
    RemoveFile(String),
    
    /// File chooser cancelled
    FileChooserCancelled,
    
    /// File chooser error
    FileChooserError(Arc<cosmic::dialog::file_chooser::Error>),
    
    /// Agent update (from agentic loop)
    AgentUpdate(AgentUpdate),
    
    /// Tool call started (tool_name, parameters)
    ToolCallStarted(String, String),
    
    /// Tool call completed (tool_name, result)
    ToolCallCompleted(String, String),
    
    /// Tool call error (tool_name, error)
    ToolCallError(String, String),
    
    /// Tool call widget message (index, message)
    ToolCallWidgetMessage(usize, ToolCallMessage),
    
    /// Toggle tool summary (message idx, summary id)
    ToggleToolSummary(usize, String),
    
    /// Toggle reasoning (message idx)
    ToggleReasoning(usize),
    
    /// Toggle summary (message idx)
    ToggleSummary(usize),
    
    /// Scroll to bottom
    ScrollToBottom,
    
    /// Typing indicator tick
    TypingIndicatorTick(cosmic::iced::time::Instant),
    
    /// Manual summarization
    ManualSummarize,
    
    /// D-Bus TTS/STT messages
    DbusServiceAvailable(bool),
    CheckDbusService,
    PlayMessageTts(usize),
    StopMessageTts,
    StartStt,
    StopStt,
    SttResult(String),
    DbusStatusChanged(String),
}

