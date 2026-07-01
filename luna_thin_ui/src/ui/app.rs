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
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::client::{FileClient, LunaWsClient, ServerConfig};
use crate::server::dto::{
    ClientCommand, ConversationSummary, MemoryView, MessageView, SearchResult, ServerEvent,
};
use crate::ui::pages::{
    chat_page, history_page, memories_page, mcp_servers_page, settings_page, ChatPageState,
};
use crate::ui::handlers::{
    handle_connection_messages,
    handle_chat_messages,
    handle_history_memories_messages,
    handle_navigation_messages,
    handle_settings_messages,
    handle_server_event_messages,
    handle_tts_messages,
};
use crate::ui::icons;
use tokio::sync::broadcast::error::RecvError as BroadcastRecvError;

// ============================================================================
// Messages
// ============================================================================

#[derive(Debug, Clone)]
pub enum Message {
    // Input
    InputChanged(String),
    SendMessage,
    StopMessage,
    SummarizeConversation,
    ResumeAgent,

    // Navigation
    NavigateTo(Page),
    /// Return from a sidebar sub-page (History, Memories, …) to the chat view.
    BackToChat,
    SelectConversation(String),
    DeleteConversation(String),
    NewConversation,

    // History search & rename
    HistorySearchChanged(String),
    BeginRenameConversation(String),
    RenameDraftChanged(String),
    ConfirmRenameConversation,
    CancelRenameConversation,

    // Transient (internal) conversations
    ToggleShowInternal,
    ToggleNewChatInternal,
    SetConversationInternal { conversation_id: String, internal: bool },

    // Memories
    LoadMemories,
    LoadMoreMemories,
    MemoriesSearchChanged(String),
    BeginEditMemory(i64),
    MemoryDraftContentAction(text_editor::Action),
    MemoryDraftCategoryChanged(String),
    MemoryDraftImportanceChanged(String),
    ConfirmEditMemory,
    CancelEditMemory,
    DeleteMemory(i64),

    // Server events
    ServerEvent(ServerEvent),
    ServerConnected {
        insecure_warning: Option<String>,
        rest_base: String,
    },
    ServerDisconnected,
    ServerError(String),

    // Connection
    Connect,
    Disconnect,
    AutoReconnect,

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
    DismissInfo,
    DismissConnectionWarning,
    ToggleChatMenu,
    CloseChatMenu,
    ShowRecalledMemories(String),
    CloseRecalledMemories,

    // Menu
    ShowAbout,
    CloseAbout,
    OpenSettings,
    Quit,
    OpenUrl(String),

    // File attachments
    AttachFile,
    FileSelected(String),
    UploadSuccess {
        uid: String,
        original_name: String,
        stored_path: String,
    },
    FileUploadError(String),

    // Profile
    ChangeProfile(String),

    // MCP Servers
    LoadMCPServers,
    MCPServersLoaded(Vec<crate::server::dto::MCPServerInfo>),
    MCPServersLoadError(String),
    ToggleMCPServerExpand(String),

    // Tick for animations
    Tick(cosmic::iced::time::Instant),

    // Copy message
    CopyMessage(String),
    // Regenerate message (agent only)
    RegenerateMessage(String),
    // Retry message (user only) - retry from this user message
    RetryMessage(String), // message_id
    // TTS messages
    StartTts(String), // message_id
    StopTts,
    TtsStatusChanged(String), // "idle" | "speaking" | "listening" | "processing"
    TtsClientInitialized(Option<Arc<crate::services::tts_client::TtsClient>>),

    // Markdown image loading
    ImageLoaded { url: String, data: Vec<u8>, is_svg: bool },
    ImageLoadFailed { url: String, error: String },
    DownloadImage { url: String, title: String },
    ImageSaved(String),
    ImageSaveError(String),

    /// Incrementally parse markdown for a loaded conversation off the
    /// synchronous load path so long histories don't freeze the UI.
    /// Each tick parses a small chunk and chains into the next one until done,
    /// then triggers image fetching.
    ParseMarkdownChunk { start: usize },

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
    Memories,
    MCPServers,
    Settings,
}

/// Draft state while editing a memory inline on the Memories page.
#[derive(Debug)]
pub struct MemoryDraft {
    pub id: i64,
    pub content: text_editor::Content,
    pub category: String,
    pub importance: String,
}

impl Clone for MemoryDraft {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            content: text_editor::Content::with_text(&self.content.text()),
            category: self.category.clone(),
            importance: self.importance.clone(),
        }
    }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TtsStatus {
    Idle,
    Speaking,
}

// ============================================================================
// Markdown image cache
// ============================================================================

/// Cache state for a markdown image (remote, local, or inline data URI).
#[derive(Debug, Clone)]
pub enum ImageState {
    /// Download / decode in flight.
    Fetching,
    /// Raster image ready to display (PNG, JPEG, WebP, GIF, …).
    Raster {
        handle: cosmic::widget::image::Handle,
        data: Vec<u8>,
    },
    /// SVG data ready to display.
    Svg(Vec<u8>),
    /// Download or decode failed.
    Error(String),
}

impl ImageState {
    pub(crate) fn download_bytes(&self) -> Option<&[u8]> {
        match self {
            ImageState::Raster { data, .. } | ImageState::Svg(data) => Some(data),
            ImageState::Fetching | ImageState::Error(_) => None,
        }
    }
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
    /// Parsed markdown for assistant/summary bubbles (iced 0.14 `markdown::view` borrows items).
    pub markdown_items: Vec<cosmic::widget::markdown::Item>,
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
    /// Long-term memories injected for this user turn (memory RAG).
    pub recalled_memories: Vec<MemoryView>,
}

impl ChatMessage {
    pub(crate) fn parse_markdown_items(content: &str) -> Vec<cosmic::widget::markdown::Item> {
        cosmic::widget::markdown::parse(content).collect()
    }

    /// Create user message
    pub fn user(content: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            content,
            markdown_items: Vec::new(),
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
            recalled_memories: Vec::new(),
        }
    }
    
    /// Create assistant message
    pub fn assistant(content: String, reasoning_content: Option<String>) -> Self {
        let markdown_items = Self::parse_markdown_items(&content);
        Self {
            id: Uuid::new_v4().to_string(),
            content,
            markdown_items,
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
            recalled_memories: Vec::new(),
        }
    }
    
    /// Create tool request bubble
    pub fn tool_request(tool_call_id: String, name: String, params: String, status: &str) -> Self {
        Self {
            id: format!("{}_request", tool_call_id),
            content: format!("🧰 {}", name),
            markdown_items: Vec::new(),
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
            recalled_memories: Vec::new(),
        }
    }
    
    /// Create tool result bubble
    pub fn tool_result(tool_call_id: String, name: String, result: String, is_error: bool) -> Self {
        Self {
            id: format!("{}_result", tool_call_id),
            content: format!("🧰 {}", name),
            markdown_items: Vec::new(),
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
            recalled_memories: Vec::new(),
        }
    }
    
    /// Create summary message
    pub fn summary(content: String, summarized_count: usize) -> Self {
        let markdown_items = Self::parse_markdown_items(&content);
        Self {
            id: Uuid::new_v4().to_string(),
            content,
            markdown_items,
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
            recalled_memories: Vec::new(),
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
// Main Application
// ============================================================================

pub struct LunaThinApp {
    pub core: Core,

    // Server connectivity
    pub server_config: ServerConfig,
    pub ws_client: Arc<RwLock<LunaWsClient>>,
    pub file_client: Option<FileClient>,
    /// Resolved REST API base (https:// or http://) after connect.
    pub rest_base: Option<String>,
    pub connection_status: ConnectionStatus,
    
    // WebSocket event sender (stored for subscribing to broadcast channel)
    // The broadcast sender is stored in ws_client, we just track connection status

    // Server state
    pub current_conversation_id: Option<String>,
    pub conversations: Vec<ConversationSummary>,
    pub messages: Vec<ChatMessage>,
    pub profiles: Vec<String>,
    pub current_profile: String,
    /// Ephemeral token for static file URLs (from HealthOk).
    pub static_token: Option<String>,
    pub mcp_servers: Vec<crate::server::dto::MCPServerInfo>,
    pub mcp_expanded_servers: HashSet<String>,

    // History page
    pub history_search: String,
    pub history_search_results: Vec<SearchResult>,
    pub renaming_conversation: Option<(String, String)>,
    /// When true, history list/search includes internal (transient) conversations.
    pub show_internal: bool,
    /// Next new chat (first message) is created as internal/transient.
    pub new_chat_internal: bool,
    /// Internal flag of the active conversation (from server).
    pub current_conversation_internal: bool,

    // Memories page
    pub memories: Vec<MemoryView>,
    pub memories_search: String,
    pub memories_has_more: bool,
    /// Offset used for the in-flight `ListMemories` request (0 = replace list).
    memories_fetch_offset: u32,
    pub editing_memory: Option<MemoryDraft>,

    // Streaming state — conversations with an active agent loop on the server
    pub streaming_conversations: HashSet<String>,
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
    pub about: widget::about::About,

    // Chat page state
    pub chat_page: ChatPageState,

    // Input state
    pub input_text: String,

    // Expanded states
    pub expanded_reasoning: HashSet<usize>,
    pub expanded_summaries: HashSet<usize>,
    pub expanded_tools: HashSet<String>,

    // TTS state
    pub tts_client: Option<Arc<crate::services::tts_client::TtsClient>>,
    pub tts_status: TtsStatus,
    pub current_tts_message_id: Option<String>, // ID of message being spoken
    pub pending_auto_connect: bool, // True if we need to auto-connect after TTS init
    pub pending_retry_input: Option<String>, // Input text to restore after conversation reload
    pub user_disconnect_flag: Arc<AtomicBool>,
    pub reconnect_in_progress: bool,

    // Error display
    pub inline_error: Option<String>,
    /// Informational message (e.g. summarization started/finished) – show as info banner.
    pub inline_info: Option<String>,
    pub connection_warning: Option<String>,
    pub chat_menu_open: bool,
    pub recalled_memories_popup: Option<String>,
    pub pending_recalled_memories: std::collections::HashMap<String, Vec<MemoryView>>,

    /// Upload UUIDs waiting to be sent with the next message (`SendMessage.attachment_ids`).
    pub pending_attachment_ids: Vec<String>,

    /// Remote image cache for markdown rendering.
    pub image_cache: std::collections::HashMap<String, ImageState>,

    // Settings input
    pub settings_host: String,
    pub settings_port: String,
    pub settings_api_key: String,
}

// ============================================================================
// Constants
// ============================================================================

const MAX_NAV_CONVERSATIONS: usize = 11;
const CONVERSATION_LIST_LIMIT: u32 = 20;
const MEMORIES_LIST_LIMIT: u32 = 100;
const CONVERSATION_TITLE_MAX_LEN: usize = 28;
const CONVERSATION_TITLE_TRUNCATE_LEN: usize = 25;
const FRAME_RATE_MS: u64 = 16; // ~60fps max
const TYPING_INDICATOR_INTERVAL_MS: u64 = 250;

impl LunaThinApp {
    fn new(core: Core) -> Self {
        let server_config = ServerConfig::load()
            .unwrap_or_else(|e| {
                tracing::warn!("Failed to load server config: {}, using defaults", e);
                ServerConfig::default()
            });

        Self {
            core,
            settings_host: server_config.host.clone(),
            settings_port: server_config.port.to_string(),
            settings_api_key: server_config.api_key.clone(),
            server_config,
            ws_client: Arc::new(RwLock::new(LunaWsClient::new())),
            file_client: None,
            rest_base: None,
            connection_status: ConnectionStatus::Disconnected,
            current_conversation_id: None,
            conversations: Vec::new(),
            messages: Vec::new(),
            profiles: Vec::new(),
            current_profile: String::new(),
            static_token: None,
            mcp_servers: Vec::new(),
            mcp_expanded_servers: HashSet::new(),
            history_search: String::new(),
            history_search_results: Vec::new(),
            renaming_conversation: None,
            show_internal: false,
            new_chat_internal: false,
            current_conversation_internal: false,
            memories: Vec::new(),
            memories_search: String::new(),
            memories_has_more: false,
            memories_fetch_offset: 0,
            editing_memory: None,
            streaming_conversations: HashSet::new(),
            streaming_content: String::new(),
            reasoning_content: String::new(),
            current_assistant_bubble_id: None,
            current_page: Page::Settings,
            key_binds: Self::create_key_binds(),
            nav_model: Self::create_nav_model(),
            show_about: false,
            about: Self::create_about_widget(),
            chat_page: ChatPageState::default(),
            input_text: String::new(),
            expanded_reasoning: HashSet::new(),
            expanded_summaries: HashSet::new(),
            expanded_tools: HashSet::new(),
            tts_client: None, // Will be initialized in init()
            tts_status: TtsStatus::Idle,
            current_tts_message_id: None,
            pending_auto_connect: false,
            pending_retry_input: None,
            user_disconnect_flag: Arc::new(AtomicBool::new(false)),
            reconnect_in_progress: false,
            inline_error: None,
            inline_info: None,
            connection_warning: None,
            chat_menu_open: false,
            recalled_memories_popup: None,
            pending_recalled_memories: std::collections::HashMap::new(),
            pending_attachment_ids: Vec::new(),
            image_cache: std::collections::HashMap::new(),
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
            .icon(widget::icon::icon(icons::get_handle("chat-bubble-empty-symbolic", 16)))
            .data(NavItem::Page(Page::Chat));
        model.insert()
            .text("More history")
            .icon(widget::icon::icon(icons::get_handle("list-large-symbolic", 16)))
            .data(NavItem::Page(Page::History))
            .divider_above(true);
        model.insert()
            .text("Settings")
            .icon(widget::icon::icon(icons::get_handle("settings-symbolic", 16)))
            .data(NavItem::Page(Page::Settings))
            .divider_above(true);
        model
    }

    pub(crate) fn update_nav_model(&mut self) {
        let mut model = widget::segmented_button::ModelBuilder::default().build();

        // New Chat (only if no active conversation)
        if self.current_conversation_id.is_none() {
            model.insert()
                .text("New Chat")
                .icon(widget::icon::icon(icons::get_handle("chat-bubble-empty-symbolic", 16)))
                .data(NavItem::Page(Page::Chat));
        }

        // Recent conversations (max to match original)
        for conv in self
            .conversations
            .iter()
            .filter(|c| self.show_internal || !c.internal)
            .take(MAX_NAV_CONVERSATIONS)
        {
            let title = if conv.title.chars().count() > CONVERSATION_TITLE_MAX_LEN {
                let truncated: String = conv.title
                    .chars()
                    .take(CONVERSATION_TITLE_TRUNCATE_LEN)
                    .collect();
                format!("{}...", truncated)
            } else {
                conv.title.clone()
            };
            model.insert()
                .text(title)
                .icon(widget::icon::icon(icons::get_handle("chat-bubble-text-symbolic", 16)))
                .data(NavItem::Conversation(conv.id.clone()));
        }

        // More history
        model.insert()
            .text("More history")
            .icon(widget::icon::icon(icons::get_handle("list-large-symbolic", 16)))
            .data(NavItem::Page(Page::History))
            .divider_above(true);

        // Memories
        model.insert()
            .text("Memories")
            .icon(widget::icon::icon(icons::get_handle("emblem-favorite-symbolic", 16)))
            .data(NavItem::Page(Page::Memories));

        // MCP Servers
        model.insert()
            .text("MCP Servers")
            .icon(widget::icon::icon(icons::get_handle("network-server-symbolic", 16)))
            .data(NavItem::Page(Page::MCPServers))
            .divider_above(true);

        // Settings
        model.insert()
            .text("Settings")
            .icon(widget::icon::icon(icons::get_handle("settings-symbolic", 16)))
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

    fn create_about_widget() -> widget::about::About {
        widget::about::About::default()
            .name("Luna AI Thin Client")
            .icon(cosmic::widget::icon::Named::new(Self::APP_ID))
            .version("0.1.0")
            .license("GPL-3.0")
            .links([
                ("Repository", "https://github.com/digit1024/LunaAI"),
                ("Issues", "https://github.com/digit1024/LunaAI/issues"),
                ("Documentation", "https://github.com/digit1024/LunaAI#readme"),
            ])
            .developers([
                ("Michał Banaś", "https://github.com/digit1024")
            ])
            .comments("A thin client that connects to a Luna AI server via WebSocket. All processing happens on the server - this app only provides the interface.")
    }

    // ========================================================================
    // Markdown image fetching
    // ========================================================================

    /// Scans `items` for image URLs not yet in the cache, marks them as
    /// `Fetching`, and returns a batched `Task` that resolves each one.
    ///
    /// Supported URL schemes:
    /// - `luna-static:`           — rewritten to `/api/static/{token}/...` at fetch time
    /// - `http://` / `https://`  — fetched via reqwest
    /// - `file://`               — read from the local filesystem
    /// - `data:<mime>;base64,`   — decoded inline (no I/O)
    pub(crate) fn fetch_missing_images(
        &mut self,
        items: &[cosmic::widget::markdown::Item],
    ) -> app::Task<Message> {
        use crate::ui::widgets::markdown_viewer::collect_image_urls;
        let urls = collect_image_urls(items);
        if urls.is_empty() {
            return app::Task::none();
        }

        let mut tasks: Vec<app::Task<Message>> = Vec::new();
        for url in urls {
            match self.image_cache.get(&url) {
                Some(ImageState::Raster { .. }) | Some(ImageState::Svg(_)) => continue,
                Some(ImageState::Fetching) => continue,
                Some(ImageState::Error(_)) | None => {}
            }

            if let Some(msg) = Self::try_resolve_inline(&url) {
                self.image_cache.insert(url.clone(), ImageState::Fetching);
                tasks.push(app::Task::perform(
                    async move { msg },
                    cosmic::Action::App,
                ));
                continue;
            }

            if url.starts_with("luna-static:") {
                let Some(fetch_urls) = self.resolve_luna_static_fetch_urls(&url) else {
                    // No static_token yet — leave uncached so we retry after health_ok.
                    continue;
                };
                self.image_cache.insert(url.clone(), ImageState::Fetching);
                let marker = url.clone();
                tasks.push(app::Task::perform(
                    async move {
                        let mut last_err = String::new();
                        for fetch_url in &fetch_urls {
                            match reqwest::get(fetch_url).await {
                                Ok(resp) if resp.status().is_success() => {
                                    match resp.bytes().await {
                                        Ok(bytes) => {
                                            let is_svg = Self::is_svg_bytes(&bytes);
                                            return Message::ImageLoaded {
                                                url: marker,
                                                data: bytes.to_vec(),
                                                is_svg,
                                            };
                                        }
                                        Err(e) => last_err = e.to_string(),
                                    }
                                }
                                Ok(resp) => last_err = format!("HTTP {}", resp.status()),
                                Err(e) => last_err = e.to_string(),
                            }
                            tracing::debug!(
                                fetch_url = %fetch_url,
                                error = %last_err,
                                "Static image fetch attempt failed"
                            );
                        }
                        Message::ImageLoadFailed {
                            url: marker,
                            error: last_err,
                        }
                    },
                    cosmic::Action::App,
                ));
                continue;
            }

            if !(url.starts_with("http://") || url.starts_with("https://")) {
                continue;
            }

            self.image_cache.insert(url.clone(), ImageState::Fetching);
            let url_clone = url.clone();
            tasks.push(app::Task::perform(
                async move {
                    match reqwest::get(&url_clone).await {
                        Ok(resp) => match resp.bytes().await {
                            Ok(bytes) => {
                                let is_svg = Self::is_svg_bytes(&bytes);
                                Message::ImageLoaded { url: url_clone, data: bytes.to_vec(), is_svg }
                            }
                            Err(e) => Message::ImageLoadFailed {
                                url: url_clone,
                                error: e.to_string(),
                            },
                        },
                        Err(e) => Message::ImageLoadFailed {
                            url: url_clone,
                            error: e.to_string(),
                        },
                    }
                },
                cosmic::Action::App,
            ));
        }
        app::Task::batch(tasks)
    }

    /// Rewrite a token-free `luna-static:{conv}/{file}` marker to fetchable URL(s).
    fn resolve_luna_static_fetch_urls(&self, marker: &str) -> Option<Vec<String>> {
        let rest = marker.strip_prefix("luna-static:")?;
        let token = self.static_token.as_ref()?;
        let static_path = format!("/api/static/{}/{}", token, rest);

        if let Some(base) = &self.rest_base {
            return Some(vec![format!(
                "{}{}",
                base.trim_end_matches('/'),
                static_path
            )]);
        }

        Some(
            self.server_config
                .http_rest_base_uris()
                .into_iter()
                .map(|base| format!("{}{}", base.trim_end_matches('/'), static_path))
                .collect(),
        )
    }

    /// Resolve a `data:` URI or `file://` URL into an `ImageLoaded` /
    /// `ImageLoadFailed` message without doing any async I/O.
    fn try_resolve_inline(url: &str) -> Option<Message> {
        if let Some(rest) = url.strip_prefix("data:") {
            // data:[<mediatype>][;base64],<encoded>
            if let Some(comma_pos) = rest.find(',') {
                let meta = &rest[..comma_pos];
                let encoded = &rest[comma_pos + 1..];
                let is_base64 = meta.ends_with(";base64");
                let mime = meta.trim_end_matches(";base64");
                let is_svg = mime.contains("svg");
                if is_base64 {
                    use base64::Engine as _;
                    match base64::engine::general_purpose::STANDARD.decode(encoded) {
                        Ok(bytes) => {
                            return Some(Message::ImageLoaded {
                                url: url.to_string(),
                                data: bytes,
                                is_svg,
                            });
                        }
                        Err(e) => {
                            return Some(Message::ImageLoadFailed {
                                url: url.to_string(),
                                error: format!("base64 decode: {e}"),
                            });
                        }
                    }
                }
            }
            Some(Message::ImageLoadFailed {
                url: url.to_string(),
                error: "Unsupported data URI (only base64 is supported)".into(),
            })
        } else if let Some(path) = url.strip_prefix("file://") {
            match std::fs::read(path) {
                Ok(bytes) => {
                    let is_svg = path.ends_with(".svg") || path.ends_with(".svgz")
                        || Self::is_svg_bytes(&bytes);
                    Some(Message::ImageLoaded {
                        url: url.to_string(),
                        data: bytes,
                        is_svg,
                    })
                }
                Err(e) => Some(Message::ImageLoadFailed {
                    url: url.to_string(),
                    error: format!("file read: {e}"),
                }),
            }
        } else {
            None
        }
    }

    fn is_svg_bytes(bytes: &[u8]) -> bool {
        // Check for SVG XML signature (may have a BOM or whitespace before '<')
        let trimmed = bytes.iter().position(|&b| b != b' ' && b != b'\t' && b != b'\n' && b != b'\r')
            .map(|i| &bytes[i..])
            .unwrap_or(bytes);
        trimmed.starts_with(b"<svg") || trimmed.starts_with(b"<?xml")
            || trimmed.windows(4).take(20).any(|w| w == b"<svg")
    }

    pub(crate) fn send_command(&self, command: ClientCommand) {
        let ws_client = self.ws_client.clone();
        tokio::spawn(async move {
            let client = ws_client.read().await;
            client.send(command);
        });
    }

    pub(crate) fn set_ws_streaming(&self, streaming: bool) {
        let ws_client = self.ws_client.clone();
        tokio::spawn(async move {
            let client = ws_client.read().await;
            client.set_streaming(streaming);
        });
    }

    /// True when the conversation currently open in the chat UI has an active stream.
    pub fn is_current_streaming(&self) -> bool {
        match &self.current_conversation_id {
            Some(id) => self.streaming_conversations.contains(id),
            None => false,
        }
    }

    /// True when the animated typing dots are visible (waiting for first token / tools).
    fn needs_typing_indicator_tick(&self) -> bool {
        if !self.is_current_streaming() {
            return false;
        }
        let has_running_tools = self.messages.iter().any(|m| {
            m.bubble_type == BubbleType::ToolRequest
                && m.tool_status.as_deref() == Some("running")
        });
        let has_streaming_bubble = self.messages.iter().any(|m| m.is_streaming);
        !has_streaming_bubble && !has_running_tools
    }

    fn is_viewing_conversation(&self, conversation_id: &str) -> bool {
        self.current_conversation_id.as_deref() == Some(conversation_id)
    }

    fn mark_conversation_streaming(&mut self, conversation_id: String) {
        if self.streaming_conversations.insert(conversation_id) {
            self.sync_ws_streaming();
        }
    }

    fn unmark_conversation_streaming(&mut self, conversation_id: &str) {
        if self.streaming_conversations.remove(conversation_id) {
            self.sync_ws_streaming();
        }
    }

    fn sync_ws_streaming(&self) {
        self.set_ws_streaming(!self.streaming_conversations.is_empty());
    }

    /// Helper: List conversations with default parameters
    pub(crate) fn list_conversations(&self) {
        self.send_command(ClientCommand::ListConversations {
            query: None,
            limit: Some(CONVERSATION_LIST_LIMIT),
            offset: None,
            include_internal: Some(self.show_internal),
        });
    }

    /// Search conversation message history via FTS.
    pub(crate) fn search_conversations(&self, query: &str) {
        self.send_command(ClientCommand::ListConversations {
            query: Some(query.to_string()),
            limit: Some(CONVERSATION_LIST_LIMIT),
            offset: None,
            include_internal: Some(self.show_internal),
        });
    }

    /// List or search long-term memories (`offset` 0 replaces the list; higher appends).
    pub(crate) fn list_memories(&mut self, query: Option<String>, offset: u32) {
        self.memories_fetch_offset = offset;
        self.send_command(ClientCommand::ListMemories {
            query,
            limit: Some(MEMORIES_LIST_LIMIT),
            offset: Some(offset),
        });
    }

    fn sort_memories_by_updated(memories: &mut [MemoryView]) {
        memories.sort_by(|a, b| b.updated_at.cmp(&a.updated_at).then(b.id.cmp(&a.id)));
    }

    fn sync_last_user_message_id(&mut self, message_id: String) {
        if let Some(msg) = self
            .messages
            .iter_mut()
            .rev()
            .find(|m| m.bubble_type == BubbleType::User)
        {
            let old_id = msg.id.clone();
            msg.id = message_id.clone();
            if let Some(memories) = self.pending_recalled_memories.remove(&message_id) {
                msg.recalled_memories = memories;
            } else if let Some(memories) = self.pending_recalled_memories.remove(&old_id) {
                msg.recalled_memories = memories;
            }
        }
    }

    fn apply_recalled_memories(&mut self, message_id: &str, memories: Vec<MemoryView>) {
        if memories.is_empty() {
            return;
        }
        if let Some(msg) = self.messages.iter_mut().find(|m| m.id == message_id) {
            msg.recalled_memories = memories;
        } else {
            self.pending_recalled_memories
                .insert(message_id.to_string(), memories);
        }
    }

    /// Helper: On connection established - send initial commands.
    /// If we have a current conversation, re-subscribe so we receive any in-flight stream (broadcast).
    pub(crate) fn on_connect(&mut self) {
        self.send_command(ClientCommand::HealthCheck);
        self.list_conversations();
        self.send_command(ClientCommand::ListProfiles);
        if let Some(ref id) = self.current_conversation_id {
            self.send_command(ClientCommand::LoadConversation {
                conversation_id: id.clone(),
            });
        }
    }

    // ========================================================================
    // Message mapping (like mobile app's _mapMessages)
    // ========================================================================
    
    /// Map server messages to UI messages (like mobile app)
    /// Tool calls become separate request/result bubbles
    fn map_messages_from_server(&self, messages: &[MessageView]) -> Vec<ChatMessage> {
        let mut result = Vec::new();
        
        for m in messages {
            if let Some(tool_call_id) = m.tool_call_id.as_deref() {
                // Tool message - single bubble with params and result
                let tool_call_id = tool_call_id.to_string();
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
                // Summary message — defer markdown parse (see chunked pass below).
                result.push(ChatMessage {
                    id: Uuid::new_v4().to_string(),
                    content: m.content.clone(),
                    markdown_items: Vec::new(),
                    bubble_type: BubbleType::Summary,
                    is_error: false,
                    reasoning_content: None,
                    summarized_count: Some(m.summarized_count.unwrap_or(0)),
                    tool_call_id: None,
                    tool_name: None,
                    tool_params: None,
                    tool_result: None,
                    tool_status: None,
                    is_streaming: false,
                    recalled_memories: Vec::new(),
                });
            } else {
                // Regular user/assistant message. Markdown parsing is deferred to
                // a chunked background pass (see `Message::ParseMarkdownChunk`) so
                // a long history doesn't block the iced update loop. The
                // assistant bubble falls back to plain text while items are empty.
                let bubble_type = if m.role == "user" {
                    BubbleType::User
                } else {
                    BubbleType::Assistant
                };

                result.push(ChatMessage {
                    id: m.id.clone(),
                    content: m.content.clone(),
                    markdown_items: Vec::new(),
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
                    recalled_memories: if m.role == "user" {
                        m.recalled_memories.clone()
                    } else {
                        Vec::new()
                    },
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
                // Plain text while streaming; markdown parsed once in complete_assistant.
                msg.markdown_items.clear();
                msg.is_streaming = true;
                return;
            }
        }
        
        // Create new assistant bubble
        let new_id = Uuid::new_v4().to_string();
        self.current_assistant_bubble_id = Some(new_id.clone());
        
        let content = chunk.to_string();
        self.messages.push(ChatMessage {
            id: new_id,
            content,
            markdown_items: Vec::new(),
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
            recalled_memories: Vec::new(),
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
            markdown_items: Vec::new(),
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
            recalled_memories: Vec::new(),
        });
    }
    
    /// Complete assistant message.
    /// Returns a Task for any image fetches triggered by the final content.
    ///
    /// NOTE: Do not remove the conversation from `streaming_conversations` here.
    /// In an agentic loop, AssistantComplete fires before tool execution begins;
    /// the per-conversation streaming flag must stay set so stop/typing remain visible.
    fn complete_assistant(
        &mut self,
        content: &str,
        reasoning_content: Option<&str>,
    ) -> app::Task<Message> {
        self.streaming_content.clear();
        self.reasoning_content.clear();

        let items = ChatMessage::parse_markdown_items(content);
        let fetch_task = self.fetch_missing_images(&items);

        if let Some(ref bubble_id) = self.current_assistant_bubble_id.clone() {
            if let Some(msg) = self.messages.iter_mut().find(|m| &m.id == bubble_id) {
                msg.content = content.to_string();
                msg.markdown_items = items;
                msg.reasoning_content = reasoning_content.map(|s| s.to_string());
                msg.is_streaming = false;
                return fetch_task;
            }
        }

        if let Some(msg) = self
            .messages
            .iter_mut()
            .rev()
            .find(|m| m.bubble_type == BubbleType::Assistant)
        {
            msg.content = content.to_string();
            msg.markdown_items = items;
            msg.reasoning_content = reasoning_content.map(|s| s.to_string());
            msg.is_streaming = false;
        } else {
            self.messages.push(ChatMessage::assistant(
                content.to_string(),
                reasoning_content.map(|s| s.to_string()),
            ));
        }
        fetch_task
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

    pub(crate) fn handle_server_event(&mut self, event: ServerEvent) -> app::Task<Message> {
        match event {
            ServerEvent::HealthOk { profile, static_token, .. } => {
                tracing::debug!("HealthOk received, profile: {}", profile);
                self.connection_status = ConnectionStatus::Connected;
                self.current_profile = profile;
                self.static_token = (!static_token.is_empty()).then_some(static_token);
                let chunk_items: Vec<_> = self
                    .messages
                    .iter()
                    .flat_map(|m| m.markdown_items.iter().cloned())
                    .collect();
                if !chunk_items.is_empty() {
                    return self.fetch_missing_images(&chunk_items);
                }
            }
            ServerEvent::Error { message } => {
                if is_transport_error(&message) {
                    self.inline_error = None;
                    return app::Task::done(cosmic::Action::App(Message::ServerDisconnected));
                }
                if let Some(id) = self.current_conversation_id.clone() {
                    if self.streaming_conversations.contains(&id) {
                        self.unmark_conversation_streaming(&id);
                    }
                }
                self.inline_error = Some(message);
            }
            ServerEvent::Info { message } => {
                self.inline_info = Some(message);
            }
            ServerEvent::ConversationCreated { conversation_id } => {
                // Just set the conversation ID - don't clear messages!
                // The user message we added is already there and should be preserved
                self.current_conversation_id = Some(conversation_id);
                self.pending_attachment_ids.clear();
                self.list_conversations();
            }
            ServerEvent::ConversationLoaded { conversation } => {
                tracing::info!("📥 ConversationLoaded: {} ({} messages)", conversation.id, conversation.messages.len());
                self.current_conversation_id = Some(conversation.id.clone());
                self.current_conversation_internal = conversation.internal;
                self.pending_attachment_ids.clear();
                self.current_profile = conversation.profile_name.unwrap_or_default();
                self.current_assistant_bubble_id = None;
                self.image_cache.clear();

                // Map messages like mobile app - tool calls become separate bubbles
                self.messages = self.map_messages_from_server(&conversation.messages);

                self.current_page = Page::Chat;
                self.update_nav_model();

                // Restore pending retry input AFTER messages are updated
                if let Some(retry_input) = self.pending_retry_input.take() {
                    self.input_text = retry_input.clone();
                    self.chat_page.input_content =
                        cosmic::widget::text_editor::Content::with_text(&retry_input);
                    tracing::info!("Restored retry input text: {} chars", self.input_text.len());
                } else {
                    self.input_text.clear();
                    self.chat_page.input_content = cosmic::widget::text_editor::Content::new();
                }

                // Defer markdown parsing and image fetching: chain through
                // `ParseMarkdownChunk` so the UI thread can paint the first
                // bubbles immediately on long conversations.
                return app::Task::done(cosmic::Action::App(
                    Message::ParseMarkdownChunk { start: 0 },
                ));
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
            ServerEvent::MessageAccepted { message_id, .. } => {
                self.sync_last_user_message_id(message_id);
            }
            ServerEvent::StreamingStarted { conversation_id } => {
                self.mark_conversation_streaming(conversation_id.clone());
                if self.is_viewing_conversation(&conversation_id) {
                    self.current_assistant_bubble_id = None;
                    self.streaming_content.clear();
                    self.reasoning_content.clear();
                    crate::ui::audio::AudioService::play_sound("typing.mp3");
                }
            }
            ServerEvent::AssistantDelta { conversation_id, chunk, .. } => {
                if self.is_viewing_conversation(&conversation_id) {
                    self.apply_assistant_delta(&chunk);
                }
            }
            ServerEvent::ReasoningContentDelta { conversation_id, chunk } => {
                if self.is_viewing_conversation(&conversation_id) {
                    self.apply_reasoning_delta(&chunk);
                }
            }
            ServerEvent::AssistantComplete {
                conversation_id,
                content,
                reasoning_content,
                ..
            } => {
                if self.is_viewing_conversation(&conversation_id) {
                    return self.complete_assistant(&content, reasoning_content.as_deref());
                }
            }
            ServerEvent::ToolPlanned {
                conversation_id,
                tools,
                ..
            } => {
                if self.is_viewing_conversation(&conversation_id) {
                    self.current_assistant_bubble_id = None;
                    self.add_tool_request_bubbles(&tools);
                }
            }
            ServerEvent::ToolStarted {
                conversation_id,
                tool_call_id,
                name,
                params_json,
                ..
            } => {
                if self.is_viewing_conversation(&conversation_id) {
                    let params = serde_json::to_string_pretty(&params_json).unwrap_or_default();
                    self.update_tool_request(&tool_call_id, "running", &name, &params);
                }
            }
            ServerEvent::ToolResult {
                conversation_id,
                tool_call_id,
                name,
                result_json,
                ..
            } => {
                if self.is_viewing_conversation(&conversation_id) {
                    let result = serde_json::to_string_pretty(&result_json).unwrap_or_default();
                    self.add_tool_result_bubble(&tool_call_id, &name, &result, false);
                    crate::ui::audio::AudioService::play_sound("tool.mp3");
                }
            }
            ServerEvent::ToolError {
                conversation_id,
                tool_call_id,
                name,
                error,
                ..
            } => {
                if self.is_viewing_conversation(&conversation_id) {
                    self.add_tool_result_bubble(&tool_call_id, &name, &error, true);
                }
            }
            ServerEvent::ConversationComplete { conversation_id } => {
                self.unmark_conversation_streaming(&conversation_id);
                if self.is_viewing_conversation(&conversation_id) {
                    self.current_assistant_bubble_id = None;
                    crate::ui::audio::AudioService::play_sound("done.mp3");
                    self.list_conversations();
                }
            }
            ServerEvent::ConversationDeleted { conversation_id } => {
                self.unmark_conversation_streaming(&conversation_id);
                if self.current_conversation_id.as_ref() == Some(&conversation_id) {
                    self.current_conversation_id = None;
                    self.messages.clear();
                    self.current_assistant_bubble_id = None;
                }
                self.conversations.retain(|c| c.id != conversation_id);
                self.update_nav_model();
            }
            ServerEvent::ConversationRenamed {
                conversation_id,
                title,
            } => {
                if let Some(conv) = self
                    .conversations
                    .iter_mut()
                    .find(|c| c.id == conversation_id)
                {
                    conv.title = title.clone();
                }
                if self.renaming_conversation.as_ref().map(|(id, _)| id) == Some(&conversation_id) {
                    self.renaming_conversation = None;
                }
                self.update_nav_model();
            }
            ServerEvent::ConversationInternalChanged {
                conversation_id,
                internal,
            } => {
                if self.current_conversation_id.as_deref() == Some(conversation_id.as_str()) {
                    self.current_conversation_internal = internal;
                }
                if internal && !self.show_internal {
                    self.conversations.retain(|c| c.id != conversation_id);
                } else if let Some(conv) = self
                    .conversations
                    .iter_mut()
                    .find(|c| c.id == conversation_id)
                {
                    conv.internal = internal;
                } else if internal && self.show_internal {
                    self.list_conversations();
                }
                self.update_nav_model();
            }
            ServerEvent::SearchResults { results } => {
                self.history_search_results = results;
            }
            ServerEvent::MemoriesList { memories } => {
                let batch_len = memories.len();
                if self.memories_fetch_offset == 0 {
                    self.memories = memories;
                } else {
                    let existing_ids: std::collections::HashSet<i64> =
                        self.memories.iter().map(|m| m.id).collect();
                    for memory in memories {
                        if !existing_ids.contains(&memory.id) {
                            self.memories.push(memory);
                        }
                    }
                }
                Self::sort_memories_by_updated(&mut self.memories);
                self.memories_has_more = batch_len >= MEMORIES_LIST_LIMIT as usize;
            }
            ServerEvent::MemoryUpdated { memory } => {
                if let Some(entry) = self.memories.iter_mut().find(|m| m.id == memory.id) {
                    *entry = memory.clone();
                } else {
                    self.memories.push(memory.clone());
                }
                Self::sort_memories_by_updated(&mut self.memories);
                if self.editing_memory.as_ref().map(|d| d.id) == Some(memory.id) {
                    self.editing_memory = None;
                }
            }
            ServerEvent::MemoryDeleted { id } => {
                self.memories.retain(|m| m.id != id);
                if self.editing_memory.as_ref().map(|d| d.id) == Some(id) {
                    self.editing_memory = None;
                }
            }
            ServerEvent::StreamingStopped { conversation_id } => {
                self.unmark_conversation_streaming(&conversation_id);
                if self.is_viewing_conversation(&conversation_id) {
                    self.current_assistant_bubble_id = None;
                }
            }
            ServerEvent::MemoriesRecalled {
                conversation_id,
                message_id,
                memories,
                ..
            } => {
                if self.is_viewing_conversation(&conversation_id) {
                    self.apply_recalled_memories(&message_id, memories);
                }
            }
        }
        app::Task::none()
    }
}

fn is_transport_error(message: &str) -> bool {
    message.starts_with("WebSocket error:")
        || message.starts_with("Connection timed out")
        || message == "Connection closed by server"
}

/// Identity for `Subscription::run_with` (iced 0.14; replaces removed `run_with_id`).
#[derive(Clone)]
struct WsSubscriptionClient(std::sync::Arc<tokio::sync::RwLock<LunaWsClient>>);

impl std::hash::Hash for WsSubscriptionClient {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        (std::sync::Arc::as_ptr(&self.0) as usize).hash(state);
    }
}

fn ws_client_event_stream(
    id: &WsSubscriptionClient,
) -> std::pin::Pin<Box<dyn futures::Stream<Item = Message> + Send>> {
    let ws_client = id.0.clone();
    Box::pin(async_stream::stream! {
        tracing::info!("🎧 WS subscription started");

        let rx = {
            let client = ws_client.read().await;
            client.subscribe()
        };

        if let Some(mut rx) = rx {
            tracing::info!("🎧 Subscribed to event channel, starting event loop");
            let mut last_yield = std::time::Instant::now();
            let min_yield_interval = std::time::Duration::from_millis(FRAME_RATE_MS);
            let mut event_buffer = Vec::new();
            let max_buffer_size = 100;

            loop {
                // Block on recv while idle — no timer arm, so no ~60 Hz wakeups.
                match rx.recv().await {
                    Ok(event) => event_buffer.push(event),
                    Err(BroadcastRecvError::Lagged(skipped)) => {
                        tracing::warn!("🎧 Lagged behind by {} messages, continuing...", skipped);
                        event_buffer.clear();
                        continue;
                    }
                    Err(BroadcastRecvError::Closed) => {
                        for buffered_event in event_buffer.drain(..) {
                            yield Message::ServerEvent(buffered_event);
                        }
                        tracing::info!("🎧 Event channel closed (disconnected)");
                        yield Message::ServerDisconnected;
                        break;
                    }
                }

                // Coalesce bursts before yielding to the UI thread.
                loop {
                    let should_flush = event_buffer.len() >= max_buffer_size
                        || last_yield.elapsed() >= min_yield_interval;

                    if should_flush {
                        for buffered_event in event_buffer.drain(..) {
                            tracing::debug!("🎧 Yielding buffered event: {:?}", buffered_event);
                            yield Message::ServerEvent(buffered_event);
                        }
                        last_yield = std::time::Instant::now();
                        break;
                    }

                    let remaining = min_yield_interval.saturating_sub(last_yield.elapsed());
                    let flush_timer = tokio::time::sleep(remaining);
                    tokio::pin!(flush_timer);

                    tokio::select! {
                        result = rx.recv() => {
                            match result {
                                Ok(event) => event_buffer.push(event),
                                Err(BroadcastRecvError::Lagged(skipped)) => {
                                    tracing::warn!("🎧 Lagged behind by {} messages, continuing...", skipped);
                                    event_buffer.clear();
                                }
                                Err(BroadcastRecvError::Closed) => {
                                    for buffered_event in event_buffer.drain(..) {
                                        yield Message::ServerEvent(buffered_event);
                                    }
                                    tracing::info!("🎧 Event channel closed (disconnected)");
                                    yield Message::ServerDisconnected;
                                    return;
                                }
                            }
                        }
                        _ = flush_timer.as_mut() => {
                            for buffered_event in event_buffer.drain(..) {
                                tracing::debug!("🎧 Yielding buffered event (timer): {:?}", buffered_event);
                                yield Message::ServerEvent(buffered_event);
                            }
                            last_yield = std::time::Instant::now();
                            break;
                        }
                    }
                }
            }
        } else {
            tracing::warn!("🎧 Not connected, cannot subscribe to events");
            yield Message::ServerDisconnected;
        }
    })
}

// ============================================================================
// Application Implementation
// ============================================================================

impl Application for LunaThinApp {
    type Executor = cosmic::executor::Default;
    type Flags = ();
    type Message = Message;
    const APP_ID: &'static str = "com.github.digit1024.luna";

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
        
        let mut app = Self::new(core);
        
        // Initialize TTS client (async, will be set when ready)
        let tts_init_task = app::Task::perform(
            async move {
                match crate::services::tts_client::TtsClient::new().await {
                    Ok(client) => {
                        tracing::info!("TTS client connected successfully");
                        Some(Arc::new(client))
                    }
                    Err(e) => {
                        tracing::warn!("Failed to connect to TTS service: {} (TTS features will be unavailable)", e);
                        None
                    }
                }
            },
            |tts_client| cosmic::Action::App(Message::TtsClientInitialized(tts_client)),
        );
        
        // Auto-connect if server config is valid (has host and api_key)
        if !app.server_config.host.is_empty() && !app.server_config.api_key.is_empty() {
            tracing::info!("🔌 Will auto-connect after TTS initialization...");
            app.pending_auto_connect = true;
        } else {
            tracing::info!("⚙️ No valid server config, showing settings");
        }
        
        // Always initialize TTS client - it will complete asynchronously
        (app, tts_init_task)
    }

    fn update(&mut self, message: Self::Message) -> app::Task<Self::Message> {
        // Try connection handlers first (WebSocket, server events)
        if let Some(task) = handle_connection_messages(self, message.clone()) {
            return task;
        }
        
        // Try chat handlers
        if let Some(task) = handle_chat_messages(self, message.clone()) {
            return task;
        }
        
        // Try navigation handlers
        if let Some(task) = handle_navigation_messages(self, message.clone()) {
            return task;
        }

        // History & memories handlers
        if let Some(task) = handle_history_memories_messages(self, message.clone()) {
            return task;
        }
        
        // Try settings handlers
        if let Some(task) = handle_settings_messages(self, message.clone()) {
            return task;
        }
        
        // Try server event handlers (ServerEvent variants)
        if let Some(task) = handle_server_event_messages(self, message.clone()) {
            return task;
        }
        
        // Handle TTS client initialization
        if let Message::TtsClientInitialized(client) = message.clone() {
            self.tts_client = client;
            // If we were supposed to auto-connect, do it now
            if self.pending_auto_connect && self.connection_status == ConnectionStatus::Disconnected {
                self.pending_auto_connect = false;
                tracing::info!("TTS initialization complete, now auto-connecting to server...");
                return app::Task::done(cosmic::Action::App(Message::Connect));
            }
            return app::Task::none();
        }
        
        // On first update, if we need to auto-connect and TTS init hasn't completed yet,
        // trigger connect anyway (TTS will initialize in background)
        // This handles the case where TTS init is slow or fails
        static INIT_CONNECT: std::sync::Once = std::sync::Once::new();
        if self.connection_status == ConnectionStatus::Disconnected
            && !self.server_config.host.is_empty()
            && !self.server_config.api_key.is_empty()
        {
            INIT_CONNECT.call_once(|| {
                // This will only run once, but we need to send Connect message
                // We'll handle this by checking in update() if we need to connect
            });
        }
        
        // Try TTS handlers
        if let Some(task) = handle_tts_messages(self, message.clone()) {
            return task;
        }
        
        // Handle MCP Servers messages
        match message.clone() {
            Message::LoadMCPServers => {
                if self.file_client.is_none() && self.connection_status == ConnectionStatus::Connected
                {
                    let rest_base = self
                        .rest_base
                        .clone()
                        .unwrap_or_else(|| {
                            self.server_config
                                .http_rest_base_uris()
                                .into_iter()
                                .next()
                                .unwrap_or_else(|| self.server_config.http_uri())
                        });
                    self.file_client = Some(FileClient::with_rest_base(
                        self.server_config.clone(),
                        rest_base,
                    ));
                }
                
                if let Some(ref file_client) = self.file_client {
                    let client = file_client.clone();
                    return app::Task::perform(
                        async move {
                            match client.list_mcp_servers().await {
                                Ok(response) => Message::MCPServersLoaded(response.servers),
                                Err(e) => Message::MCPServersLoadError(e.to_string()),
                            }
                        },
                        cosmic::Action::App,
                    );
                } else {
                    return app::Task::perform(
                        async move {
                            Message::MCPServersLoadError("Not connected to server".to_string())
                        },
                        cosmic::Action::App,
                    );
                }
            }
            Message::MCPServersLoaded(servers) => {
                self.mcp_servers = servers;
                return app::Task::none();
            }
            Message::MCPServersLoadError(error) => {
                self.inline_error = Some(format!("Failed to load MCP servers: {}", error));
                return app::Task::none();
            }
            Message::ToggleMCPServerExpand(name) => {
                if self.mcp_expanded_servers.contains(&name) {
                    self.mcp_expanded_servers.remove(&name);
                } else {
                    self.mcp_expanded_servers.insert(name);
                }
                return app::Task::none();
            }
            _ => {}
        }

        // Handle remaining simple messages
        match message {
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
            Message::ShowAbout => {
                self.show_about = !self.show_about;
            }
            Message::CloseAbout => {
                self.show_about = false;
            }
            Message::OpenUrl(url) => {
                let _ = webbrowser::open(&url);
            }
            Message::Quit => {
                std::process::exit(0);
            }
            // Image cache updates
            Message::ImageLoaded { url, data, is_svg } => {
                let state = if is_svg {
                    ImageState::Svg(data)
                } else {
                    let handle = cosmic::widget::image::Handle::from_bytes(data.clone());
                    ImageState::Raster { handle, data }
                };
                self.image_cache.insert(url, state);
            }
            Message::ImageLoadFailed { url, error } => {
                tracing::warn!("Failed to load markdown image {url}: {error}");
                self.image_cache.insert(url, ImageState::Error(error));
            }
            Message::ParseMarkdownChunk { start } => {
                // Parse markdown for a small slice of messages, then yield to
                // the iced runtime by chaining the next chunk via Task::done.
                // This keeps each update tick short on huge histories.
                const CHUNK_SIZE: usize = 15;
                let end = (start + CHUNK_SIZE).min(self.messages.len());
                if start < end {
                    for msg in &mut self.messages[start..end] {
                        if matches!(
                            msg.bubble_type,
                            BubbleType::Assistant | BubbleType::Summary
                        ) && msg.markdown_items.is_empty()
                            && !msg.content.trim().is_empty()
                        {
                            msg.markdown_items =
                                ChatMessage::parse_markdown_items(&msg.content);
                        }
                    }
                }

                let chunk_items: Vec<_> = self.messages[start..end]
                    .iter()
                    .flat_map(|m| m.markdown_items.iter().cloned())
                    .collect();
                let image_task = self.fetch_missing_images(&chunk_items);

                if end < self.messages.len() {
                    return app::Task::batch([
                        app::Task::done(cosmic::Action::App(
                            Message::ParseMarkdownChunk { start: end },
                        )),
                        image_task,
                    ]);
                }
                return image_task;
            }
            _ => {} // Messages handled by handler modules
        }

        app::Task::none()
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        let mut subscriptions = vec![];

        // TTS status subscription - TODO: Implement proper stream subscription
        // For now, status is updated via TtsStatusChanged messages from TTS operations

        // Typing indicator tick only while the animated dots are on screen.
        if self.needs_typing_indicator_tick() {
            subscriptions.push(
                cosmic::iced::time::every(std::time::Duration::from_millis(TYPING_INDICATOR_INTERVAL_MS))
                    .map(Message::Tick),
            );
        }

        // WebSocket event subscription - subscribes to broadcast channel (supports reconnection)
        if self.connection_status == ConnectionStatus::Connected {
            subscriptions.push(Subscription::run_with(
                WsSubscriptionClient(self.ws_client.clone()),
                ws_client_event_stream,
            ));
        }

        Subscription::batch(subscriptions)
    }

    fn view(&self) -> Element<'_, Self::Message> {
        match self.current_page {
            Page::Chat => chat_page(self),
            Page::History => history_page(self),
            Page::Memories => memories_page(self),
            Page::MCPServers => mcp_servers_page(self),
            Page::Settings => settings_page(self),
        }
    }

    fn header_start(&self) -> Vec<Element<'_, Self::Message>> {
        vec![crate::ui::widgets::menu_bar::create_menu_bar(&self.key_binds)]
    }

    fn nav_model(&self) -> Option<&widget::segmented_button::SingleSelectModel> {
        Some(&self.nav_model)
    }

    fn context_drawer(
        &self,
    ) -> Option<app::context_drawer::ContextDrawer<'_, Self::Message>> {
        if self.show_about {
            Some(
                app::context_drawer::about(
                    &self.about,
                    |url| Message::OpenUrl(url.to_string()),
                    Message::CloseAbout,
                )
                .title("About")
            )
        } else {
            None
        }
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
                    // Load MCP servers when navigating to that page
                    if page == Page::MCPServers && self.connection_status == ConnectionStatus::Connected {
                        return app::Task::perform(
                            async { Message::LoadMCPServers },
                            cosmic::Action::App,
                        );
                    }
                    if page == Page::Memories && self.connection_status == ConnectionStatus::Connected {
                        return app::Task::perform(
                            async { Message::LoadMemories },
                            cosmic::Action::App,
                        );
                    }
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

