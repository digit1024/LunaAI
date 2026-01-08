//! Luna Thin UI - Main Application
//!
//! A thin client that connects to a Luna server via WebSocket.

use cosmic::{
    app::{self, Core},
    iced::Subscription,
    widget::{self, menu, text_editor},
    Application, Element,
};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::client::{FileClient, LunaWsClient, ServerConfig};
use crate::client::ws_client::EventReceiver;
use crate::server::dto::{ClientCommand, ConversationSummary, MessageView, ServerEvent};
use crate::ui::pages::{chat_page, history_page, settings_page, ChatPageState};

// ============================================================================
// Messages
// ============================================================================

#[derive(Debug, Clone)]
pub enum Message {
    // Input
    InputChanged(String),
    SendMessage,
    StopMessage,

    // Navigation
    NavigateTo(Page),
    SelectConversation(String),
    DeleteConversation(String),
    NewConversation,

    // Server events
    ServerEvent(ServerEvent),
    ServerConnected,
    ServerDisconnected,
    ServerError(String),

    // Connection
    Connect,
    Disconnect,

    // Settings
    HostChanged(String),
    PortChanged(String),
    ApiKeyChanged(String),

    // UI state
    ToggleReasoning(usize),
    ToggleSummary(usize),
    ToggleToolDetails(String),
    ScrollToBottom,
    DismissError,

    // Menu
    ShowAbout,
    CloseAbout,
    OpenSettings,
    Quit,
    OpenUrl(String),

    // File attachments
    AttachFile,
    FileSelected(String),
    RemoveFile(String),
    FileUploaded(String, String),
    FileUploadError(String),

    // Profile
    ChangeProfile(String),

    // Tick for animations
    Tick(cosmic::iced::time::Instant),

    // Copy message
    CopyMessage(String),

    // Text editor action
    InputActionPerformed(text_editor::Action),

    // Connection events
    ConnectionEstablished,
    ConnectionFailed(String),
}

// ============================================================================
// Navigation
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Chat,
    History,
    Settings,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NavItem {
    Page(Page),
    Conversation(String),
}

// ============================================================================
// Menu Actions
// ============================================================================

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MenuAction {
    About,
    NewConversation,
    Settings,
    Quit,
    SendMessage,
}

impl menu::Action for MenuAction {
    type Message = Message;

    fn message(&self) -> Self::Message {
        match self {
            MenuAction::About => Message::ShowAbout,
            MenuAction::NewConversation => Message::NewConversation,
            MenuAction::Settings => Message::OpenSettings,
            MenuAction::Quit => Message::Quit,
            MenuAction::SendMessage => Message::SendMessage,
        }
    }
}

// ============================================================================
// Connection Status
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionStatus {
    Disconnected,
    Connecting,
    Connected,
    Error,
}

// ============================================================================
// Chat Message (UI representation)
// ============================================================================

// ============================================================================
// Bubble Types (like mobile app)
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BubbleType {
    User,
    Assistant,
    ToolRequest,
    ToolResult,
    Summary,
}

#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub id: String,
    pub content: String,
    pub bubble_type: BubbleType,
    pub is_error: bool,
    pub reasoning_content: Option<String>,
    pub summarized_count: Option<usize>,
    /// For tool bubbles
    pub tool_call_id: Option<String>,
    pub tool_name: Option<String>,
    pub tool_params: Option<String>,
    pub tool_result: Option<String>,
    pub tool_status: Option<String>, // planned, running, done, error
    /// Whether this message is still streaming
    pub is_streaming: bool,
}

impl ChatMessage {
    /// Create user message
    pub fn user(content: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            content,
            bubble_type: BubbleType::User,
            is_error: false,
            reasoning_content: None,
            summarized_count: None,
            tool_call_id: None,
            tool_name: None,
            tool_params: None,
            tool_result: None,
            tool_status: None,
            is_streaming: false,
        }
    }
    
    /// Create assistant message
    pub fn assistant(content: String, reasoning_content: Option<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            content,
            bubble_type: BubbleType::Assistant,
            is_error: false,
            reasoning_content,
            summarized_count: None,
            tool_call_id: None,
            tool_name: None,
            tool_params: None,
            tool_result: None,
            tool_status: None,
            is_streaming: false,
        }
    }
    
    /// Create tool request bubble
    pub fn tool_request(tool_call_id: String, name: String, params: String, status: &str) -> Self {
        Self {
            id: format!("{}_request", tool_call_id),
            content: format!("🧰 {}", name),
            bubble_type: BubbleType::ToolRequest,
            is_error: false,
            reasoning_content: None,
            summarized_count: None,
            tool_call_id: Some(tool_call_id),
            tool_name: Some(name),
            tool_params: Some(params),
            tool_result: None,
            tool_status: Some(status.to_string()),
            is_streaming: false,
        }
    }
    
    /// Create tool result bubble
    pub fn tool_result(tool_call_id: String, name: String, result: String, is_error: bool) -> Self {
        Self {
            id: format!("{}_result", tool_call_id),
            content: format!("🧰 {}", name),
            bubble_type: BubbleType::ToolResult,
            is_error,
            reasoning_content: None,
            summarized_count: None,
            tool_call_id: Some(tool_call_id),
            tool_name: Some(name),
            tool_params: None,
            tool_result: Some(result),
            tool_status: Some(if is_error { "error".to_string() } else { "done".to_string() }),
            is_streaming: false,
        }
    }
    
    /// Create summary message
    pub fn summary(content: String, summarized_count: usize) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            content,
            bubble_type: BubbleType::Summary,
            is_error: false,
            reasoning_content: None,
            summarized_count: Some(summarized_count),
            tool_call_id: None,
            tool_name: None,
            tool_params: None,
            tool_result: None,
            tool_status: None,
            is_streaming: false,
        }
    }
    
    pub fn is_user(&self) -> bool {
        self.bubble_type == BubbleType::User
    }
    
    pub fn is_summary(&self) -> bool {
        self.bubble_type == BubbleType::Summary
    }
}

// ============================================================================
// Pending Attachment
// ============================================================================

#[derive(Debug, Clone)]
pub struct PendingAttachment {
    pub file_path: String,
    pub file_id: Option<String>,
    pub uploading: bool,
    pub error: Option<String>,
}

// ============================================================================
// Main Application
// ============================================================================

pub struct LunaThinApp {
    pub core: Core,

    // Server connectivity
    pub server_config: ServerConfig,
    pub ws_client: Arc<RwLock<LunaWsClient>>,
    pub file_client: Option<FileClient>,
    pub connection_status: ConnectionStatus,
    
    // WebSocket event receiver (for subscription polling)
    pub event_receiver: Arc<RwLock<Option<EventReceiver>>>,

    // Server state
    pub current_conversation_id: Option<String>,
    pub conversations: Vec<ConversationSummary>,
    pub messages: Vec<ChatMessage>,
    pub profiles: Vec<String>,
    pub current_profile: String,

    // Streaming state
    pub is_streaming: bool,
    pub streaming_content: String,
    pub reasoning_content: String,
    /// Tracks current assistant bubble ID for streaming (like mobile app)
    /// Reset when tools interrupt or new turn starts
    pub current_assistant_bubble_id: Option<String>,

    // UI state
    pub current_page: Page,
    pub key_binds: HashMap<menu::KeyBind, MenuAction>,
    pub nav_model: widget::segmented_button::SingleSelectModel,
    pub show_about: bool,

    // Chat page state
    pub chat_page: ChatPageState,

    // Input state
    pub input_text: String,
    pub pending_attachments: Vec<PendingAttachment>,

    // Expanded states
    pub expanded_reasoning: HashSet<usize>,
    pub expanded_summaries: HashSet<usize>,
    pub expanded_tools: HashSet<String>,

    // Error display
    pub inline_error: Option<String>,

    // Settings input
    pub settings_host: String,
    pub settings_port: String,
    pub settings_api_key: String,
}

impl LunaThinApp {
    fn new(core: Core) -> Self {
        let server_config = ServerConfig::load().unwrap_or_default();

        Self {
            core,
            settings_host: server_config.host.clone(),
            settings_port: server_config.port.to_string(),
            settings_api_key: server_config.api_key.clone(),
            server_config,
            ws_client: Arc::new(RwLock::new(LunaWsClient::new())),
            file_client: None,
            connection_status: ConnectionStatus::Disconnected,
            event_receiver: Arc::new(RwLock::new(None)),
            current_conversation_id: None,
            conversations: Vec::new(),
            messages: Vec::new(),
            profiles: Vec::new(),
            current_profile: String::new(),
            is_streaming: false,
            streaming_content: String::new(),
            reasoning_content: String::new(),
            current_assistant_bubble_id: None,
            current_page: Page::Settings,
            key_binds: Self::create_key_binds(),
            nav_model: Self::create_nav_model(),
            show_about: false,
            chat_page: ChatPageState::default(),
            input_text: String::new(),
            pending_attachments: Vec::new(),
            expanded_reasoning: HashSet::new(),
            expanded_summaries: HashSet::new(),
            expanded_tools: HashSet::new(),
            inline_error: None,
        }
    }

    fn create_key_binds() -> HashMap<menu::KeyBind, MenuAction> {
        use cosmic::iced::keyboard::Key;
        use cosmic::widget::menu::key_bind::{KeyBind, Modifier};

        let mut key_binds = HashMap::new();
        key_binds.insert(
            KeyBind {
                modifiers: vec![Modifier::Ctrl],
                key: Key::Character("n".into()),
            },
            MenuAction::NewConversation,
        );
        key_binds.insert(
            KeyBind {
                modifiers: vec![Modifier::Ctrl],
                key: Key::Character("q".into()),
            },
            MenuAction::Quit,
        );
        key_binds.insert(
            KeyBind {
                modifiers: vec![Modifier::Ctrl],
                key: Key::Character(",".into()),
            },
            MenuAction::Settings,
        );
        key_binds.insert(
            KeyBind {
                modifiers: vec![Modifier::Ctrl],
                key: Key::Named(cosmic::iced::keyboard::key::Named::Enter),
            },
            MenuAction::SendMessage,
        );
        key_binds
    }

    fn create_nav_model() -> widget::segmented_button::SingleSelectModel {
        let mut model = widget::segmented_button::ModelBuilder::default().build();
        model.insert()
            .text("New Chat")
            .icon(widget::icon::from_name("chat-symbolic").size(16))
            .data(NavItem::Page(Page::Chat));
        model.insert()
            .text("More history")
            .icon(widget::icon::from_name("list-large-symbolic").size(16))
            .data(NavItem::Page(Page::History))
            .divider_above(true);
        model.insert()
            .text("Settings")
            .icon(widget::icon::from_name("settings-symbolic").size(16))
            .data(NavItem::Page(Page::Settings))
            .divider_above(true);
        model
    }

    fn update_nav_model(&mut self) {
        let mut model = widget::segmented_button::ModelBuilder::default().build();

        // New Chat (only if no active conversation)
        if self.current_conversation_id.is_none() {
            model.insert()
                .text("New Chat")
                .icon(widget::icon::from_name("chat-symbolic").size(16))
                .data(NavItem::Page(Page::Chat));
        }

        // Recent conversations (max 11 to match original)
        for conv in self.conversations.iter().take(11) {
            let title = if conv.title.len() > 28 {
                format!("{}...", &conv.title[..25])
            } else {
                conv.title.clone()
            };
            model.insert()
                .text(title)
                .icon(widget::icon::from_name("chat-bubble-text-symbolic").size(16))
                .data(NavItem::Conversation(conv.id.clone()));
        }

        // More history
        model.insert()
            .text("More history")
            .icon(widget::icon::from_name("list-large-symbolic").size(16))
            .data(NavItem::Page(Page::History))
            .divider_above(true);

        // Settings
        model.insert()
            .text("Settings")
            .icon(widget::icon::from_name("settings-symbolic").size(16))
            .data(NavItem::Page(Page::Settings))
            .divider_above(true);

        // Activate current conversation if set
        if let Some(ref conv_id) = self.current_conversation_id {
            let entity_to_activate = model.iter()
                .find(|&entity| {
                    model.data::<NavItem>(entity)
                        .map(|item| matches!(item, NavItem::Conversation(id) if id == conv_id))
                        .unwrap_or(false)
                });
            if let Some(entity) = entity_to_activate {
                model.activate(entity);
            }
        }

        self.nav_model = model;
    }

    fn send_command(&self, command: ClientCommand) {
        let ws_client = self.ws_client.clone();
        tokio::spawn(async move {
            let client = ws_client.read().await;
            client.send(command);
        });
    }

    // ========================================================================
    // Message mapping (like mobile app's _mapMessages)
    // ========================================================================
    
    /// Map server messages to UI messages (like mobile app)
    /// Tool calls become separate request/result bubbles
    fn map_messages_from_server(&self, messages: &[MessageView]) -> Vec<ChatMessage> {
        let mut result = Vec::new();
        
        for m in messages {
            if m.tool_call_id.is_some() {
                // Tool message - single bubble with params and result
                let tool_call_id = m.tool_call_id.clone().unwrap();
                let tool_name = m.tool_name.clone().unwrap_or_else(|| "tool".to_string());
                let tool_status = m.tool_status.clone().unwrap_or_else(|| "done".to_string());
                let params = m.tool_params_json.as_ref()
                    .map(|v| serde_json::to_string_pretty(v).unwrap_or_default())
                    .unwrap_or_default();
                let tool_result = m.tool_result_json.as_ref()
                    .map(|v| serde_json::to_string_pretty(v).unwrap_or_default());
                let is_error = tool_status == "error";
                
                // Single tool bubble with both params and result
                let mut tool_msg = ChatMessage::tool_request(
                    tool_call_id,
                    tool_name,
                    params,
                    &tool_status,
                );
                tool_msg.tool_result = tool_result;
                tool_msg.is_error = is_error;
                
                result.push(tool_msg);
            } else if m.is_summary {
                // Summary message
                result.push(ChatMessage::summary(
                    m.content.clone(),
                    m.summarized_count.unwrap_or(0),
                ));
            } else {
                // Regular user/assistant message
                let bubble_type = if m.role == "user" {
                    BubbleType::User
                } else {
                    BubbleType::Assistant
                };
                
                result.push(ChatMessage {
                    id: m.id.clone(),
                    content: m.content.clone(),
                    bubble_type,
                    is_error: false,
                    reasoning_content: m.reasoning_content.clone(),
                    summarized_count: None,
                    tool_call_id: None,
                    tool_name: None,
                    tool_params: None,
                    tool_result: None,
                    tool_status: None,
                    is_streaming: false,
                });
            }
        }
        
        result
    }

    // ========================================================================
    // Streaming helpers (like mobile app)
    // ========================================================================
    
    /// Apply assistant delta - find or create assistant bubble
    fn apply_assistant_delta(&mut self, chunk: &str) {
        // Find existing bubble by our tracked ID
        if let Some(ref bubble_id) = self.current_assistant_bubble_id {
            if let Some(msg) = self.messages.iter_mut().find(|m| &m.id == bubble_id) {
                msg.content.push_str(chunk);
                msg.is_streaming = true;
                return;
            }
        }
        
        // Create new assistant bubble
        let new_id = Uuid::new_v4().to_string();
        self.current_assistant_bubble_id = Some(new_id.clone());
        
        self.messages.push(ChatMessage {
            id: new_id,
            content: chunk.to_string(),
            bubble_type: BubbleType::Assistant,
            is_error: false,
            reasoning_content: None,
            summarized_count: None,
            tool_call_id: None,
            tool_name: None,
            tool_params: None,
            tool_result: None,
            tool_status: None,
            is_streaming: true,
        });
    }
    
    /// Apply reasoning content delta
    fn apply_reasoning_delta(&mut self, chunk: &str) {
        // Find existing bubble by our tracked ID
        if let Some(ref bubble_id) = self.current_assistant_bubble_id {
            if let Some(msg) = self.messages.iter_mut().find(|m| &m.id == bubble_id) {
                let current = msg.reasoning_content.take().unwrap_or_default();
                msg.reasoning_content = Some(format!("{}{}", current, chunk));
                msg.is_streaming = true;
                return;
            }
        }
        
        // If no assistant bubble exists yet, create one with reasoning content
        let new_id = Uuid::new_v4().to_string();
        self.current_assistant_bubble_id = Some(new_id.clone());
        
        self.messages.push(ChatMessage {
            id: new_id,
            content: String::new(),
            bubble_type: BubbleType::Assistant,
            is_error: false,
            reasoning_content: Some(chunk.to_string()),
            summarized_count: None,
            tool_call_id: None,
            tool_name: None,
            tool_params: None,
            tool_result: None,
            tool_status: None,
            is_streaming: true,
        });
    }
    
    /// Complete assistant message
    fn complete_assistant(&mut self, content: &str, reasoning_content: Option<&str>) {
        self.is_streaming = false;
        self.streaming_content.clear();
        self.reasoning_content.clear();
        
        // Find our tracked bubble or the last assistant bubble
        if let Some(ref bubble_id) = self.current_assistant_bubble_id {
            if let Some(msg) = self.messages.iter_mut().find(|m| &m.id == bubble_id) {
                msg.content = content.to_string();
                msg.reasoning_content = reasoning_content.map(|s| s.to_string());
                msg.is_streaming = false;
                return;
            }
        }
        
        // Fallback: update last assistant or create new
        if let Some(msg) = self.messages.iter_mut().rev()
            .find(|m| m.bubble_type == BubbleType::Assistant)
        {
            msg.content = content.to_string();
            msg.reasoning_content = reasoning_content.map(|s| s.to_string());
            msg.is_streaming = false;
        } else {
            self.messages.push(ChatMessage::assistant(
                content.to_string(),
                reasoning_content.map(|s| s.to_string()),
            ));
        }
    }

    // ========================================================================
    // Tool call helpers (like mobile app)
    // ========================================================================
    
    /// Add tool request bubbles when tools are planned
    fn add_tool_request_bubbles(&mut self, tools: &[crate::server::dto::PlannedToolView]) {
        for tool in tools {
            let params = serde_json::to_string_pretty(&tool.params_json).unwrap_or_default();
            self.messages.push(ChatMessage::tool_request(
                tool.id.clone(),
                tool.name.clone(),
                params,
                "planned",
            ));
        }
    }
    
    /// Update tool request bubble when tool starts running
    fn update_tool_request(&mut self, tool_call_id: &str, status: &str, name: &str, params: &str) {
        let request_id = format!("{}_request", tool_call_id);
        
        if let Some(msg) = self.messages.iter_mut()
            .find(|m| m.id == request_id && m.bubble_type == BubbleType::ToolRequest)
        {
            msg.tool_status = Some(status.to_string());
            msg.tool_params = Some(params.to_string());
        } else {
            // Tool wasn't planned - create request bubble now
            self.messages.push(ChatMessage::tool_request(
                tool_call_id.to_string(),
                name.to_string(),
                params.to_string(),
                status,
            ));
        }
    }
    
    /// Update tool bubble when tool completes (no separate result bubble)
    fn add_tool_result_bubble(&mut self, tool_call_id: &str, _name: &str, result: &str, is_error: bool) {
        let request_id = format!("{}_request", tool_call_id);
        
        // Update the existing tool request bubble with result (single bubble approach)
        if let Some(msg) = self.messages.iter_mut()
            .find(|m| m.id == request_id && m.bubble_type == BubbleType::ToolRequest)
        {
            msg.tool_status = Some(if is_error { "error".to_string() } else { "done".to_string() });
            msg.tool_result = Some(result.to_string());
            msg.is_error = is_error;
        }
    }

    fn handle_server_event(&mut self, event: ServerEvent) -> app::Task<Message> {
        match event {
            ServerEvent::HealthOk { profile, .. } => {
                tracing::info!("📥 HealthOk received! Profile: {}", profile);
                self.connection_status = ConnectionStatus::Connected;
                self.current_profile = profile;
                self.current_page = Page::Chat;
            }
            ServerEvent::Error { message } => {
                self.inline_error = Some(message);
            }
            ServerEvent::ConversationCreated { conversation_id } => {
                // Just set the conversation ID - don't clear messages!
                // The user message we added is already there and should be preserved
                self.current_conversation_id = Some(conversation_id);
                self.send_command(ClientCommand::ListConversations {
                    query: None,
                    limit: Some(20),
                    offset: None,
                });
            }
            ServerEvent::ConversationLoaded { conversation } => {
                tracing::info!("📥 ConversationLoaded: {} ({} messages)", conversation.id, conversation.messages.len());
                self.current_conversation_id = Some(conversation.id.clone());
                self.current_profile = conversation.profile_name.unwrap_or_default();
                self.current_assistant_bubble_id = None;
                
                // Map messages like mobile app - tool calls become separate bubbles
                self.messages = self.map_messages_from_server(&conversation.messages);
                
                self.current_page = Page::Chat;
                self.update_nav_model();
            }
            ServerEvent::ConversationsList { conversations } => {
                self.conversations = conversations;
                self.update_nav_model();
            }
            ServerEvent::ProfileChanged { profile } => {
                self.current_profile = profile;
            }
            ServerEvent::ProfilesList { profiles, default_profile } => {
                self.profiles = profiles;
                if self.current_profile.is_empty() {
                    self.current_profile = default_profile;
                }
            }
            ServerEvent::MessageAccepted { .. } => {
                // Input is already cleared when SendMessage was triggered
                // Just clear pending attachments
                self.pending_attachments.clear();
            }
            ServerEvent::StreamingStarted { .. } => {
                self.current_assistant_bubble_id = None; // Reset for new streaming session
                self.is_streaming = true;
                self.streaming_content.clear();
                self.reasoning_content.clear();
                // Play typing sound
                crate::ui::audio::AudioService::play_sound("typing.mp3");
            }
            ServerEvent::AssistantDelta { chunk, .. } => {
                self.apply_assistant_delta(&chunk);
            }
            ServerEvent::ReasoningContentDelta { chunk, .. } => {
                self.apply_reasoning_delta(&chunk);
            }
            ServerEvent::AssistantComplete { content, reasoning_content, .. } => {
                self.complete_assistant(&content, reasoning_content.as_deref());
            }
            ServerEvent::ToolPlanned { tools, .. } => {
                // Tools interrupt assistant stream - reset so next delta creates new bubble
                self.current_assistant_bubble_id = None;
                self.add_tool_request_bubbles(&tools);
            }
            ServerEvent::ToolStarted { tool_call_id, name, params_json, .. } => {
                let params = serde_json::to_string_pretty(&params_json).unwrap_or_default();
                self.update_tool_request(&tool_call_id, "running", &name, &params);
            }
            ServerEvent::ToolResult { tool_call_id, name, result_json, .. } => {
                let result = serde_json::to_string_pretty(&result_json).unwrap_or_default();
                self.add_tool_result_bubble(&tool_call_id, &name, &result, false);
                // Play tool completion sound
                crate::ui::audio::AudioService::play_sound("tool.mp3");
            }
            ServerEvent::ToolError { tool_call_id, name, error, .. } => {
                self.add_tool_result_bubble(&tool_call_id, &name, &error, true);
            }
            ServerEvent::ConversationComplete { .. } => {
                self.is_streaming = false;
                // Play completion sound
                crate::ui::audio::AudioService::play_sound("done.mp3");
                // Tool calls are already moved to messages, just refresh list
                self.send_command(ClientCommand::ListConversations {
                    query: None,
                    limit: Some(20),
                    offset: None,
                });
            }
            ServerEvent::ConversationDeleted { conversation_id } => {
                if self.current_conversation_id.as_ref() == Some(&conversation_id) {
                    self.current_conversation_id = None;
                    self.messages.clear();
                }
                self.conversations.retain(|c| c.id != conversation_id);
                self.update_nav_model();
            }
            ServerEvent::StreamingStopped { .. } => {
                self.is_streaming = false;
            }
            _ => {}
        }
        app::Task::none()
    }
}

// ============================================================================
// Application Implementation
// ============================================================================

impl Application for LunaThinApp {
    type Executor = cosmic::executor::Default;
    type Flags = ();
    type Message = Message;
    const APP_ID: &'static str = "com.github.digit1024.luna_thin_ui";

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(core: Core, _flags: Self::Flags) -> (Self, app::Task<Self::Message>) {
        // Initialize icon cache
        if let Err(e) = crate::ui::icons::ICON_CACHE
            .set(std::sync::Mutex::new(crate::ui::icons::IconCache::new()))
        {
            tracing::warn!(error = ?e, "Failed to initialize icon cache (may be already initialized)");
        }
        
        let app = Self::new(core);
        
        // Auto-connect if server config is valid (has host and api_key)
        if !app.server_config.host.is_empty() && !app.server_config.api_key.is_empty() {
            tracing::info!("🔌 Auto-connecting to server...");
            (app, app::Task::done(cosmic::Action::App(Message::Connect)))
        } else {
            tracing::info!("⚙️ No valid server config, showing settings");
            (app, app::Task::none())
        }
    }

    fn update(&mut self, message: Self::Message) -> app::Task<Self::Message> {
        match message {
            Message::InputChanged(text) => {
                self.input_text = text;
            }
            Message::SendMessage => {
                if self.input_text.trim().is_empty() || self.connection_status != ConnectionStatus::Connected {
                    return app::Task::none();
                }

                // Save content before clearing
                let message_content = self.input_text.clone();
                
                // Clear input immediately when message is sent
                self.input_text.clear();
                self.chat_page.input_content = text_editor::Content::new();

                // Reset assistant bubble tracking for new conversation turn
                self.current_assistant_bubble_id = None;
                
                // Add user message immediately (only if not empty)
                if !message_content.trim().is_empty() {
                    self.messages.push(ChatMessage::user(message_content.clone()));
                }

                // Play sent sound
                crate::ui::audio::AudioService::play_sound("sent.mp3");

                let attachment_ids: Vec<String> = self
                    .pending_attachments
                    .iter()
                    .filter_map(|a| a.file_id.clone())
                    .collect();

                self.send_command(ClientCommand::SendMessage {
                    conversation_id: self.current_conversation_id.clone(),
                    content: message_content,
                    attachment_ids: if attachment_ids.is_empty() { None } else { Some(attachment_ids) },
                });
            }
            Message::StopMessage => {
                self.send_command(ClientCommand::StopStreaming {
                    conversation_id: self.current_conversation_id.clone(),
                });
            }
            Message::NewConversation => {
                self.current_conversation_id = None;
                self.messages.clear();
                self.current_assistant_bubble_id = None;
                self.current_page = Page::Chat;
                self.update_nav_model();
            }
            Message::NavigateTo(page) => {
                self.current_page = page;
            }
            Message::SelectConversation(conv_id) => {
                tracing::info!("📂 SelectConversation: {}", conv_id);
                self.send_command(ClientCommand::LoadConversation { conversation_id: conv_id.clone() });
            }
            Message::DeleteConversation(conv_id) => {
                self.send_command(ClientCommand::DeleteConversation { conversation_id: conv_id });
            }
            Message::ServerEvent(event) => {
                tracing::debug!("📥 ServerEvent received: {:?}", event);
                return self.handle_server_event(event);
            }
            Message::ServerConnected => {
                self.connection_status = ConnectionStatus::Connected;
                self.send_command(ClientCommand::HealthCheck);
                self.send_command(ClientCommand::ListConversations {
                    query: None,
                    limit: Some(20),
                    offset: None,
                });
                self.send_command(ClientCommand::ListProfiles);
            }
            Message::ServerDisconnected => {
                self.connection_status = ConnectionStatus::Disconnected;
            }
            Message::ServerError(error) => {
                self.connection_status = ConnectionStatus::Error;
                self.inline_error = Some(error);
            }
            Message::Connect => {
                // Update config from settings
                self.server_config.host = self.settings_host.clone();
                self.server_config.port = self.settings_port.parse().unwrap_or(8080);
                self.server_config.api_key = self.settings_api_key.clone();
                let _ = self.server_config.save();

                self.file_client = Some(FileClient::new(self.server_config.clone()));
                self.connection_status = ConnectionStatus::Connecting;

                let ws_client = self.ws_client.clone();
                let event_receiver = self.event_receiver.clone();
                let config = self.server_config.clone();
                return app::Task::perform(
                    async move {
                        let mut client = ws_client.write().await;
                        match client.connect(config).await {
                            Ok(_) => {
                                // Take the event receiver from the client
                                if let Some(rx) = client.take_event_receiver() {
                                    let mut event_rx = event_receiver.write().await;
                                    *event_rx = Some(rx);
                                }
                                Message::ServerConnected
                            },
                            Err(e) => Message::ServerError(e.to_string()),
                        }
                    },
                    |msg| cosmic::Action::App(msg),
                );
            }
            Message::Disconnect => {
                let ws_client = self.ws_client.clone();
                tokio::spawn(async move {
                    let mut client = ws_client.write().await;
                    client.disconnect().await;
                });
                self.connection_status = ConnectionStatus::Disconnected;
            }
            Message::HostChanged(host) => {
                self.settings_host = host;
            }
            Message::PortChanged(port) => {
                self.settings_port = port;
            }
            Message::ApiKeyChanged(api_key) => {
                self.settings_api_key = api_key;
            }
            Message::ChangeProfile(profile) => {
                self.send_command(ClientCommand::ChangeProfile { profile });
            }
            Message::ShowAbout => {
                self.show_about = !self.show_about;
            }
            Message::CloseAbout => {
                self.show_about = false;
            }
            Message::OpenSettings => {
                self.current_page = Page::Settings;
            }
            Message::OpenUrl(url) => {
                let _ = webbrowser::open(&url);
            }
            Message::Quit => {
                std::process::exit(0);
            }
            Message::ToggleReasoning(idx) => {
                if self.expanded_reasoning.contains(&idx) {
                    self.expanded_reasoning.remove(&idx);
                } else {
                    self.expanded_reasoning.insert(idx);
                }
            }
            Message::ToggleSummary(idx) => {
                if self.expanded_summaries.contains(&idx) {
                    self.expanded_summaries.remove(&idx);
                } else {
                    self.expanded_summaries.insert(idx);
                }
            }
            Message::ToggleToolDetails(id) => {
                if self.expanded_tools.contains(&id) {
                    self.expanded_tools.remove(&id);
                } else {
                    self.expanded_tools.insert(id);
                }
            }
            Message::DismissError => {
                self.inline_error = None;
            }
            Message::FileSelected(path) => {
                self.pending_attachments.push(PendingAttachment {
                    file_path: path.clone(),
                    file_id: None,
                    uploading: true,
                    error: None,
                });

                if let Some(ref file_client) = self.file_client {
                    let client = file_client.clone();
                    let path_clone = path.clone();
                    return app::Task::perform(
                        async move {
                            match client.upload_file(&path_clone).await {
                                Ok(attachment) => Message::FileUploaded(path_clone, attachment.file_id),
                                Err(e) => Message::FileUploadError(e.to_string()),
                            }
                        },
                        |msg| cosmic::Action::App(msg),
                    );
                }
            }
            Message::FileUploaded(path, file_id) => {
                if let Some(attachment) = self.pending_attachments.iter_mut().find(|a| a.file_path == path) {
                    attachment.file_id = Some(file_id);
                    attachment.uploading = false;
                }
            }
            Message::FileUploadError(error) => {
                self.inline_error = Some(format!("File upload failed: {}", error));
            }
            Message::RemoveFile(path) => {
                self.pending_attachments.retain(|a| a.file_path != path);
            }
            Message::CopyMessage(content) => {
                // Copy to clipboard - for now just log it
                // TODO: Add arboard dependency for actual clipboard support
                tracing::info!("Copy to clipboard: {} bytes", content.len());
                let _ = content; // Suppress unused warning
            }
            Message::InputActionPerformed(action) => {
                self.chat_page.input_content.perform(action);
                // Sync input_text with editor content
                self.input_text = self.chat_page.input_content.text();
            }
            Message::ConnectionEstablished => {
                self.connection_status = ConnectionStatus::Connected;
                self.send_command(ClientCommand::HealthCheck);
                self.send_command(ClientCommand::ListConversations {
                    query: None,
                    limit: Some(20),
                    offset: None,
                });
                self.send_command(ClientCommand::ListProfiles);
            }
            Message::ConnectionFailed(error) => {
                self.connection_status = ConnectionStatus::Error;
                self.inline_error = Some(error);
            }
            Message::Tick(_) => {
                // Update typing indicator animation
                self.chat_page.typing_indicator_progress = 
                    (self.chat_page.typing_indicator_progress + 0.1) % 1.0;
            }
            _ => {}
        }
        app::Task::none()
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        let mut subscriptions = vec![];

        // Typing indicator tick when streaming
        if self.is_streaming {
            subscriptions.push(
                cosmic::iced::time::every(std::time::Duration::from_millis(100))
                    .map(Message::Tick),
            );
        }

        // WebSocket event subscription - polls the event receiver with backpressure handling
        if self.connection_status == ConnectionStatus::Connected {
            let event_receiver = self.event_receiver.clone();
            subscriptions.push(
                Subscription::run_with_id(
                    "ws-events",
                    async_stream::stream! {
                        tracing::info!("🎧 WS subscription started");
                        // Take the receiver from the shared state
                        let rx = {
                            let mut guard = event_receiver.write().await;
                            guard.take()
                        };
                        
                        if let Some(mut rx) = rx {
                            tracing::info!("🎧 Got event receiver, starting poll loop");
                            let mut last_yield = std::time::Instant::now();
                            let min_yield_interval = std::time::Duration::from_millis(16); // ~60fps max
                            
                            loop {
                                match rx.recv().await {
                                    Some(event) => {
                                        // Add small delay to prevent channel overflow during rapid streaming
                                        // This batches rapid events together
                                        let now = std::time::Instant::now();
                                        if now.duration_since(last_yield) < min_yield_interval {
                                            tokio::time::sleep(min_yield_interval - now.duration_since(last_yield)).await;
                                        }
                                        last_yield = std::time::Instant::now();
                                        
                                        tracing::debug!("🎧 Received event from WS: {:?}", event);
                                        yield Message::ServerEvent(event);
                                    }
                                    None => {
                                        // Channel closed
                                        tracing::warn!("🎧 Event channel closed");
                                        yield Message::ServerDisconnected;
                                        break;
                                    }
                                }
                            }
                        } else {
                            tracing::warn!("🎧 No event receiver available (already taken?)");
                        }
                    },
                ),
            );
        }

        Subscription::batch(subscriptions)
    }

    fn view(&self) -> Element<Self::Message> {
        match self.current_page {
            Page::Chat => chat_page(self),
            Page::History => history_page(self),
            Page::Settings => settings_page(self),
        }
    }

    fn header_start(&self) -> Vec<Element<Self::Message>> {
        vec![self.view_menu_bar()]
    }

    fn nav_model(&self) -> Option<&widget::segmented_button::SingleSelectModel> {
        Some(&self.nav_model)
    }

    fn on_nav_select(
        &mut self,
        entity: widget::segmented_button::Entity,
    ) -> app::Task<Self::Message> {
        tracing::debug!("🖱️ on_nav_select called with entity: {:?}", entity);
        if let Some(nav_item) = self.nav_model.data::<NavItem>(entity).cloned() {
            tracing::info!("🖱️ NavItem found: {:?}", nav_item);
            match nav_item {
                NavItem::Page(page) => {
                    tracing::info!("🖱️ Navigating to page: {:?}", page);
                    self.current_page = page;
                }
                NavItem::Conversation(conv_id) => {
                    tracing::info!("🖱️ Loading conversation: {}", conv_id);
                    // Load the conversation
                    return app::Task::perform(
                        async move { cosmic::Action::App(Message::SelectConversation(conv_id)) },
                        |action| action,
                    );
                }
            }
        } else {
            tracing::warn!("🖱️ No NavItem found for entity: {:?}", entity);
        }
        app::Task::none()
    }
}

// ============================================================================
// View Implementations
// ============================================================================

impl LunaThinApp {
    fn view_menu_bar(&self) -> Element<Message> {
        // Simple menu using buttons instead of full menu bar for now
        widget::row()
            .push(widget::button::text("New").on_press(Message::NewConversation))
            .push(widget::button::text("Settings").on_press(Message::OpenSettings))
            .push(widget::button::text("About").on_press(Message::ShowAbout))
            .spacing(8)
            .into()
    }
}

