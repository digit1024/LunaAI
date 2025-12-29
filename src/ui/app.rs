use cosmic::{
    app::{self, Core},
    dialog::file_chooser::{self},
    iced::Subscription,
    widget::{self, menu, text_editor},
    Application, Element,
};
use std::sync::{Arc, Mutex};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::{
    agentic::protocol::AgentUpdate,
    config::AppConfig,
    llm::{self, LlmClient},
    mcp::MCPServerRegistry,
    prompts::PromptManager,
    storage::Storage,
    ui::context::ContextPage,
    ui::dialogs::{DialogAction, DialogPage},
    ui::pages::chat,
    ui::pages::history,
    ui::pages::mcp_config,
    ui::pages::settings::{self, SimpleSettingsMessage, SimpleSettingsPage},
    ui::pages::tools,
    ui::state::{AttachmentState, ContextState, ConversationState, ToolCallState},
    ui::widgets::ToolCallMessage,
};
use crate::services::MCPService;
use crate::ui::handlers::{
    handle_chat_messages, handle_tool_messages, handle_navigation_messages, 
    handle_agent_messages, handle_settings_messages, handle_dbus_messages,
    handle_dialog_messages, handle_mcp_messages,
};
use serde_json::Value;

#[derive(Debug, Clone)]
pub enum Message {
    InputChanged(String),
    InputActionPerformed(text_editor::Action),
    SendMessage,
    StopMessage,
    RetryMessage,
    AttachFile,
    FileSelected(String), // file path
    RemoveFile(String),   // file path
    FileChooserCancelled,
    FileChooserError(Arc<file_chooser::Error>),
    NavigateTo(NavigationPage),
    SelectConversation(Uuid),
    DeleteConversation(Uuid),
    NewConversation,
    AgentUpdate(AgentUpdate),
    ToolCallStarted(String, String),   // tool_name, parameters
    ToolCallCompleted(String, String), // tool_name, result
    ToolCallError(String, String),     // tool_name, error
    ToolCallWidgetMessage(usize, ToolCallMessage), // index, message
    ToggleToolSummary(usize, String),  // message idx, summary id
    ToggleReasoning(usize),            // message idx
    ToggleSummary(usize),              // message idx for summary messages
    ScrollToBottom,
    // Menu actions
    ShowAbout,
    OpenSettings,
    Quit,
    CloseAbout,
    OpenUrl(String),
    // Settings actions
    ChangeDefaultProfile(usize),
    SaveSettings,
    ResetSettings,
    // Settings page messages (delegated to page module)
    SettingsPage(SimpleSettingsMessage),
    // Page messages (delegated to page modules)
    ChatPage(chat::Message),
    HistoryPage(history::Message),
    MCPConfigPage(mcp_config::Message),
    // Dialog actions
    DialogAction(DialogAction),
    ShowMessageDialog(String),
    // MCP actions
    MCPToolsUpdated(Vec<crate::llm::ToolDefinition>),
    RefreshMCPTools,
    ToggleMCPServer(String), // server_name
    OpenMCPConfig, // Open MCP config file in cosmic-edit
    // Settings actions
    OpenConfigFile, // Open main config file in cosmic-edit
    OpenProfilePrompt(String), // Open prompt file for profile in cosmic-edit
    // Tool toggle actions
    ToggleAllTools(bool),     // true = enable all, false = disable all
    ToggleTool(String, bool), // tool_name, enabled
    ToggleMCPServerEnabled(String, bool), // server_name, enabled - toggles all tools for a server
    ShowToolsContext,
    HideToolsContext,
    // Markdown link handling
    MarkdownLinkClicked(widget::markdown::Url),
    // Search functionality
    SearchChanged(String),
    SearchResults(Vec<crate::storage::sqlite_storage_simple::Snippet>),
    // Inline error handling
    InlineError(String),
    DismissError,
    // Typing indicator animation tick
    TypingIndicatorTick(cosmic::iced::time::Instant),
    // Refresh conversation list for nav bar
    RefreshConversationList,
    // Manual summarization
    ManualSummarize,
    // D-Bus TTS/STT messages
    DbusServiceAvailable(bool), // Service availability changed
    CheckDbusService,            // Check service availability
    PlayMessageTts(usize),       // Play TTS for message at index
    StopMessageTts,              // Stop TTS playback
    StartStt,                    // Start STT recording
    StopStt,                     // Stop STT recording
    SttResult(String),           // STT transcription result
    DbusStatusChanged(String),    // Status changed signal from D-Bus service (idle/speaking/listening/processing)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavigationPage {
    Chat,
    History,
    MCPConfig,
    Settings,
}

// NavItem represents either a navigation page or a conversation in the nav bar
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NavItem {
    Page(NavigationPage),
    Conversation(Uuid),
}

// ContextPage moved to ui::context module for better organization

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MenuAction {
    About,
    NewConversation,
    Settings,
    Quit,
    SendMessage,
    SummarizeConversation,
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
            MenuAction::SummarizeConversation => Message::ManualSummarize,
        }
    }
}

// NavMenuAction for navigation context menu (pattern from msToDO)
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NavMenuAction {
    NewConversation,
    Settings,
    About,
    Quit,
}

impl menu::Action for NavMenuAction {
    type Message = cosmic::Action<Message>;

    fn message(&self) -> Self::Message {
        cosmic::Action::App(match self {
            NavMenuAction::NewConversation => Message::NewConversation,
            NavMenuAction::Settings => Message::OpenSettings,
            NavMenuAction::About => Message::ShowAbout,
            NavMenuAction::Quit => Message::Quit,
        })
    }
}

pub struct CosmicLlmApp {
    pub core: Core,
    pub config: AppConfig,
    pub storage: Storage,
    pub prompt_manager: PromptManager,
    pub current_page: NavigationPage,
    pub mcp_registry: Arc<RwLock<MCPServerRegistry>>,
    pub llm_client: Arc<dyn LlmClient>,
    pub is_streaming: bool,
    pub current_streaming_id: Option<Uuid>,
    pub key_binds: std::collections::HashMap<menu::KeyBind, MenuAction>,
    pub settings_changed: bool,
    #[allow(dead_code)] // Reserved for future use
    pub title_sender: Option<tokio::sync::mpsc::UnboundedSender<(Uuid, String)>>,
    pub settings_page: SimpleSettingsPage,
    pub context_page: ContextPage,
    pub about: widget::about::About,
    // Navigation model to integrate with COSMIC shell nav bar (pattern from msToDO)
    pub nav_model: widget::segmented_button::SingleSelectModel,
    // When true, ignore legacy StreamingUpdate to avoid duplicate UI events
    #[allow(dead_code)] // Reserved for future use
    pub agent_mode_active: bool,
    // Dialog state
    pub dialog: Option<DialogPage>,
    pub dialog_text_content: Option<text_editor::Content>,
    // MCP tools cache
    pub available_mcp_tools: Vec<crate::llm::ToolDefinition>,
    // Tool enable/disable state (tool_name -> enabled)
    pub tool_states: std::collections::HashMap<String, bool>,
    // D-Bus TTS/STT service
    #[cfg(feature = "ttsandstt")]
    pub dbus_ttsstt_available: bool,
    #[cfg(feature = "ttsandstt")]
    pub dbus_ttsstt_client: Arc<crate::dbus::DbusTtsSttClient>,
    #[cfg(feature = "ttsandstt")]
    pub dbus_ttsstt_status: Arc<RwLock<String>>, // Current status: idle/speaking/listening/processing (shared for signal updates)
    #[cfg(feature = "ttsandstt")]
    pub dbus_ttsstt_status_display: String, // Current status for UI display (updated via messages)
    #[cfg(feature = "ttsandstt")]
    pub stt_listening_initiated: bool, // True if we initiated the listening (to distinguish from other apps)
    #[cfg(feature = "ttsandstt")]
    pub playing_message_id: Option<usize>, // Index of message currently playing TTS
    
    // State modules (extracted from god object)
    pub conversation_state: ConversationState,
    pub tool_call_state: ToolCallState,
    pub attachment_state: AttachmentState,
    pub context_state: ContextState,
    
    // Page modules (extracted from god object)
    pub chat_page: chat::Page,
    pub history_page: history::Page,
    pub mcp_config_page: mcp_config::Page,
}

#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub content: String,
    pub is_user: bool,
    pub is_error: bool,
    pub reasoning_content: Option<String>, // For DeepSeek thinking/reasoning content
    pub is_summary: bool, // True if this message is a summary of previous messages
    #[allow(dead_code)] // Field used for serialization/storage
    pub is_summarized: bool, // True if this message has been summarized (should be excluded from LLM payload)
    pub summarized_count: Option<usize>, // Count of messages summarized
}

#[derive(Debug, Clone)]
pub struct ToolCallInfo {
    pub id: Option<String>,
    pub tool_name: String,
    pub parameters: String,
    pub status: ToolCallStatus,
    pub result: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ToolCallStatus {
    Started,
    Completed,
    Error,
}

#[derive(Debug, Clone)]
pub struct AnchoredToolCall {
    pub anchor_index: usize,
    pub tool_call: ToolCallInfo,
}

#[derive(Debug, Clone)]
pub struct ToolRuntimeContext {
    pub anchor_index: usize,
    pub params: Option<Value>,
}

impl CosmicLlmApp {
    pub fn new(
        core: Core,
        config: AppConfig,
        storage: Storage,
        prompt_manager: PromptManager,
        mcp_registry: Arc<RwLock<MCPServerRegistry>>,
        llm_client: Arc<dyn LlmClient>,
    ) -> Self {
        // Create title sender channel
        let (title_sender, _title_receiver) =
            tokio::sync::mpsc::unbounded_channel::<(Uuid, String)>();

        // Note: Title updates will be handled synchronously in the main thread
        // since Storage is not cloneable for async tasks

        let about = widget::about::About::default()
            .name("Cosmic LLM")
            .icon(cosmic::widget::icon::Named::new(Self::APP_ID))
            .version("0.1.0")
            .license("GPL-3.0")
            .links([
                ("Repository", "https://github.com/digit1024/cosmic_llm"),
                ("Issues", "https://github.com/digit1024/cosmic_llm/issues"),
                ("Documentation", "https://github.com/digit1024/cosmic_llm#readme"),
                ("Discussions", "https://github.com/digit1024/cosmic_llm/discussions"),
            ])
            .developers([
                ("Michał Banaś", "https://github.com/digit1024")
            ])
            .comments("A COSMIC desktop application for AI chat with MCP tool integration. Built with Rust and libcosmic.");

        // Initialize icon cache
        if let Err(e) = crate::ui::icons::ICON_CACHE
            .set(Mutex::new(crate::ui::icons::IconCache::new()))
        {
            tracing::warn!(error = ?e, "Failed to initialize icon cache (may be already initialized)");
        }

        let mut settings_page = SimpleSettingsPage::new();
        settings_page.load_from_config(&config);
        
        Self {
            core,
            config: config.clone(),
            storage,
            prompt_manager,
            current_page: NavigationPage::Chat,
            mcp_registry,
            llm_client,
            is_streaming: false,
            current_streaming_id: None,
            key_binds: Self::create_key_binds(),
            settings_changed: false,
            title_sender: Some(title_sender),
            settings_page,
            context_page: ContextPage::About,
            about,
            nav_model: {
                // Build initial nav model - will be updated after loading conversations
                let mut model = widget::segmented_button::ModelBuilder::default().build();
                model
                    .insert()
                    .text("New Chat")
                    .icon(crate::ui::icons::get_icon("chat-symbolic", 16))
                    .data(NavItem::Page(NavigationPage::Chat));
                model
                    .insert()
                    .text("More history")
                    .icon(crate::ui::icons::get_icon("list-large-symbolic", 16))
                    .data(NavItem::Page(NavigationPage::History))
                    .divider_above(true);
                model
                    .insert()
                    .text("MCP Config")
                    .icon(crate::ui::icons::get_icon("configure-symbolic", 16))
                    .data(NavItem::Page(NavigationPage::MCPConfig));
                model
                    .insert()
                    .text("Settings")
                    .icon(crate::ui::icons::get_icon("settings-symbolic", 16))
                    .data(NavItem::Page(NavigationPage::Settings))
                    .divider_above(true);
                // Activate first item - collect entity first to avoid borrow issues
                let first_entity = model.iter().next();
                if let Some(first) = first_entity {
                    model.activate(first);
                }
                model
            },
            agent_mode_active: true,
            dialog: None,
            dialog_text_content: None,
            available_mcp_tools: Vec::new(),
            tool_states: std::collections::HashMap::new(),
            #[cfg(feature = "ttsandstt")]
            dbus_ttsstt_available: false,
            #[cfg(feature = "ttsandstt")]
            dbus_ttsstt_client: Arc::new(crate::dbus::DbusTtsSttClient::new()),
            #[cfg(feature = "ttsandstt")]
            dbus_ttsstt_status: Arc::new(RwLock::new(String::new())), // Current status: idle/speaking/listening/processing
            #[cfg(feature = "ttsandstt")]
            dbus_ttsstt_status_display: String::new(), // Current status for UI display
            #[cfg(feature = "ttsandstt")]
            stt_listening_initiated: false, // True if we initiated the listening
            #[cfg(feature = "ttsandstt")]
            playing_message_id: None, // Index of message currently playing TTS
            
            // Initialize state modules
            conversation_state: ConversationState::new(),
            tool_call_state: ToolCallState::new(),
            attachment_state: AttachmentState::new(),
            context_state: ContextState::new(),
            
            // Initialize page modules
            chat_page: chat::Page::new(),
            history_page: history::Page::new(),
            mcp_config_page: mcp_config::Page::new(),
        }
    }

    fn create_key_binds() -> std::collections::HashMap<menu::KeyBind, MenuAction> {
        use cosmic::iced::keyboard::Key;
        use cosmic::widget::menu::key_bind::{KeyBind, Modifier};

        let mut key_binds = std::collections::HashMap::new();

        // File menu shortcuts
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

        // View menu shortcuts
        key_binds.insert(
            KeyBind {
                modifiers: vec![Modifier::Ctrl],
                key: Key::Character(",".into()),
            },
            MenuAction::Settings,
        );

        // Send message shortcut
        key_binds.insert(
            KeyBind {
                modifiers: vec![Modifier::Ctrl],
                key: Key::Named(cosmic::iced::keyboard::key::Named::Enter),
            },
            MenuAction::SendMessage,
        );

        key_binds
    }

    pub(crate) fn create_streaming_subscription(&self, streaming_id: Option<Uuid>) -> Subscription<Message> {
        crate::ui::subscriptions::streaming::create_streaming_subscription(
            streaming_id,
            self.llm_client.clone(),
            self.prompt_manager.clone(),
            self.conversation_state.messages.clone(),
            self.mcp_registry.clone(),
            &self.attachment_state,
            &self.config,
        )
    }

    /// Rebuild conversation view (delegates to ConversationState)
    pub(crate) fn rebuild_conversation_view(
        &mut self,
        conversation: crate::storage::conversation_storage::Conversation,
    ) {
        self.conversation_state.rebuild_conversation_view(
            conversation,
            &mut self.tool_call_state,
            &mut self.context_state,
            &self.storage,
            &self.config,
            &self.prompt_manager,
        );
    }

    /// Update context usage cache (delegates to ConversationState)
    pub(crate) fn update_context_usage_cache(&mut self, conversation_id: Uuid) {
        self.conversation_state.update_context_usage_cache(
            conversation_id,
            &self.storage,
            &self.config,
            &self.prompt_manager,
        );
    }

    /// Perform manual summarization on the current conversation (delegates to ContextService)
    fn perform_manual_summarization(&mut self, conv_id: Uuid) {
        if let Some(profile) = self.config.get_default_profile() {
            let llm_client = self.llm_client.clone();
            let storage = &self.storage;
            let profile_clone = profile.clone();
            
            // Use tokio runtime for async summarization (blocking but necessary for desktop)
            let summary_result = tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(async {
                    crate::services::ContextService::perform_manual_summarization(
                        conv_id,
                        storage,
                        &llm_client,
                        &profile_clone,
                    ).await
                })
            });
            
            match summary_result {
                Ok(_summary_content) => {
                    // Rebuild UI messages from DB to show the summary
                    if let Ok(Some(conv)) = self.storage.get_conversation(&conv_id) {
                        self.rebuild_conversation_view(conv);
                        // Update context usage cache
                        self.update_context_usage_cache(conv_id);
                    }
                }
                Err(e) => {
                    tracing::error!(
                        conversation_id = %conv_id,
                        error = %e,
                        "Summarization failed"
                    );
                }
            }
        } else {
            tracing::warn!("No profile configured for summarization");
        }
    }

    // Helper methods moved to src/ui/helpers/utils.rs
}

impl Application for CosmicLlmApp {
    type Executor = cosmic::executor::Default;
    type Flags = ();
    type Message = Message;
    const APP_ID: &'static str = "com.github.digit1024.cosmic_llm";

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(core: Core, _flags: Self::Flags) -> (Self, app::Task<Self::Message>) {
        // Initialize config and storage
        let config = AppConfig::load().unwrap_or_else(|_| AppConfig::default());
        if let Ok(cwd) = std::env::current_dir() {
            tracing::debug!(cwd = %cwd.display(), "Config load current directory");
        }
        tracing::debug!(default_profile = %config.default, "Loaded default profile key");
        if let Some(p) = config.get_default_profile() {
            let masked = if p.api_key.len() > 6 {
                format!(
                    "{}...{}",
                    &p.api_key[..3],
                    &p.api_key[p.api_key.len().saturating_sub(3)..]
                )
            } else {
                "***".to_string()
            };
            tracing::debug!(
                model = %p.model,
                endpoint = %p.endpoint,
                api_key_masked = %masked,
                "Default profile details"
            );
        } else {
            tracing::warn!("No default profile found; using fallback defaults");
        }
        let initial_profile_mcp_servers = config
            .get_default_profile()
            .map(|profile| profile.enabled_mcp.clone())
            .unwrap_or_default();
        // Initialize storage with fallback handling
        let storage = match crate::ui::init_helpers::initialize_storage(&config) {
            Ok(storage) => storage,
            Err(e) => {
                tracing::error!(error = %e, "Failed to initialize storage, exiting");
                std::process::exit(1);
            }
        };

        // Initialize prompt manager with fallback handling
        let prompt_manager = crate::ui::init_helpers::initialize_prompt_manager(&config);

        // Initialize MCP registry (non-blocking)
        let mcp_registry = Arc::new(RwLock::new(MCPServerRegistry::new()));
        let mcp_registry_clone = mcp_registry.clone();

        // Try to load MCP config from JSON file (new Claude Desktop format)
        // Falls back to embedded TOML format if JSON doesn't exist
        let mcp_config = crate::config::MCPConfig::load_from_json().unwrap_or_else(|e| {
            tracing::debug!(error = %e, "No mcp_config.json found, falling back to embedded TOML config");
            config.mcp.clone()
        });

        tracing::debug!(server_count = mcp_config.servers.len(), "MCP Servers configured");
        for (name, _) in &mcp_config.servers {
            tracing::debug!(server_name = %name, "MCP server configured");
        }

        // Initialize MCP registry in background
        crate::ui::init_helpers::initialize_mcp_registry(
            mcp_registry_clone,
            mcp_config,
            initial_profile_mcp_servers,
        );

        // Initialize LLM client based on default profile's backend
        let llm_client = crate::ui::init_helpers::initialize_llm_client(&config);

        let mut app = Self::new(
            core,
            config,
            storage,
            prompt_manager,
            mcp_registry,
            llm_client,
        );

        // Check for conversations with "Generating title..." and retry title generation
        crate::ui::init_helpers::retry_title_generation(&mut app);


        // Load recent conversations and update nav model
        app.load_recent_conversations();
        app.update_nav_model();

        // Create startup tasks
        let tasks = crate::ui::init_helpers::create_startup_tasks(&app);

        (app, app::Task::batch(tasks))
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        crate::ui::subscriptions::app::create_app_subscriptions(self)
    }

    fn update(&mut self, message: Self::Message) -> app::Task<Self::Message> {
        // Try chat handlers first
        if let Some(task) = handle_chat_messages(self, message.clone()) {
            return task;
        }
        
        // Try tool handlers
        if let Some(task) = handle_tool_messages(self, message.clone()) {
            return task;
        }
        
        // Try navigation handlers
        if let Some(task) = handle_navigation_messages(self, message.clone()) {
            return task;
        }
        
        // Try agent handlers
        if let Some(task) = handle_agent_messages(self, message.clone()) {
            return task;
        }
        
        // Try settings handlers
        if let Some(task) = handle_settings_messages(self, message.clone()) {
            return task;
        }
        
        // Try D-Bus handlers
        if let Some(task) = handle_dbus_messages(self, &message) {
            return task;
        }
        
        // Try dialog handlers
        if let Some(task) = handle_dialog_messages(self, &message) {
            return task;
        }
        
        // Try MCP handlers
        if let Some(task) = handle_mcp_messages(self, &message) {
            return task;
        }
        
        // Handle remaining messages (messages not handled by handler modules)
        match message {
            Message::ToggleReasoning(message_idx) => {
                if self.context_state.expanded_reasoning.contains(&message_idx) {
                    self.context_state.expanded_reasoning.remove(&message_idx);
                } else {
                    self.context_state.expanded_reasoning.insert(message_idx);
                }
            }
            Message::ToggleSummary(message_idx) => {
                if self.context_state.expanded_summaries.contains(&message_idx) {
                    self.context_state.expanded_summaries.remove(&message_idx);
                } else {
                    self.context_state.expanded_summaries.insert(message_idx);
                }
            }
            Message::ToggleMCPServer(server_name) => {
                self.context_state.toggle_mcp_server(server_name);
            }
            Message::ShowAbout => {
                // Toggle behavior: if About is already shown, hide it; otherwise show it
                // Pattern from msToDO for consistent UX
                if self.context_page == ContextPage::About && self.core.window.show_context {
                    self.core.window.show_context = false; // Toggle off
                } else {
                    self.context_page = ContextPage::About;
                    self.core.window.show_context = true; // Show
                }
            }
            Message::CloseAbout => {
                self.core.window.show_context = false;
            }
            Message::OpenUrl(url) => {
                let _ = webbrowser::open(&url);
            }
            Message::ManualSummarize => {
                if let Some(conv_id) = self.conversation_state.current_conversation_id {
                    self.perform_manual_summarization(conv_id);
                } else {
                    tracing::warn!("No active conversation to summarize");
                }
            }
            // All chat, navigation, agent, tool, settings, and D-Bus messages are handled by handler modules above
            // Handlers return None for unhandled messages, which fall through to this match
            Message::Quit => {
                // TODO: Implement proper quit
                std::process::exit(0);
            }
            // Settings messages are handled by settings handler module
            Message::MarkdownLinkClicked(url) => {
                let _ = webbrowser::open(url.as_str());
            }
            Message::ChatPage(_) => {
                // Chat page messages will be handled in future iteration
                // For now, these are still handled as direct messages
            }
            Message::MCPConfigPage(msg) => {
                match msg {
                    mcp_config::Message::ToggleServer(server_name) => {
                        self.mcp_config_page.toggle_server(server_name);
                    }
                }
            }
            Message::HistoryPage(msg) => {
                // Handle navigation messages first (before update consumes msg)
                let nav_action = match &msg {
                    history::Message::SelectConversation(id) => {
                        Some(Message::SelectConversation(*id))
                    }
                    history::Message::DeleteConversation(id) => {
                        Some(Message::DeleteConversation(*id))
                    }
                    _ => None,
                };
                
                // Delegate to history page update
                let _task = self.history_page.update(msg, &self.storage);
                
                // Handle navigation if needed
                if let Some(action) = nav_action {
                    return self.update(action);
                }
            }
            // Legacy messages (to be migrated to page messages)
            Message::SearchChanged(query) => {
                return self.update(Message::HistoryPage(history::Message::SearchChanged(query)));
            }
            Message::SearchResults(results) => {
                return self.update(Message::HistoryPage(history::Message::SearchResults(results)));
            }
            Message::RefreshConversationList => {
                self.load_recent_conversations();
                self.update_nav_model();
            }
            _ => {} // Messages handled by handler modules or page modules
        }

        app::Task::none()
    }

    fn view(&self) -> Element<Self::Message> {
        // Main layout with side panel and content area
        cosmic::widget::row::with_capacity(1).push(
            // Main content area
            match self.current_page {
                NavigationPage::Chat => chat::chat_view(self),
                NavigationPage::History => history::history_view(self),
                NavigationPage::MCPConfig => mcp_config::mcp_config_view(self),
                NavigationPage::Settings => settings::settings_view(self),
            },
        )
        .into()
    }

    fn dialog(&self) -> Option<Element<Self::Message>> {
        let dialog_page = self.dialog.as_ref()?;
        // Content should always be set when MessageText dialog is open
        let content = self.dialog_text_content.as_ref()?;
        Some(dialog_page.view(content).into())
    }

    fn header_start(&self) -> Vec<Element<Self::Message>> {
        vec![self.create_menu_bar()]
    }

    fn nav_model(&self) -> Option<&widget::segmented_button::SingleSelectModel> {
        Some(&self.nav_model)
    }

    fn on_nav_select(
        &mut self,
        entity: widget::segmented_button::Entity,
    ) -> app::Task<Self::Message> {
        if let Some(nav_item) = self.nav_model.data::<NavItem>(entity) {
            match nav_item {
                NavItem::Page(NavigationPage::Chat) => {
                    // "New Chat" clicked - trigger NewConversation if we have an active conversation
                    if self.conversation_state.current_conversation_id.is_some() {
                        return app::Task::perform(
                            async move { cosmic::Action::App(Message::NewConversation) },
                            |action| action,
                        );
                    }
                    // Already in new chat, just navigate to Chat page
                    self.current_page = NavigationPage::Chat;
                }
                NavItem::Page(page) => {
                    self.current_page = *page;
                }
                NavItem::Conversation(conv_id) => {
                    // Select the conversation
                    let conv_id = *conv_id;
                    return app::Task::perform(
                        async move { cosmic::Action::App(Message::SelectConversation(conv_id)) },
                        |action| action,
                    );
                }
            }
        }
        app::Task::none()
    }

    fn nav_context_menu(
        &self,
        _id: widget::nav_bar::Id,
    ) -> Option<Vec<widget::menu::Tree<cosmic::Action<Self::Message>>>> {
        // Context menu for navigation entries (pattern similar to msToDO)
        Some(cosmic::widget::menu::items(
            &std::collections::HashMap::new(),
            vec![
                cosmic::widget::menu::Item::Button(
                    "New Conversation",
                    None,
                    NavMenuAction::NewConversation,
                ),
                cosmic::widget::menu::Item::Button("Settings", None, NavMenuAction::Settings),
                cosmic::widget::menu::Item::Button("About", None, NavMenuAction::About),
                cosmic::widget::menu::Item::Button("Quit", None, NavMenuAction::Quit),
            ],
        ))
    }

    fn context_drawer(
        &self,
    ) -> Option<app::context_drawer::ContextDrawer<<Self as Application>::Message>> {
        if !self.core.window.show_context {
            return None;
        }

        if self.context_state.show_tools_context {
            Some(
                app::context_drawer::context_drawer(
                    tools::tools_context_view(self),
                    Message::HideToolsContext,
                )
                .title("Tool Configuration"),
            )
        } else {
            Some(match self.context_page {
                ContextPage::About => app::context_drawer::about(
                    &self.about,
                    |url| Message::OpenUrl(url.to_string()),
                    Message::CloseAbout,
                )
                .title(self.context_page.title()), // Dynamic title from ContextPage (pattern from msToDO)
            })
        }
    }
}

impl CosmicLlmApp {
    /// Update the nav model with current conversation title and recent conversations
    pub(crate) fn update_nav_model(&mut self) {
        crate::ui::helpers::navigation::update_nav_model(
            &mut self.nav_model,
            &self.conversation_state,
            &self.storage,
        );
    }
    
    /// Load recent conversations from storage (last 11 to accommodate active conversation)
    pub(crate) fn load_recent_conversations(&mut self) {
        crate::ui::helpers::navigation::load_recent_conversations(
            &mut self.conversation_state,
            &self.storage,
        );
    }
    
    // Helper methods moved to src/ui/helpers/profile.rs and src/ui/widgets/menu_bar.rs
    pub(crate) fn error_banner(&self) -> Option<Element<Message>> {
        self.chat_page.current_error.as_ref()
            .map(|error| crate::ui::widgets::error_banner::error_banner(error))
    }

    pub(crate) fn profile_tool_defaults_task(&self) -> Option<app::Task<Message>> {
        crate::ui::helpers::profile::profile_tool_defaults_task(self)
    }

    fn create_menu_bar(&self) -> Element<Message> {
        crate::ui::widgets::menu_bar::create_menu_bar(&self.key_binds)
    }
}
