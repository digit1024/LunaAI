use cosmic::{
    app::{self, Core},
    dialog::file_chooser::{self, FileFilter},
    iced::Subscription,
    widget::{self, menu, text_editor},
    Application, Element,
};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::{
    agentic::protocol::AgentUpdate,
    config::{AppConfig, LlmProfile},
    ui::pages::settings::simple_settings::{EditingProfileState, ProfileField},
    llm::{self, LlmClient, ToolCall},
    mcp::MCPServerRegistry,
    prompts::{ProfilePromptError, PromptManager},
    storage::{
        sqlite_storage_simple::{MessageMetadata, SqliteSettings},
        Storage,
    },
    ui::context::ContextPage,
    ui::dialogs::{DialogAction, DialogPage},
    ui::pages::chat,
    ui::pages::history,
    ui::pages::mcp_config,
    ui::pages::settings::{SimpleSettingsMessage, SimpleSettingsPage},
    ui::pages::tools,
    ui::state::{AttachmentState, ContextState, ConversationState, ToolCallState},
    ui::widgets::ToolCallMessage,
};
use crate::services::{ContextService, MessageConverter, MCPService};
use crate::ui::handlers::{handle_chat_messages, handle_tool_messages, handle_navigation_messages, handle_agent_messages, handle_settings_messages};
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
    // New Settings page messages
    SettingsMessage(SimpleSettingsMessage),
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
    pub title_sender: Option<tokio::sync::mpsc::UnboundedSender<(Uuid, String)>>,
    pub settings_page: SimpleSettingsPage,
    pub context_page: ContextPage,
    pub about: widget::about::About,
    // Navigation model to integrate with COSMIC shell nav bar (pattern from msToDO)
    pub nav_model: widget::segmented_button::SingleSelectModel,
    // When true, ignore legacy StreamingUpdate to avoid duplicate UI events
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

    fn create_streaming_subscription(&self, streaming_id: Option<Uuid>) -> Subscription<Message> {
        use cosmic::iced_futures::futures::SinkExt;
        use cosmic::iced_futures::stream;
        use tokio::sync::mpsc;

        // Create a streaming subscription using the channel pattern
        let id = streaming_id.unwrap_or_else(|| uuid::Uuid::new_v4());
        let llm_client = self.llm_client.clone();
        let prompt_manager = self.prompt_manager.clone();
        let messages = self.conversation_state.messages.clone();
        let mcp_registry = self.mcp_registry.clone();
        let pending_messages = self.attachment_state.pending_llm_messages.clone();
        let profile = self.config.get_default_profile().cloned();

        Subscription::run_with_id(
            id,
            stream::channel(100, move |mut output| async move {
                // Use prepared messages if available (which includes attachments), otherwise rebuild
                let llm_messages = if let Some(prepared_messages) = pending_messages {
                    tracing::debug!("Using prepared messages with attachments");
                    prepared_messages
                } else {
                    tracing::debug!("Rebuilding messages from history");
                    // Build LLM messages from conversation history (without prompts - ContextService will add them)
                    let mut llm_messages = Vec::new();

                    // Add conversation history, filtering out placeholder assistant messages
                    for msg in &messages {
                        let content_trimmed = msg.content.trim();
                        if !msg.is_user {
                            // Skip placeholder or empty assistant messages
                            if content_trimmed.is_empty() || content_trimmed == "🤔 Thinking..." {
                                continue;
                            }
                        }

                        let role = if msg.is_user {
                            crate::llm::Role::User
                        } else {
                            crate::llm::Role::Assistant
                        };
                        llm_messages.push(crate::llm::Message::new(role, msg.content.clone()));
                    }

                    llm_messages
                };

                // === CONTEXT MANAGEMENT ===
                // Use ContextService to prepare context (inject prompts, apply truncation)
                let final_messages = if let Some(ref prof) = profile {
                    let context_service = ContextService;
                    // Clone llm_messages for fallback in error case
                    let llm_messages_fallback = llm_messages.clone();
                    match context_service.prepare_context(llm_messages, prof, &prompt_manager) {
                        Ok(prepared) => {
                            // Check if truncation occurred and notify user if needed
                            use crate::llm::tokenizer::TokenCounter;
                            let token_counter = TokenCounter::new(prof);
                            let safe_limit = token_counter.get_safe_context_limit(prof);
                            let final_tokens: usize = prepared.iter()
                                .map(|msg| token_counter.count_message_tokens(msg))
                                .sum();
                            
                            if final_tokens > safe_limit * 9 / 10 {
                                // Close to limit, warn user
                                let _ = output.send(Message::InlineError(format!(
                                    "Context size ({} tokens) is close to limit ({}). Some messages may have been truncated.",
                                    final_tokens, safe_limit
                                ))).await;
                            }
                            
                            prepared
                        }
                        Err(e) => {
                            tracing::error!(error = %e, "Failed to prepare context");
                            // Try to handle profile prompt errors gracefully
                            if let Some(err_msg) = e.to_string().as_str().strip_prefix("Profile prompt error: ") {
                                let _ = output.send(Message::InlineError(format!(
                                    "Profile prompt error: {}",
                                    err_msg
                                ))).await;
                            } else {
                                let _ = output.send(Message::InlineError(format!(
                                    "Failed to prepare context: {}",
                                    e
                                ))).await;
                            }
                            llm_messages_fallback // Fallback to original messages
                        }
                    }
                } else {
                    llm_messages
                };

                // Create channel for agent updates
                let (tx_agent, mut rx_agent) = mpsc::unbounded_channel::<AgentUpdate>();

                // Start agentic processing in background
                let llm_client_clone = llm_client.clone();
                let mcp_registry_clone = mcp_registry.clone();
                let llm_messages_clone = final_messages.clone();

                tokio::spawn(async move {
                    let mut agentic_loop = crate::agentic::loop_engine::AgenticLoop::new(
                        mcp_registry_clone,
                        llm_client_clone,
                    );

                    match agentic_loop
                        .process_message(llm_messages_clone, Some(tx_agent.clone()), Some(id))
                        .await
                    {
                        Ok(_final_response) => {
                            // Final response is sent via AgentUpdate::EndConversation
                        }
                        Err(e) => {
                            // Send error via AgentUpdate - this handles cases where the loop fails completely
                            let _ = tx_agent.send(AgentUpdate::ModelError {
                                error: format!("Agent processing failed: {}", e),
                            });
                        }
                    }
                });

                // Process AgentUpdate stream
                while let Some(update) = rx_agent.recv().await {
                    let _ = output.send(Message::AgentUpdate(update)).await;
                }
            }),
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

    pub(crate) fn format_json_string(raw: &str) -> String {
        match serde_json::from_str::<Value>(raw) {
            Ok(value) => serde_json::to_string_pretty(&value).unwrap_or_else(|_| raw.to_string()),
            Err(_) => raw.to_string(),
        }
    }

    pub(crate) fn coerce_value(raw: &str) -> Value {
        serde_json::from_str::<Value>(raw).unwrap_or(Value::String(raw.to_string()))
    }
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
        let sqlite_settings = SqliteSettings::from(&config.server);
        let storage =
            Storage::new_default_with_settings(sqlite_settings.clone()).unwrap_or_else(|e| {
                tracing::error!(error = %e, "Failed to initialize SQLite storage");
                // Fallback to a temporary database
                Storage::new_with_settings(
                    std::env::temp_dir().join("cosmic_llm_temp.db"),
                    sqlite_settings,
                )
                .unwrap_or_else(|e| {
                    tracing::error!(error = %e, "Failed to create temporary database");
                    std::process::exit(1);
                })
            });

        // Initialize prompt manager
        let prompt_manager = crate::prompts::PromptManager::load_from_config(&config.prompts)
            .unwrap_or_else(|e| {
                tracing::error!(error = %e, "Failed to load prompts");
                crate::prompts::PromptManager::load_from_config(
                    &crate::prompts::PromptConfig::default(),
                )
                .unwrap_or_else(|e| {
                    tracing::error!(error = %e, "Failed to load default prompt config, using empty PromptManager");
                    crate::prompts::PromptManager {
                        system_prompt: None,
                    }
                })
            });

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
        let llm_client: Arc<dyn LlmClient> = {
            let profile = config
                .get_default_profile()
                .unwrap_or(&crate::config::LlmProfile::default())
                .clone();
            llm::build_llm_client(&profile)
        };

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

        // Load MCP tools on startup (same as refresh button)
        let load_tools_task = cosmic::Task::perform(
            async move {
                // Wait for MCP servers to initialize (give them more time)
                tokio::time::sleep(tokio::time::Duration::from_millis(5000)).await;
                tracing::debug!("Startup: Attempting to refresh MCP tools");
                cosmic::Action::App(Message::RefreshMCPTools)
            },
            |msg| msg,
        );

        let mut tasks = vec![load_tools_task];
        
        // Check D-Bus TTS/STT service availability at startup (only if feature enabled)
        #[cfg(feature = "ttsandstt")]
        {
            let dbus_client = app.dbus_ttsstt_client.clone();
            let dbus_check_task = cosmic::Task::perform(
                async move {
                    tracing::debug!("Checking D-Bus TTS/STT service availability");
                    let available = dbus_client.check_availability().await;
                    tracing::debug!(available, "D-Bus service check result");
                    cosmic::Action::App(Message::DbusServiceAvailable(available))
                },
                |msg| msg,
            );
            tasks.push(dbus_check_task);
        }
        if let Some(task) = app.profile_tool_defaults_task() {
            tasks.push(task);
        }

        (app, app::Task::batch(tasks))
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        use cosmic::iced::time;
        use cosmic::iced_futures::Subscription;
        
        // Create a subscription for streaming LLM responses
        let streaming_sub = if self.is_streaming {
            self.create_streaming_subscription(self.current_streaming_id)
        } else {
            Subscription::none()
        };
        
        // Create a timer subscription for typing indicator animation
        let animation_sub = if self.is_streaming {
            time::every(time::Duration::from_millis(50))
                .map(|instant| Message::TypingIndicatorTick(instant))
        } else {
            Subscription::none()
        };
        
        // Create a periodic subscription to refresh conversation list every 15 seconds
        let conversation_refresh_sub = time::every(time::Duration::from_secs(15))
            .map(|_| Message::RefreshConversationList);
        
        // Periodically check D-Bus service availability (every 5 seconds) - only if feature enabled
        #[cfg(feature = "ttsandstt")]
        let dbus_check_sub = time::every(time::Duration::from_secs(5))
            .map(|_| Message::CheckDbusService);
        #[cfg(not(feature = "ttsandstt"))]
        let dbus_check_sub = Subscription::none();
        
        // D-Bus status signal subscription
        #[cfg(feature = "ttsandstt")]
        let dbus_status_sub = if self.dbus_ttsstt_available {
            crate::ui::subscriptions::dbus::create_dbus_status_subscription(
                self.dbus_ttsstt_client.clone()
            )
        } else {
            Subscription::none()
        };
        #[cfg(not(feature = "ttsandstt"))]
        let dbus_status_sub = Subscription::none();
        
        Subscription::batch(vec![streaming_sub, animation_sub, conversation_refresh_sub, dbus_check_sub, dbus_status_sub])
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
        
        // Check if any handler handled the message (they all return None for handled messages)
        // If none returned a task, check if it's a handled message and return Task::none()
        if matches!(&message,
            Message::InputChanged(_) | Message::InputActionPerformed(_) |
            Message::SendMessage | Message::StopMessage | Message::RetryMessage |
            Message::AttachFile | Message::FileSelected(_) | Message::RemoveFile(_) |
            Message::FileChooserCancelled | Message::FileChooserError(_) |
            Message::ScrollToBottom | Message::InlineError(_) | Message::DismissError |
            Message::TypingIndicatorTick(_) |
            Message::ToolCallStarted(_, _) | Message::ToolCallCompleted(_, _) |
            Message::ToolCallError(_, _) | Message::ToolCallWidgetMessage(_, _) |
            Message::ToggleToolSummary(_, _) |
            Message::NavigateTo(_) | Message::SelectConversation(_) |
            Message::DeleteConversation(_) | Message::NewConversation |
            Message::AgentUpdate(_) |
            Message::OpenSettings | Message::OpenConfigFile | Message::OpenProfilePrompt(_) |
            Message::OpenMCPConfig
        ) {
            return app::Task::none();
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
            // All chat, navigation, agent, tool, and settings messages are handled by handler modules above
            // These match arms are intentionally left empty - handlers return None for handled messages
            #[cfg(feature = "ttsandstt")]
            Message::DbusServiceAvailable(available) => {
                let was_available = self.dbus_ttsstt_available;
                self.dbus_ttsstt_available = available;
                if available && !was_available {
                    tracing::info!("D-Bus TTS/STT service is now available");
                    // Signal subscription will automatically start listening when available
                } else if !available && was_available {
                    tracing::warn!("D-Bus TTS/STT service is no longer available");
                    let mut guard = self.dbus_ttsstt_status.blocking_write();
                    guard.clear();
                    self.stt_listening_initiated = false;
                }
            }
            #[cfg(feature = "ttsandstt")]
            Message::DbusStatusChanged(status) => {
                // Update the display status (this triggers UI re-render)
                let old_status = self.dbus_ttsstt_status_display.clone();
                if old_status != status {
                    tracing::debug!(
                        old_status = %old_status,
                        new_status = %status,
                        "D-Bus status changed, buttons will update"
                    );
                    self.dbus_ttsstt_status_display = status.clone();
                    
                    // Reset listening initiated flag when status goes to idle
                    if status == "idle" {
                        self.stt_listening_initiated = false;
                        // Also clear playing message ID when TTS stops
                        self.playing_message_id = None;
                    }
                    
                    // Update shared status for signal updates
                    {
                        let mut guard = self.dbus_ttsstt_status.blocking_write();
                        *guard = status;
                    }
                }
            }
            #[cfg(feature = "ttsandstt")]
            Message::CheckDbusService => {
                let client = self.dbus_ttsstt_client.clone();
                return cosmic::Task::perform(
                    async move {
                        let available = client.check_availability().await;
                        cosmic::Action::App(Message::DbusServiceAvailable(available))
                    },
                    |msg| msg,
                );
            }
            #[cfg(feature = "ttsandstt")]
            Message::PlayMessageTts(message_idx) => {
                // Get message content
                if let Some(msg) = self.conversation_state.messages.get(message_idx) {
                    // Mark this message as playing
                    self.playing_message_id = Some(message_idx);
                    
                    // Strip markdown from content for TTS (simple regex-based approach)
                    let text = msg.content
                        .lines()
                        .filter_map(|line| {
                            let trimmed = line.trim();
                            // Skip markdown headers, code blocks, links, etc.
                            if trimmed.starts_with('#') || trimmed.starts_with("```") || trimmed.starts_with('[') {
                                None
                            } else {
                                Some(trimmed)
                            }
                        })
                        .collect::<Vec<_>>()
                        .join(" ");
                    
                    let client = self.dbus_ttsstt_client.clone();
                    return cosmic::Task::perform(
                        async move {
                            if let Err(e) = client.call_tts(&text, "en-US").await {
                                tracing::error!(error = %e, "Failed to call TTS");
                            }
                            cosmic::Action::App(Message::DismissError)
                        },
                        |msg| msg,
                    );
                }
            }
            #[cfg(feature = "ttsandstt")]
            Message::StopMessageTts => {
                // Clear playing message ID
                self.playing_message_id = None;
                let client = self.dbus_ttsstt_client.clone();
                return cosmic::Task::perform(
                    async move {
                        if let Err(e) = client.stop().await {
                            tracing::error!(error = %e, "Failed to stop TTS");
                        }
                        cosmic::Action::App(Message::DismissError)
                    },
                    |msg| msg,
                );
            }
            #[cfg(feature = "ttsandstt")]
            Message::StartStt => {
                // Mark that we initiated listening
                self.stt_listening_initiated = true;
                let client = self.dbus_ttsstt_client.clone();
                return cosmic::Task::perform(
                    async move {
                        match client.call_stt("en-US", 2.0).await {
                            Ok(text) => cosmic::Action::App(Message::SttResult(text)),
                            Err(e) => {
                                tracing::error!(error = %e, "Failed to call STT");
                                cosmic::Action::App(Message::DismissError)
                            }
                        }
                    },
                    |msg| msg,
                );
            }
            #[cfg(feature = "ttsandstt")]
            Message::StopStt => {
                // Reset listening initiated flag
                self.stt_listening_initiated = false;
                let client = self.dbus_ttsstt_client.clone();
                return cosmic::Task::perform(
                    async move {
                        if let Err(e) = client.stop().await {
                            tracing::error!(error = %e, "Failed to stop STT");
                        }
                        cosmic::Action::App(Message::DismissError)
                    },
                    |msg| msg,
                );
            }
            #[cfg(feature = "ttsandstt")]
            Message::SttResult(text) => {
                // Reset listening initiated flag when we get the result
                self.stt_listening_initiated = false;
                // Insert STT result into input field
                // Use perform_action to insert text
                self.chat_page.input_content.perform(text_editor::Action::Edit(
                    text_editor::Edit::Paste(Arc::new(text.clone())),
                ));
                self.chat_page.input = self.chat_page.input_content.text();
            }
            #[cfg(not(feature = "ttsandstt"))]
            Message::DbusServiceAvailable(_) | 
            Message::DbusStatusChanged(_) | 
            Message::CheckDbusService | 
            Message::PlayMessageTts(_) | 
            Message::StopTts | 
            Message::StartStt | 
            Message::StopStt | 
            Message::SttResult(_) => {
                // No-op when feature is disabled
            }
            Message::Quit => {
                // TODO: Implement proper quit
                std::process::exit(0);
            }
            Message::ChangeDefaultProfile(profile_index) => {
                // Must sort the same way as in the view to maintain index consistency
                // Filter out hidden profiles
                let mut profile_names: Vec<String> = self.config.profiles
                    .iter()
                    .filter(|(_, p)| !p.hidden)
                    .map(|(name, _)| name.clone())
                    .collect();
                profile_names.sort();
                if let Some(profile_name) = profile_names.get(profile_index) {
                    let new_profile = profile_name.clone();
                    self.config.default = new_profile.clone();
                    self.settings_changed = true;
                    // Recreate LLM client for new default provider
                    if let Some(profile) = self.config.get_default_profile().cloned() {
                        let masked = if profile.api_key.len() > 6 {
                            format!(
                                "{}...{}",
                                &profile.api_key[..3],
                                &profile.api_key[profile.api_key.len().saturating_sub(3)..]
                            )
                        } else {
                            "***".to_string()
                        };
                        tracing::debug!(
                            profile_name = %self.config.default,
                            model = %profile.model,
                            endpoint = %profile.endpoint,
                            api_key_masked = %masked,
                            "Switching default profile"
                        );
                        self.llm_client = llm::build_llm_client(&profile);
                        
                        // Update active conversation's profile in database if there is one
                        if let Some(conv_id) = self.conversation_state.current_conversation_id {
                            if let Err(e) = self.storage.update_conversation_profile(&conv_id, Some(&new_profile)) {
                                tracing::error!(error = %e, "Failed to update conversation profile");
                            } else {
                                tracing::debug!(
                                    conversation_id = %conv_id,
                                    profile_name = %new_profile,
                                    "Updated conversation profile"
                                );
                            }
                        }
                    }
                    if let Some(task) = self.profile_tool_defaults_task() {
                        return task;
                    }
                }
            }
            Message::SaveSettings => {
                if let Err(e) = self.config.save() {
                    tracing::error!(error = %e, "Failed to save settings");
                } else {
                    self.settings_changed = false;
                    tracing::debug!("Settings saved successfully");
                }
            }
            Message::ResetSettings => {
                self.config = AppConfig::default();
                self.settings_changed = true;
            }
            Message::SettingsMessage(settings_msg) => {
                match settings_msg {
                    SimpleSettingsMessage::BackToMain => {
                        self.current_page = NavigationPage::Chat;
                    }
                    SimpleSettingsMessage::SetDefaultProfile(name) => {
                        if self.settings_page.staged_profiles.contains_key(&name) {
                            self.settings_page.staged_default = name;
                            self.settings_page.has_changes = true;
                        }
                    }
                    SimpleSettingsMessage::NewProfileNameChanged(val) => {
                        self.settings_page.new_profile_name = val;
                    }
                    SimpleSettingsMessage::NewProfileModelChanged(val) => {
                        self.settings_page.new_profile_model = val;
                    }
                    SimpleSettingsMessage::NewProfileEndpointChanged(val) => {
                        self.settings_page.new_profile_endpoint = val;
                    }
                    SimpleSettingsMessage::NewProfileApiKeyChanged(val) => {
                        self.settings_page.new_profile_api_key = val;
                    }
                    SimpleSettingsMessage::NewProfileBackendChanged(val) => {
                        self.settings_page.new_profile_backend = val;
                    }
                    SimpleSettingsMessage::AddNewProfile => {
                        let name = self.settings_page.new_profile_name.trim().to_string();
                        let model = self.settings_page.new_profile_model.trim().to_string();
                        let endpoint = self.settings_page.new_profile_endpoint.trim().to_string();
                        let api_key = self.settings_page.new_profile_api_key.trim().to_string();
                        let backend = self.settings_page.new_profile_backend.trim().to_string();
                        if !name.is_empty() && !model.is_empty() {
                            let mut profile = LlmProfile::default();
                            profile.backend = if backend.is_empty() { "openai".to_string() } else { backend };
                            profile.model = model;
                            profile.endpoint = endpoint;
                            profile.api_key = api_key;
                            profile.temperature = Some(0.7);
                            profile.max_tokens = Some(1000);
                            self.settings_page.staged_profiles.insert(name.clone(), profile);
                            if self.settings_page.staged_default.is_empty() {
                                self.settings_page.staged_default = name.clone();
                            }
                            self.settings_page.has_changes = true;
                            // Clear inputs
                            self.settings_page.new_profile_name.clear();
                            self.settings_page.new_profile_model.clear();
                            self.settings_page.new_profile_endpoint.clear();
                            self.settings_page.new_profile_api_key.clear();
                            self.settings_page.new_profile_backend = "openai".to_string();
                        }
                    }
                    SimpleSettingsMessage::ToggleProfile(profile_name) => {
                        if self.settings_page.expanded_profiles.contains(&profile_name) {
                            self.settings_page.expanded_profiles.remove(&profile_name);
                        } else {
                            self.settings_page.expanded_profiles.insert(profile_name);
                        }
                    }
                    SimpleSettingsMessage::StartEditProfile(profile_name) => {
                        if let Some(profile) = self.settings_page.staged_profiles.get(&profile_name).cloned() {
                            self.settings_page.editing_profiles.insert(
                                profile_name.clone(),
                                EditingProfileState {
                                    name: profile_name.clone(),
                                    backend: profile.backend,
                                    model: profile.model,
                                    endpoint: profile.endpoint,
                                    api_key: profile.api_key,
                                    temperature: profile.temperature,
                                    temperature_str: profile.temperature.map(|t| t.to_string()).unwrap_or_default(),
                                    max_tokens: profile.max_tokens,
                                    max_tokens_str: profile.max_tokens.map(|t| t.to_string()).unwrap_or_default(),
                                    context_window_size: profile.context_window_size,
                                    context_window_size_str: profile.context_window_size.map(|s| s.to_string()).unwrap_or_default(),
                                    summarize_threshold: profile.summarize_threshold,
                                    summarize_threshold_str: profile.summarize_threshold.to_string(),
                                    profile_prompt_file: profile.profile_prompt_file.clone(),
                                    profile_prompt_file_str: profile.profile_prompt_file.as_deref().unwrap_or("").to_string(),
                                    enabled_mcp: profile.enabled_mcp.clone(),
                                    enabled_mcp_str: profile.enabled_mcp.join(", "),
                                    hidden: profile.hidden,
                                },
                            );
                        }
                    }
                    SimpleSettingsMessage::CancelEditProfile(profile_name) => {
                        self.settings_page.editing_profiles.remove(&profile_name);
                    }
                    SimpleSettingsMessage::SaveProfile(profile_name) => {
                        if let Some(edit_state) = self.settings_page.editing_profiles.get(&profile_name) {
                            if let Some(profile) = self.settings_page.staged_profiles.get_mut(&profile_name) {
                                profile.backend = edit_state.backend.clone();
                                profile.model = edit_state.model.clone();
                                profile.endpoint = edit_state.endpoint.clone();
                                profile.api_key = edit_state.api_key.clone();
                                profile.temperature = edit_state.temperature;
                                profile.max_tokens = edit_state.max_tokens;
                                profile.context_window_size = edit_state.context_window_size;
                                profile.summarize_threshold = edit_state.summarize_threshold;
                                profile.profile_prompt_file = edit_state.profile_prompt_file.clone();
                                profile.enabled_mcp = edit_state.enabled_mcp.clone();
                                profile.hidden = edit_state.hidden;
                                self.settings_page.has_changes = true;
                            }
                            self.settings_page.editing_profiles.remove(&profile_name);
                        }
                    }
                    SimpleSettingsMessage::DeleteProfile(profile_name) => {
                        self.settings_page.staged_profiles.remove(&profile_name);
                        self.settings_page.expanded_profiles.remove(&profile_name);
                        self.settings_page.editing_profiles.remove(&profile_name);
                        if self.settings_page.staged_default == profile_name && !self.settings_page.staged_profiles.is_empty() {
                            self.settings_page.staged_default = self.settings_page.staged_profiles.keys().next()
                                .expect("Checked that staged_profiles is not empty")
                                .clone();
                        }
                        self.settings_page.has_changes = true;
                    }
                    SimpleSettingsMessage::UpdateProfileField(profile_name, field, value) => {
                        if let Some(edit_state) = self.settings_page.editing_profiles.get_mut(&profile_name) {
                            match field {
                                ProfileField::Name => edit_state.name = value,
                                ProfileField::Backend => edit_state.backend = value,
                                ProfileField::Model => edit_state.model = value,
                                ProfileField::Endpoint => edit_state.endpoint = value,
                                ProfileField::ApiKey => edit_state.api_key = value,
                            }
                        }
                    }
                    SimpleSettingsMessage::UpdateProfileTemperature(profile_name, temp) => {
                        if let Some(edit_state) = self.settings_page.editing_profiles.get_mut(&profile_name) {
                            edit_state.temperature = temp;
                            edit_state.temperature_str = temp.map(|t| t.to_string()).unwrap_or_default();
                        }
                    }
                    SimpleSettingsMessage::UpdateProfileMaxTokens(profile_name, tokens) => {
                        if let Some(edit_state) = self.settings_page.editing_profiles.get_mut(&profile_name) {
                            edit_state.max_tokens = tokens;
                            edit_state.max_tokens_str = tokens.map(|t| t.to_string()).unwrap_or_default();
                        }
                    }
                    SimpleSettingsMessage::UpdateProfileContextWindowSize(profile_name, size) => {
                        if let Some(edit_state) = self.settings_page.editing_profiles.get_mut(&profile_name) {
                            edit_state.context_window_size = size;
                            edit_state.context_window_size_str = size.map(|s| s.to_string()).unwrap_or_default();
                        }
                    }
                    SimpleSettingsMessage::UpdateProfileSummarizeThreshold(profile_name, threshold) => {
                        if let Some(edit_state) = self.settings_page.editing_profiles.get_mut(&profile_name) {
                            edit_state.summarize_threshold = threshold;
                            edit_state.summarize_threshold_str = threshold.to_string();
                        }
                    }
                    SimpleSettingsMessage::UpdateProfilePromptFile(profile_name, prompt_file) => {
                        if let Some(edit_state) = self.settings_page.editing_profiles.get_mut(&profile_name) {
                            edit_state.profile_prompt_file = if prompt_file.trim().is_empty() {
                                None
                            } else {
                                Some(prompt_file.clone())
                            };
                            edit_state.profile_prompt_file_str = prompt_file;
                        }
                    }
                    SimpleSettingsMessage::UpdateProfileEnabledMCP(profile_name, enabled_mcp_str) => {
                        if let Some(edit_state) = self.settings_page.editing_profiles.get_mut(&profile_name) {
                            edit_state.enabled_mcp_str = enabled_mcp_str.clone();
                            // Parse the comma-separated string into Vec, but keep the raw string for editing
                            edit_state.enabled_mcp = enabled_mcp_str
                                .split(',')
                                .map(|s| s.trim().to_string())
                                .filter(|s| !s.is_empty())
                                .collect();
                        }
                    }
                    SimpleSettingsMessage::UpdateProfileHidden(profile_name, hidden) => {
                        if let Some(edit_state) = self.settings_page.editing_profiles.get_mut(&profile_name) {
                            edit_state.hidden = hidden;
                        }
                    }
                    SimpleSettingsMessage::UpdateServerHost(val) => {
                        self.settings_page.staged_server.host = val.clone();
                        self.settings_page.server_host = val;
                        self.settings_page.has_changes = true;
                    }
                    SimpleSettingsMessage::UpdateServerPort(val) => {
                        if let Ok(port) = val.parse::<u16>() {
                            self.settings_page.staged_server.port = port;
                            self.settings_page.server_port = port;
                            self.settings_page.server_port_str = val.clone();
                            self.settings_page.has_changes = true;
                        } else {
                            self.settings_page.server_port_str = val;
                        }
                    }
                    SimpleSettingsMessage::UpdateServerApiKey(val) => {
                        self.settings_page.staged_server.api_key = val.clone();
                        self.settings_page.server_api_key = val;
                        self.settings_page.has_changes = true;
                    }
                    SimpleSettingsMessage::UpdateStreamTimeout(val) => {
                        if let Ok(timeout) = val.parse::<u64>() {
                            self.settings_page.staged_server.stream_timeout_secs = timeout;
                            self.settings_page.stream_timeout_secs = timeout;
                            self.settings_page.stream_timeout_str = val.clone();
                            self.settings_page.has_changes = true;
                        } else {
                            self.settings_page.stream_timeout_str = val;
                        }
                    }
                    SimpleSettingsMessage::UpdateTitleGenProfile(val) => {
                        self.settings_page.staged_title_summary.title_generation_profile = if val.is_empty() {
                            None
                        } else {
                            Some(val.clone())
                        };
                        self.settings_page.title_generation_profile = val;
                        self.settings_page.has_changes = true;
                    }
                    SimpleSettingsMessage::UpdateSummaryChars(val) => {
                        if let Ok(chars) = val.parse::<u32>() {
                            self.settings_page.staged_title_summary.summary_chars = chars;
                            self.settings_page.summary_chars = chars;
                            self.settings_page.summary_chars_str = val.clone();
                            self.settings_page.has_changes = true;
                        } else {
                            self.settings_page.summary_chars_str = val;
                        }
                    }
                    SimpleSettingsMessage::UpdateSummaryLoopSleep(val) => {
                        if let Ok(sleep) = val.parse::<u64>() {
                            self.settings_page.staged_title_summary.summary_loop_sleep_seconds = sleep;
                            self.settings_page.summary_loop_sleep_seconds = sleep;
                            self.settings_page.summary_loop_str = val.clone();
                            self.settings_page.has_changes = true;
                        } else {
                            self.settings_page.summary_loop_str = val;
                        }
                    }
                    SimpleSettingsMessage::UpdateTitleGenPrompt(val) => {
                        self.settings_page.staged_title_summary.title_generation_system_prompt = val.clone();
                        self.settings_page.title_generation_system_prompt = val;
                        self.settings_page.has_changes = true;
                    }
                    SimpleSettingsMessage::OpenConfigFile => {
                        return cosmic::Task::perform(
                            async {},
                            |_| cosmic::Action::App(Message::OpenConfigFile),
                        );
                    }
                    SimpleSettingsMessage::OpenProfilePrompt(profile_name) => {
                        let profile_name_for_task = profile_name.clone();
                        return cosmic::Task::perform(
                            async { profile_name_for_task },
                            |profile_name_for_task| cosmic::Action::App(Message::OpenProfilePrompt(profile_name_for_task)),
                        );
                    }
                    SimpleSettingsMessage::SaveConfig => {
                        // Apply all staged changes to actual config
                        self.config.profiles = self.settings_page.staged_profiles.clone();
                        self.config.default = self.settings_page.staged_default.clone();
                        self.config.server = self.settings_page.staged_server.clone();
                        self.config.title_summary = self.settings_page.staged_title_summary.clone();
                        
                        // Save to file
                        if let Err(e) = self.config.save() {
                            tracing::error!(error = %e, "Failed to save settings");
                            self.dialog = Some(DialogPage::message_text(format!(
                                "Failed to save settings:\n{}",
                                e
                            )));
                        } else {
                            self.settings_page.has_changes = false;
                            self.settings_changed = false;
                            // Update LLM client if default profile changed
                            let profile_changed = self.config.default.clone();
                            if let Some(profile) = self.config.get_default_profile().cloned() {
                                self.llm_client = llm::build_llm_client(&profile);
                            }
                            // Update active conversation's profile in database if there is one
                            if let Some(conv_id) = self.conversation_state.current_conversation_id {
                                if let Err(e) = self.storage.update_conversation_profile(&conv_id, Some(&profile_changed)) {
                                    tracing::error!(error = %e, "Failed to update conversation profile");
                                } else {
                                    tracing::debug!(
                                        conversation_id = %conv_id,
                                        profile_name = %profile_changed,
                                        "Updated conversation profile"
                                    );
                                }
                            }
                            if let Some(task) = self.profile_tool_defaults_task() {
                                return task;
                            }
                        }
                    }
                    SimpleSettingsMessage::CancelConfig => {
                        // Reload from config to discard all staged changes
                        self.settings_page.load_from_config(&self.config);
                        self.current_page = NavigationPage::Chat;
                    }
                }
            }
            Message::DialogAction(action) => {
                match action {
                    DialogAction::Open(page) => {
                        // Initialize text content when opening MessageText dialog
                        match &page {
                            DialogPage::MessageText(text) => {
                                self.dialog_text_content = Some(text_editor::Content::with_text(text));
                            }
                        }
                        self.dialog = Some(page);
                    }
                    DialogAction::Update(page) => {
                        // Update text content when updating MessageText dialog
                        match &page {
                            DialogPage::MessageText(text) => {
                                self.dialog_text_content = Some(text_editor::Content::with_text(text));
                            }
                        }
                        self.dialog = Some(page);
                    }
                    DialogAction::Close => {
                        self.dialog = None;
                        self.dialog_text_content = None;
                    }
                    DialogAction::Complete => {
                        // For MessageText dialog, Complete just closes it
                        self.dialog = None;
                        self.dialog_text_content = None;
                    }
                    DialogAction::CopyText => {
                        // Copy the current dialog text to clipboard
                        if let Some(DialogPage::MessageText(text)) = &self.dialog {
                            let _ = cli_clipboard::set_contents(text.clone());
                        }
                        // Keep dialog open for multiple copies
                    }
                    DialogAction::TextEditorAction(action) => {
                        // Handle text editor actions to enable selection
                        if let Some(content) = &mut self.dialog_text_content {
                            content.perform(action);
                        }
                    }
                }
            }
            Message::ShowMessageDialog(content) => {
                let text = content.clone();
                self.dialog_text_content = Some(text_editor::Content::with_text(&text));
                self.dialog = Some(DialogPage::message_text(text));
            }
            Message::MCPToolsUpdated(tools) => {
                self.available_mcp_tools = tools;
                // Sync tool states from registry
                if let Ok(registry) = self.mcp_registry.try_read() {
                    self.tool_states = registry.get_tool_states();
                }
            }
            Message::RefreshMCPTools => {
                // Try to get tools synchronously from registry
                if let Ok(registry) = self.mcp_registry.try_read() {
                    let tools = registry.get_available_tools();
                    tracing::debug!(tool_count = tools.len(), "RefreshMCPTools: Found tools");
                    self.available_mcp_tools = tools;
                    // Also sync tool states
                    self.tool_states = registry.get_tool_states();
                } else {
                    tracing::error!("RefreshMCPTools: Failed to get registry read lock");
                }
            }
            Message::ToggleAllTools(enabled) => {
                // Update local state
                for tool in &self.available_mcp_tools {
                    self.tool_states.insert(tool.name.clone(), enabled);
                }
                // Update registry asynchronously
                let mcp_registry = self.mcp_registry.clone();
                return cosmic::Task::perform(
                    async move {
                        let mut registry = mcp_registry.write().await;
                        if enabled {
                            registry.enable_all_tools();
                        } else {
                            registry.disable_all_tools();
                        }
                        cosmic::Action::App(Message::RefreshMCPTools)
                    },
                    |msg| msg,
                );
            }
            Message::ToggleTool(tool_name, enabled) => {
                // Update local state
                self.tool_states.insert(tool_name.clone(), enabled);
                // Update registry asynchronously
                let mcp_registry = self.mcp_registry.clone();
                return cosmic::Task::perform(
                    async move {
                        let mut registry = mcp_registry.write().await;
                        registry.set_tool_enabled(&tool_name, enabled);
                        cosmic::Action::App(Message::RefreshMCPTools)
                    },
                    |msg| msg,
                );
            }
            Message::ToggleMCPServerEnabled(server_name, enabled) => {
                // Update profile's enabled_mcp list synchronously
                let profile_name = self.config.default.clone();
                if let Some(profile) = self.config.profiles.get_mut(&profile_name) {
                    if enabled {
                        // Add server to enabled list if not present
                        if !profile.enabled_mcp.iter().any(|s| s.eq_ignore_ascii_case(&server_name)) {
                            profile.enabled_mcp.push(server_name.clone());
                        }
                    } else {
                        // Remove server from enabled list
                        profile.enabled_mcp.retain(|s| !s.eq_ignore_ascii_case(&server_name));
                    }
                }
                
                // Update tool_states synchronously for immediate UI feedback
                if let Ok(registry) = self.mcp_registry.try_read() {
                    // Find all tools for this server and update their states
                    for tool in &self.available_mcp_tools {
                        if let Ok(tool_server) = registry.get_server_for_tool(&tool.name) {
                            if tool_server == &server_name {
                                self.tool_states.insert(tool.name.clone(), enabled);
                            }
                        }
                    }
                }
                
                // Update registry and save config asynchronously
                let mcp_registry = self.mcp_registry.clone();
                let server_name_clone = server_name.clone();
                let config = self.config.clone();
                
                return cosmic::Task::perform(
                    async move {
                        // Update registry
                        {
                            let mut registry = mcp_registry.write().await;
                            registry.set_server_enabled(&server_name_clone, enabled);
                        }
                        
                        // Save config
                        if let Err(e) = config.save() {
                            tracing::error!(error = %e, "Failed to save config");
                        }
                        
                        cosmic::Action::App(Message::RefreshMCPTools)
                    },
                    |msg| msg,
                );
            }
            Message::ShowToolsContext => {
                self.context_state.show_tools_context = true;
                self.core.window.show_context = true;
            }
            Message::HideToolsContext => {
                self.context_state.show_tools_context = false;
                self.core.window.show_context = false;
            }
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
            Message::InlineError(error) => {
                self.chat_page.current_error = Some(error);
            }
            Message::DismissError => {
                self.chat_page.current_error = None;
            }
            Message::TypingIndicatorTick(instant) => {
                if let Some(start_time) = self.chat_page.typing_indicator_start_time {
                    let elapsed = instant.duration_since(start_time);
                    // Update animation progress (cycles every 1.2 seconds)
                    self.chat_page.typing_indicator_progress = (elapsed.as_secs_f32() / 1.2) % 1.0;
                }
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
                NavigationPage::Settings => self
                    .settings_page
                    .view(&self.config)
                    .map(Message::SettingsMessage),
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
    
    pub(crate) fn error_banner(&self) -> Option<Element<Message>> {
        self.chat_page.current_error.as_ref()
            .map(|error| crate::ui::widgets::error_banner::error_banner(error))
    }

    fn load_active_profile_prompt(&mut self) -> Option<String> {
        crate::ui::helpers::profile::load_active_profile_prompt(
            &self.config,
            &self.prompt_manager,
            &mut self.chat_page,
        )
    }

    pub(crate) fn profile_tool_defaults_task(&self) -> Option<app::Task<Message>> {
        let profile = self.config.get_default_profile()?;
        // Always apply profile defaults, even if enabled_mcp is empty
        // (empty list means enable all tools)
        let allowed_servers = profile.enabled_mcp.clone();
        let registry = self.mcp_registry.clone();

        Some(MCPService::profile_tool_defaults_task(registry, allowed_servers))
    }

    fn create_menu_bar(&self) -> Element<Message> {
        crate::ui::widgets::menu_bar::create_menu_bar(&self.key_binds)
    }
}
