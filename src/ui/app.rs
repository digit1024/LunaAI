use cosmic::{
    app::{self, Core},
    iced::Subscription,
    widget::{self, menu, text_editor},
    Application, Element,
    dialog::file_chooser::{self, FileFilter},
};
use std::sync::{Arc, Mutex};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::{
    agentic::protocol::AgentUpdate,
    config::{AppConfig, LlmProfile},
    llm::{LlmClient, ToolCall},
    mcp::MCPServerRegistry,
    prompts::PromptManager,
    storage::{sqlite_storage_simple::MessageMetadata, Storage},
    ui::context::ContextPage,
    ui::dialogs::{DialogAction, DialogPage},
    ui::pages::chat,
    ui::pages::history,
    ui::pages::mcp_config,
    ui::pages::settings::{SimpleSettingsMessage, SimpleSettingsPage},
    ui::pages::tools,
    ui::widgets::ToolCallMessage,
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
    RemoveFile(String), // file path
    FileChooserCancelled,
    FileChooserError(Arc<file_chooser::Error>),
    NavigateTo(NavigationPage),
    SelectConversation(Uuid),
    DeleteConversation(Uuid),
    NewConversation,
    AgentUpdate(AgentUpdate),
    ToolCallStarted(String, String), // tool_name, parameters
    ToolCallCompleted(String, String), // tool_name, result
    ToolCallError(String, String), // tool_name, error
    ToolCallWidgetMessage(usize, ToolCallMessage), // index, message
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
    // Dialog actions
    DialogAction(DialogAction),
    ShowMessageDialog(String),
    // MCP actions
    MCPToolsUpdated(Vec<crate::llm::ToolDefinition>),
    RefreshMCPTools,
    // Tool toggle actions
    ToggleAllTools(bool), // true = enable all, false = disable all
    ToggleTool(String, bool), // tool_name, enabled
    ShowToolsContext,
    HideToolsContext,
    // Markdown link handling
    MarkdownLinkClicked(widget::markdown::Url),
    // Search functionality
    SearchChanged(String),
    SearchResults(Vec<crate::storage::sqlite_storage_simple::Snippet>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavigationPage {
    Chat,
    History,
    MCPConfig,
    Settings,
}

// ContextPage moved to ui::context module for better organization

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
    pub input: String,
    pub input_content: text_editor::Content,
    pub messages: Vec<ChatMessage>,
    pub input_id: cosmic::widget::Id,
    pub current_page: NavigationPage,
    pub current_conversation_id: Option<Uuid>,
    pub mcp_registry: Arc<RwLock<MCPServerRegistry>>,
    pub llm_client: Arc<dyn LlmClient>,
    pub is_streaming: bool,
    pub current_streaming_id: Option<Uuid>,
    pub active_tool_calls: Vec<ToolCallInfo>,
    // Anchors tool calls under the AI message that executed them
    pub current_ai_message_index: Option<usize>,
    pub archived_tool_calls: Vec<AnchoredToolCall>,
    pub expanded_tool_calls: std::collections::HashSet<usize>,
    pub scrollable_id: cosmic::widget::Id,
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
    pub dialog_text_input_id: widget::Id,
    // MCP tools cache
    pub available_mcp_tools: Vec<crate::llm::ToolDefinition>,
    // Tool enable/disable state (tool_name -> enabled)
    pub tool_states: std::collections::HashMap<String, bool>,
    // Pending tool calls for persistence
    pub pending_tool_calls_for_history: Vec<ToolCall>,
    pub tool_runtime_context: std::collections::HashMap<String, ToolRuntimeContext>,
    // Show tools context panel
    pub show_tools_context: bool,
    // Store last user message for retry functionality
    pub last_user_message: Option<String>,
    // Store attached files
    pub attached_files: Vec<String>,
    // Store current error message
    pub current_error: Option<String>,
    // Store prepared LLM messages with attachments for the current request
    pub pending_llm_messages: Option<Vec<crate::llm::Message>>,
    // Search functionality
    pub search_query: String,
    pub search_results: Vec<crate::storage::sqlite_storage_simple::Snippet>,
}

#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub content: String,
    pub is_user: bool,
    pub is_error: bool,
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
    pub fn new(core: Core, config: AppConfig, storage: Storage, prompt_manager: PromptManager, mcp_registry: Arc<RwLock<MCPServerRegistry>>, llm_client: Arc<dyn LlmClient>) -> Self {
        // Create title sender channel
        let (title_sender, _title_receiver) = tokio::sync::mpsc::unbounded_channel::<(Uuid, String)>();
        
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
        crate::ui::icons::ICON_CACHE.set(Mutex::new(crate::ui::icons::IconCache::new())).unwrap();

        Self {
            core,
            config: config.clone(),
            storage,
            prompt_manager,
            input: String::new(),
            input_content: text_editor::Content::new(),
            messages: Vec::new(),
            input_id: cosmic::widget::Id::unique(),
            current_page: NavigationPage::Chat,
            current_conversation_id: None,
            mcp_registry,
            llm_client,
            is_streaming: false,
            current_streaming_id: None,
            active_tool_calls: Vec::new(),
            current_ai_message_index: None,
            archived_tool_calls: Vec::new(),
            expanded_tool_calls: std::collections::HashSet::new(),
            scrollable_id: cosmic::widget::Id::unique(),
            key_binds: Self::create_key_binds(),
            settings_changed: false,
            title_sender: Some(title_sender),
            settings_page: SimpleSettingsPage::new(),
            context_page: ContextPage::About,
            about,
            nav_model: {
                // Build and populate a segmented nav model mirroring app sections
                let mut model = widget::segmented_button::ModelBuilder::default().build();
                model
                    .insert()
                    .text("Chat")
                    .icon(crate::ui::icons::get_icon("chat-symbolic", 16))
                    .data(NavigationPage::Chat);
                model
                    .insert()
                    .text("History")
                    .icon(crate::ui::icons::get_icon("list-large-symbolic", 16))
                    .data(NavigationPage::History)
                    .divider_above(true);
                model
                    .insert()
                    .text("MCP Config")
                    .icon(crate::ui::icons::get_icon("configure-symbolic", 16))
                    .data(NavigationPage::MCPConfig);
                model
                    .insert()
                    .text("Settings")
                    .icon(crate::ui::icons::get_icon("settings-symbolic", 16))
                    .data(NavigationPage::Settings)
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
            dialog_text_input_id: widget::Id::unique(),
            available_mcp_tools: Vec::new(),
            tool_states: std::collections::HashMap::new(),
            pending_tool_calls_for_history: Vec::new(),
            tool_runtime_context: std::collections::HashMap::new(),
            show_tools_context: false,
            last_user_message: None,
            attached_files: Vec::new(),
            current_error: None,
            pending_llm_messages: None,
            search_query: String::new(),
            search_results: Vec::new(),
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
        use cosmic::iced_futures::stream;
        use cosmic::iced_futures::futures::SinkExt;
        use tokio::sync::mpsc;
        
        // Create a streaming subscription using the channel pattern
        let id = streaming_id.unwrap_or_else(|| uuid::Uuid::new_v4());
        let llm_client = self.llm_client.clone();
        let prompt_manager = self.prompt_manager.clone();
        let messages = self.messages.clone();
        let mcp_registry = self.mcp_registry.clone();
        let pending_messages = self.pending_llm_messages.clone();
        
        Subscription::run_with_id(id, stream::channel(100, move |mut output| async move {
            // Use prepared messages if available (which includes attachments), otherwise rebuild
            let llm_messages = if let Some(prepared_messages) = pending_messages {
                println!("🔍 DEBUG: Using prepared messages with attachments");
                prepared_messages
            } else {
                println!("🔍 DEBUG: Rebuilding messages from history");
                // Build LLM messages with system prompt
                let mut llm_messages = Vec::new();
                
                // Add system prompt if available
                if let Some(system_prompt) = prompt_manager.get_system_prompt() {
                    llm_messages.push(crate::llm::Message::new(
                        crate::llm::Role::System,
                        system_prompt.to_string()
                    ));
                }
                
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
            
            // Create channel for agent updates
            let (tx_agent, mut rx_agent) = mpsc::unbounded_channel::<AgentUpdate>();
            
            // Start agentic processing in background
            let llm_client_clone = llm_client.clone();
            let mcp_registry_clone = mcp_registry.clone();
            let llm_messages_clone = llm_messages.clone();
            
            tokio::spawn(async move {
                let mut agentic_loop = crate::agentic::loop_engine::AgenticLoop::new(mcp_registry_clone, llm_client_clone);
                
                match agentic_loop.process_message(llm_messages_clone, Some(tx_agent.clone()), Some(id)).await {
                    Ok(_final_response) => {
                        // Final response is sent via AgentUpdate::EndConversation
                    }
                    Err(e) => {
                        // Send error via AgentUpdate - this handles cases where the loop fails completely
                        let _ = tx_agent.send(AgentUpdate::ModelError { 
                            error: format!("Agent processing failed: {}", e)
                        });
                    }
                }
            });
            
            // Process AgentUpdate stream
            while let Some(update) = rx_agent.recv().await {
                let _ = output.send(Message::AgentUpdate(update)).await;
            }
        }))
    }

    fn rebuild_conversation_view(
        &mut self,
        conversation: crate::storage::conversation_storage::Conversation,
    ) {
        self.messages.clear();
        self.archived_tool_calls.clear();
        self.active_tool_calls.clear();
        self.current_ai_message_index = None;
        self.pending_tool_calls_for_history.clear();
        self.tool_runtime_context.clear();

        let mut archived_indices: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();

        for stored in conversation.messages {
            let is_user = stored.role == "user";
            self.messages.push(ChatMessage {
                content: stored.content.clone(),
                is_user,
                is_error: false,
            });
            let anchor_index = self.messages.len().saturating_sub(1);

            if let Some(tool_calls) = stored.tool_calls {
                for call in tool_calls {
                    let params_pretty = serde_json::to_string_pretty(&call.parameters)
                        .unwrap_or_else(|_| call.parameters.to_string());
                    let info = ToolCallInfo {
                        id: Some(call.id.clone()),
                        tool_name: call.name.clone(),
                        parameters: params_pretty,
                        status: ToolCallStatus::Started,
                        result: None,
                        error: None,
                    };
                    self.archived_tool_calls
                        .push(AnchoredToolCall { anchor_index, tool_call: info });
                    archived_indices.insert(call.id.clone(), self.archived_tool_calls.len() - 1);
                }
            }

            if stored.role == "tool" {
                if let Some(tool_call_id) = stored.tool_call_id.as_ref() {
                    if let Some(idx) = archived_indices.get(tool_call_id) {
                        if let Some(entry) = self.archived_tool_calls.get_mut(*idx) {
                            entry.tool_call.status =
                                if stored.tool_status.as_deref() == Some("error") {
                                    ToolCallStatus::Error
                                } else {
                                    ToolCallStatus::Completed
                                };

                            let result_text = stored
                                .tool_result_json
                                .as_ref()
                                .map(|value| value.to_string())
                                .unwrap_or_else(|| stored.content.clone());
                            if entry.tool_call.status == ToolCallStatus::Error {
                                entry.tool_call.error = Some(result_text.clone());
                            } else {
                                entry.tool_call.result = Some(result_text.clone());
                            }
                        }
                    }
                }
            }
        }
    }

    fn format_json_string(raw: &str) -> String {
        match serde_json::from_str::<Value>(raw) {
            Ok(value) => serde_json::to_string_pretty(&value).unwrap_or_else(|_| raw.to_string()),
            Err(_) => raw.to_string(),
        }
    }

    fn coerce_value(raw: &str) -> Value {
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
            println!("🗂️ Config load cwd: {}", cwd.display());
        }
        println!("⚙️ Loaded default profile key: '{}'", config.default);
        if let Some(p) = config.get_default_profile() {
            let masked = if p.api_key.len() > 6 { format!("{}...{}", &p.api_key[..3], &p.api_key[p.api_key.len().saturating_sub(3)..]) } else { "***".to_string() };
            println!("🔧 Default profile details → model='{}' endpoint='{}' api_key='{}'", p.model, p.endpoint, masked);
        } else {
            println!("❗ No default profile found; using fallback defaults");
        }
        let storage = Storage::new_default().unwrap_or_else(|e| {
            eprintln!("Failed to initialize SQLite storage: {}", e);
            // Fallback to a temporary database
            Storage::new(std::env::temp_dir().join("cosmic_llm_temp.db"))
                .expect("Failed to create temporary database")
        });
        
        // Initialize prompt manager
        let prompt_manager = crate::prompts::PromptManager::load_from_config(&config.prompts)
            .unwrap_or_else(|e| {
                eprintln!("Failed to load prompts: {}", e);
                crate::prompts::PromptManager::load_from_config(&crate::prompts::PromptConfig::default()).unwrap()
            });
        
        // Initialize MCP registry (non-blocking)
        let mcp_registry = Arc::new(RwLock::new(MCPServerRegistry::new()));
        let mcp_registry_clone = mcp_registry.clone();
        
        // Try to load MCP config from JSON file (new Claude Desktop format)
        // Falls back to embedded TOML format if JSON doesn't exist
        let mcp_config = crate::config::MCPConfig::load_from_json()
            .unwrap_or_else(|e| {
                println!("📝 No mcp_config.json found (or error loading): {}", e);
                println!("📝 Falling back to embedded TOML config");
                config.mcp.clone()
            });
        
        println!("🔧 MCP Servers configured: {}", mcp_config.servers.len());
        for (name, _) in &mcp_config.servers {
            println!("  • {}", name);
        }
        
        tokio::spawn(async move {
            let mut registry = mcp_registry_clone.write().await;
            if let Err(e) = registry.initialize_from_config(&mcp_config).await {
                eprintln!("Failed to initialize MCP registry: {}", e);
            }
        });
        
        // Initialize LLM client based on default profile's backend
        let llm_client: Arc<dyn LlmClient> = {
            let profile = config.get_default_profile().unwrap_or(&crate::config::LlmProfile::default()).clone();
            match profile.backend.as_str() {
                "anthropic" => Arc::new(crate::llm::anthropic::AnthropicClient::new(profile)),
                "deepseek" | "openai" => Arc::new(crate::llm::openai::OpenAIClient::new(profile)),
                "ollama" => Arc::new(crate::llm::ollama::OllamaClient::new(profile)),
                "gemini" => Arc::new(crate::llm::gemini::GeminiClient::new(profile)),
                _ => Arc::new(crate::llm::openai::OpenAIClient::new(profile)),
            }
        };
        
        let mut app = Self::new(core, config, storage, prompt_manager, mcp_registry, llm_client);
        
        // Check for conversations with "Generating title..." and retry title generation
        // Note: We'll handle this in the main thread instead of async task
        // since Storage is not cloneable
        println!("🔍 Checking for conversations with 'Generating title...'");
        let conversations = app.storage.list_conversations().unwrap_or_else(|e| {
            eprintln!("Failed to list conversations: {}", e);
            Vec::new()
        });
        let conversation_ids: Vec<_> = conversations.into_iter()
            .filter(|conv| conv.title == "Generating title...")
            .map(|conv| conv.id)
            .collect();
        
        for conv_id in conversation_ids {
            println!("🔄 Found conversation {} with 'Generating title...', retrying...", conv_id);
            
            // Get the first user message to generate title from
            if let Ok(Some(conversation)) = app.storage.get_conversation(&conv_id) {
                if let Some(first_user_msg) = conversation.messages.iter().find(|msg| msg.role == "user") {
                    let message_text = &first_user_msg.content;
                    println!("📝 Retrying title generation for: '{}'", message_text);
                    
                    // Create a simple title based on first few words
                    let fallback_title = if message_text.len() > 50 {
                        format!("{}...", &message_text[..47])
                    } else {
                        message_text.clone()
                    };
                    
                    if let Err(e) = app.storage.update_conversation_title(&conv_id, fallback_title.clone()) {
                        eprintln!("Failed to update conversation title: {}", e);
                    }
                    println!("💾 Updated title to: {}", fallback_title);
                }
            }
        }
        println!("✅ Finished checking for conversations with 'Generating title...'");
        
        // Add welcome message
        app.messages.push(ChatMessage {
            content: "Welcome to Cosmic AI".to_string(),
            is_user: false,
            is_error: false,
        });
        
        // Load MCP tools on startup (same as refresh button)
        let load_tools_task = cosmic::Task::perform(
            async move {
                // Wait for MCP servers to initialize (give them more time)
                tokio::time::sleep(tokio::time::Duration::from_millis(5000)).await;
                println!("🔄 Startup: Attempting to refresh MCP tools...");
                cosmic::Action::App(Message::RefreshMCPTools)
            },
            |msg| msg,
        );
        
        let tasks = vec![load_tools_task];

        (app, app::Task::batch(tasks))
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        // Create a subscription for streaming LLM responses
        if self.is_streaming {
            self.create_streaming_subscription(self.current_streaming_id)
        } else {
            Subscription::none()
        }
    }

    fn update(&mut self, message: Self::Message) -> app::Task<Self::Message> {
        match message {
            Message::InputChanged(input) => {
                self.input = input;
            }
            Message::InputActionPerformed(action) => {
                self.input_content.perform(action);
                self.input = self.input_content.text();
            }
            Message::SendMessage => {
                println!("🔍 DEBUG: SendMessage received. Input: '{}', Attachments: {}", 
                    self.input, self.attached_files.len());
                // Allow sending if there's text OR if there are attachments
                if !self.input.trim().is_empty() || !self.attached_files.is_empty() {
                    // Create new conversation if none exists
                    if self.current_conversation_id.is_none() {
                        let conv_id = self.storage.create_conversation("Generating title...".to_string())
                            .unwrap_or_else(|e| {
                                eprintln!("Failed to create conversation: {}", e);
                                Uuid::new_v4()
                            });
                        self.current_conversation_id = Some(conv_id);
                        
                        // Generate title synchronously
                        println!("🚀 Starting title generation for conversation {}", conv_id);
                        let message_text = self.input.clone();
                        
                        // Create a simple title based on first few words
                        let fallback_title = if message_text.len() > 50 {
                            format!("{}...", &message_text[..47])
                        } else {
                            message_text
                        };
                        
                        println!("🎯 Generated title: '{}'", fallback_title);
                        if let Err(e) = self.storage.update_conversation_title(&conv_id, fallback_title.clone()) {
                            eprintln!("Failed to update conversation title: {}", e);
                        }
                        println!("💾 Saved title to storage for conversation {}: {}", conv_id, fallback_title);
                    }
                    
                    // Create user message content
                    let message_content = self.input.clone();
                    
                    // Add user message
                    let user_msg = ChatMessage {
                        content: message_content,
                        is_user: true,
                        is_error: false,
                    };
                    self.messages.push(user_msg.clone());
                    
                    // Add to storage
                    if let Some(conv_id) = self.current_conversation_id {
                        if let Err(e) = self.storage.add_message_to_conversation(&conv_id, "user".to_string(), self.input.clone()) {
                            eprintln!("Failed to add message to conversation: {}", e);
                        }
                    }
                    
                    // Send to LLM and get response
                    let input_text = self.input.clone();
                    self.input.clear();
                    self.input_content = text_editor::Content::new();
                    
                    // Assistant bubble will be created when streaming starts
                    self.current_ai_message_index = None;
                    
                    // Create attachments for the current message FIRST
                    let mut attachments = Vec::new();
                    println!("🔍 DEBUG: Processing {} attached files: {:?}", self.attached_files.len(), self.attached_files);
                    for file_path in &self.attached_files {
                        println!("🔍 DEBUG: Processing file: {}", file_path);
                        match crate::llm::file_utils::create_attachment(file_path) {
                            Ok(attachment) => {
                                println!("🔍 DEBUG: Created attachment: {:?}", attachment);
                                // Validate file for LLM
                                if let Err(e) = crate::llm::file_utils::validate_file_for_llm(&attachment) {
                                    println!("❌ DEBUG: File validation failed: {}", e);
                                    self.current_error = Some(format!("File validation error for {}: {}", file_path, e));
                                    return app::Task::none();
                                }
                                println!("✅ DEBUG: File validation passed");
                                attachments.push(attachment);
                            }
                            Err(e) => {
                                println!("❌ DEBUG: Failed to create attachment: {}", e);
                                self.current_error = Some(format!("Failed to process file {}: {}", file_path, e));
                                return app::Task::none();
                            }
                        }
                    }
                    println!("🔍 DEBUG: Final attachments count: {}", attachments.len());
                    
                    // Convert messages to LLM format
                    let mut llm_messages = Vec::new();
                    
                    // Add system prompt if available
                    if let Some(system_prompt) = self.prompt_manager.get_system_prompt() {
                        llm_messages.push(crate::llm::Message::new(
                            crate::llm::Role::System,
                            system_prompt.to_string()
                        ));
                    }
                    
                    for msg in &self.messages {
                        let role = if msg.is_user { 
                            crate::llm::Role::User 
                        } else { 
                            crate::llm::Role::Assistant 
                        };
                        llm_messages.push(crate::llm::Message::new(role, msg.content.clone()));
                    }
                    
                    // Create the current user message with attachments
                    let current_user_message = if attachments.is_empty() {
                        crate::llm::Message::new(crate::llm::Role::User, input_text.clone())
                    } else {
                        crate::llm::Message::new_with_attachments(crate::llm::Role::User, input_text.clone(), attachments)
                    };
                    
                    // Debug: Print the final message that will be sent to LLM
                    println!("🔍 DEBUG: Final LLM message with attachments: {:?}", current_user_message);
                    
                    llm_messages.push(current_user_message);
                    
                    // Clear attached files after processing
                    self.attached_files.clear();
                    
                    // Debug: Print all messages being sent to LLM
                    println!("🔍 DEBUG: All LLM messages being sent:");
                    for (i, msg) in llm_messages.iter().enumerate() {
                        println!("  Message {}: role={:?}, content={}, attachments={:?}", 
                            i, msg.role, msg.content, msg.attachments);
                    }
                    
                    // Store the prepared messages for the subscription to use
                    self.pending_llm_messages = Some(llm_messages);
                    
                    // Start streaming LLM response
                    let streaming_id = uuid::Uuid::new_v4();
                    self.current_streaming_id = Some(streaming_id);
                    self.is_streaming = true;
                    
                    // Store the last user message for retry functionality
                    self.last_user_message = Some(input_text.clone());
                    
                    // The scrollable widget will automatically scroll to show new content
                    // due to the spacer at the bottom
                }
            }
            Message::StopMessage => {
                if self.is_streaming {
                    // Stop the current streaming
                    self.is_streaming = false;
                    self.current_streaming_id = None;
                    self.pending_llm_messages = None; // Clear prepared messages
                    
                    // Remove any incomplete assistant message
                    if let Some(index) = self.current_ai_message_index {
                        if index < self.messages.len() && !self.messages[index].is_user {
                            self.messages.remove(index);
                        }
                    }
                    self.current_ai_message_index = None;
                }
            }
            Message::RetryMessage => {
                if let Some(last_msg) = &self.last_user_message {
                    // Stop current streaming if any
                    if self.is_streaming {
                        self.is_streaming = false;
                        self.current_streaming_id = None;
                    }
                    
                    // Remove the last assistant message if it exists
                    if let Some(index) = self.current_ai_message_index {
                        if index < self.messages.len() && !self.messages[index].is_user {
                            self.messages.remove(index);
                        }
                    }
                    
                    // Resend the last user message
                    self.input = last_msg.clone();
                    // Trigger SendMessage with the retried message
                    return self.update(Message::SendMessage);
                }
            }
            Message::AttachFile => {
                println!("🔍 DEBUG: AttachFile message received");
                // Use libcosmic's file chooser
                return cosmic::task::future(async move {
                    // Create file filters for supported file types
                    let text_filter = FileFilter::new("Text files")
                        .extension("txt")
                        .extension("md")
                        .extension("json")
                        .extension("xml")
                        .extension("csv")
                        .extension("log")
                        .extension("yaml")
                        .extension("yml")
                        .extension("rs")
                        .extension("py")
                        .extension("js")
                        .extension("ts")
                        .extension("html")
                        .extension("css");
                    
                    let image_filter = FileFilter::new("Image files")
                        .extension("jpg")
                        .extension("jpeg")
                        .extension("png")
                        .extension("gif")
                        .extension("bmp")
                        .extension("webp")
                        .extension("svg");
                    
                    let document_filter = FileFilter::new("Document files")
                        .extension("pdf")
                        .extension("doc")
                        .extension("docx")
                        .extension("xls")
                        .extension("xlsx")
                        .extension("ppt")
                        .extension("pptx");
                    
                    let dialog = file_chooser::open::Dialog::new()
                        .title("Select File to Attach")
                        .filter(text_filter)
                        .filter(image_filter)
                        .filter(document_filter);
                    
                    match dialog.open_file().await {
                        Ok(response) => {
                            let url = response.url();
                            if let Ok(path) = url.to_file_path() {
                                Message::FileSelected(path.to_string_lossy().to_string())
                            } else {
                                Message::FileChooserError(Arc::new(file_chooser::Error::UrlAbsolute))
                            }
                        }
                        Err(file_chooser::Error::Cancelled) => Message::FileChooserCancelled,
                        Err(why) => Message::FileChooserError(Arc::new(why)),
                    }
                });
            }
            Message::FileSelected(file_path) => {
                println!("🔍 DEBUG: File selected: {}", file_path);
                if !self.attached_files.contains(&file_path) {
                    self.attached_files.push(file_path);
                    println!("🔍 DEBUG: File added to attached_files. Current count: {}", self.attached_files.len());
                } else {
                    println!("🔍 DEBUG: File already in attached_files");
                }
            }
            Message::RemoveFile(file_path) => {
                self.attached_files.retain(|f| f != &file_path);
            }
            Message::FileChooserCancelled => {
                // User cancelled file selection - do nothing
            }
            Message::FileChooserError(error) => {
                if let Some(error) = Arc::into_inner(error) {
                    self.current_error = Some(format!("File selection error: {}", error));
                }
            }
            Message::NavigateTo(page) => {
                self.current_page = page;
                
                // Refresh MCP tools when navigating to MCP config page or Chat page
                if page == NavigationPage::MCPConfig || page == NavigationPage::Chat {
                    // Immediately try to get cached tools
                    if let Ok(registry) = self.mcp_registry.try_read() {
                        self.available_mcp_tools = registry.get_available_tools();
                        self.tool_states = registry.get_tool_states();
                    }
                }
            }
            Message::SelectConversation(id) => {
                self.current_conversation_id = Some(id);
                self.current_page = NavigationPage::Chat;
                if let Ok(Some(conv)) = self.storage.get_conversation(&id) {
                    self.rebuild_conversation_view(conv);
                }
            }
            Message::DeleteConversation(id) => {
                // If deleting the active conversation, clear the chat
                if self.current_conversation_id == Some(id) {
                    self.current_conversation_id = None;
                    self.messages.clear();
                    self.input.clear();
                }
                let _ = self.storage.delete_conversation(&id);
                // Stay on History page to reflect changes
                self.current_page = NavigationPage::History;
            }
            Message::NewConversation => {
                self.current_conversation_id = None;
                self.messages.clear();
                self.input.clear();
                self.current_page = NavigationPage::Chat;
                self.active_tool_calls.clear();
                self.archived_tool_calls.clear();
                self.current_ai_message_index = None;
                self.pending_tool_calls_for_history.clear();
                self.tool_runtime_context.clear();
            }
            Message::AgentUpdate(u) => match u {
                AgentUpdate::AssistantStreamingStarted => {
                    self.pending_tool_calls_for_history.clear();
                    self.tool_runtime_context.clear();
                    self.active_tool_calls.clear();
                    self.messages.push(ChatMessage {
                        content: String::new(),
                        is_user: false,
                        is_error: false,
                    });
                    self.current_ai_message_index = Some(self.messages.len() - 1);
                }
                AgentUpdate::AssistantDelta { text_chunk, .. } => {
                    if let Some(idx) = self.current_ai_message_index {
                        if let Some(msg) = self.messages.get_mut(idx) {
                            msg.content.push_str(&text_chunk);
                        }
                    }
                }
                AgentUpdate::AssistantComplete { full_text } => {
                    if let Some(idx) = self.current_ai_message_index {
                        if let Some(msg) = self.messages.get_mut(idx) {
                            msg.content = full_text.clone();
                        }
                    } else {
                        self.messages.push(ChatMessage {
                            content: full_text.clone(),
                            is_user: false,
                            is_error: false,
                        });
                        self.current_ai_message_index = Some(self.messages.len() - 1);
                    }
                    if let Some(conv_id) = self.current_conversation_id {
                        let tool_calls_slice = if self.pending_tool_calls_for_history.is_empty() {
                            None
                        } else {
                            Some(self.pending_tool_calls_for_history.as_slice())
                        };
                        let metadata = MessageMetadata {
                            tool_calls: tool_calls_slice,
                            tool_call_id: None,
                            tool_name: None,
                            tool_status: None,
                            tool_params_json: None,
                            tool_result_json: None,
                        };
                        if let Err(e) = self.storage.add_message_with_metadata(
                            &conv_id,
                            "assistant".to_string(),
                            full_text,
                            None,
                            metadata,
                        ) {
                            eprintln!("Failed to add assistant message: {}", e);
                        }
                    }
                    self.pending_tool_calls_for_history.clear();
                }
                AgentUpdate::ToolPlanned { plan_items } => {
                    let anchor = self.current_ai_message_index.unwrap_or_else(|| self.messages.len().saturating_sub(1));
                    for plan in plan_items {
                        let params_value: Value = serde_json::from_str(&plan.params_json)
                            .unwrap_or(Value::String(plan.params_json.clone()));
                        let params_pretty = serde_json::to_string_pretty(&params_value)
                            .unwrap_or(plan.params_json.clone());
                        let tool_call = ToolCall {
                            id: plan.id.clone(),
                            name: plan.name.clone(),
                            parameters: params_value.clone(),
                        };
                        self.pending_tool_calls_for_history.push(tool_call.clone());
                        self.tool_runtime_context.insert(
                            plan.id.clone(),
                            ToolRuntimeContext {
                                anchor_index: anchor,
                                params: Some(params_value.clone()),
                            },
                        );
                        self.active_tool_calls.push(ToolCallInfo {
                            id: Some(plan.id),
                            tool_name: plan.name,
                            parameters: params_pretty,
                            status: ToolCallStatus::Started,
                            result: None,
                            error: None,
                        });
                    }
                }
                AgentUpdate::ToolStarted {
                    tool_call_id,
                    name,
                    params_json,
                } => {
                    let params_value: Value = serde_json::from_str(&params_json)
                        .unwrap_or(Value::String(params_json.clone()));
                    let params_pretty = serde_json::to_string_pretty(&params_value)
                        .unwrap_or(params_json.clone());
                    let anchor = self
                        .current_ai_message_index
                        .unwrap_or_else(|| self.messages.len().saturating_sub(1));

                    self.tool_runtime_context
                        .entry(tool_call_id.clone())
                        .and_modify(|ctx| {
                            if ctx.params.is_none() {
                                ctx.params = Some(params_value.clone());
                            }
                        })
                        .or_insert(ToolRuntimeContext {
                            anchor_index: anchor,
                            params: Some(params_value.clone()),
                        });

                    if let Some(existing) = self
                        .active_tool_calls
                        .iter_mut()
                        .find(|tc| tc.id.as_ref().map(|s| s == &tool_call_id).unwrap_or(false))
                    {
                        existing.tool_name = name.clone();
                        existing.parameters = params_pretty;
                        existing.status = ToolCallStatus::Started;
                        existing.result = None;
                        existing.error = None;
                    } else {
                        self.active_tool_calls.push(ToolCallInfo {
                            id: Some(tool_call_id),
                            tool_name: name,
                            parameters: params_pretty,
                            status: ToolCallStatus::Started,
                            result: None,
                            error: None,
                        });
                    }
                }
                AgentUpdate::ToolResult {
                    tool_call_id,
                    name,
                    result_json,
                } => {
                    let context = self.tool_runtime_context.get(&tool_call_id).cloned();
                    let result_display = Self::format_json_string(&result_json);
                    let anchor = context
                        .as_ref()
                        .map(|ctx| ctx.anchor_index)
                        .or(self.current_ai_message_index)
                        .unwrap_or_else(|| self.messages.len().saturating_sub(1));

                    let mut archived_entry = None;
                    if let Some(pos) = self
                        .active_tool_calls
                        .iter()
                        .position(|tc| tc.id.as_ref().map(|s| s == &tool_call_id).unwrap_or(false))
                    {
                        let mut info = self.active_tool_calls.remove(pos);
                        info.status = ToolCallStatus::Completed;
                        info.result = Some(result_display.clone());
                        archived_entry = Some(info);
                    }
                    if archived_entry.is_none() {
                        let params_pretty = context
                            .as_ref()
                            .and_then(|ctx| ctx.params.as_ref())
                            .map(|value| serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string()))
                            .unwrap_or_else(|| "{}".to_string());
                        archived_entry = Some(ToolCallInfo {
                            id: Some(tool_call_id.clone()),
                            tool_name: name.clone(),
                            parameters: params_pretty,
                            status: ToolCallStatus::Completed,
                            result: Some(result_display.clone()),
                            error: None,
                        });
                    }

                    if let Some(entry) = archived_entry {
                        self.archived_tool_calls
                            .push(AnchoredToolCall { anchor_index: anchor, tool_call: entry });
                    }

                    if let Some(conv_id) = self.current_conversation_id {
                        let params_owned = context
                            .as_ref()
                            .and_then(|ctx| ctx.params.clone());
                        let params_ref = params_owned.as_ref();
                        let result_value = Self::coerce_value(&result_json);
                        let metadata = MessageMetadata {
                            tool_calls: None,
                            tool_call_id: Some(tool_call_id.as_str()),
                            tool_name: Some(name.as_str()),
                            tool_status: Some("success"),
                            tool_params_json: params_ref,
                            tool_result_json: Some(&result_value),
                        };
                        if let Err(e) = self.storage.add_message_with_metadata(
                            &conv_id,
                            "tool".to_string(),
                            result_json.clone(),
                            None,
                            metadata,
                        ) {
                            eprintln!("Failed to add tool result: {}", e);
                        }
                    }

                    self.tool_runtime_context.remove(&tool_call_id);
                }
                AgentUpdate::ToolError {
                    tool_call_id,
                    name,
                    error,
                    retryable: _,
                } => {
                    let context = self.tool_runtime_context.get(&tool_call_id).cloned();
                    let anchor = context
                        .as_ref()
                        .map(|ctx| ctx.anchor_index)
                        .or(self.current_ai_message_index)
                        .unwrap_or_else(|| self.messages.len().saturating_sub(1));

                    let mut archived_entry = None;
                    if let Some(pos) = self
                        .active_tool_calls
                        .iter()
                        .position(|tc| tc.id.as_ref().map(|s| s == &tool_call_id).unwrap_or(false))
                    {
                        let mut info = self.active_tool_calls.remove(pos);
                        info.status = ToolCallStatus::Error;
                        info.error = Some(error.clone());
                        archived_entry = Some(info);
                    }
                    if archived_entry.is_none() {
                        let params_pretty = context
                            .as_ref()
                            .and_then(|ctx| ctx.params.as_ref())
                            .map(|value| serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string()))
                            .unwrap_or_else(|| "{}".to_string());
                        archived_entry = Some(ToolCallInfo {
                            id: Some(tool_call_id.clone()),
                            tool_name: name.clone(),
                            parameters: params_pretty,
                            status: ToolCallStatus::Error,
                            result: None,
                            error: Some(error.clone()),
                        });
                    }

                    if let Some(entry) = archived_entry {
                        self.archived_tool_calls
                            .push(AnchoredToolCall { anchor_index: anchor, tool_call: entry });
                    }

                    if let Some(conv_id) = self.current_conversation_id {
                        let params_owned = context
                            .as_ref()
                            .and_then(|ctx| ctx.params.clone());
                        let params_ref = params_owned.as_ref();
                        let error_value = Value::String(error.clone());
                        let metadata = MessageMetadata {
                            tool_calls: None,
                            tool_call_id: Some(tool_call_id.as_str()),
                            tool_name: Some(name.as_str()),
                            tool_status: Some("error"),
                            tool_params_json: params_ref,
                            tool_result_json: Some(&error_value),
                        };
                        if let Err(e) = self.storage.add_message_with_metadata(
                            &conv_id,
                            "tool".to_string(),
                            error.clone(),
                            None,
                            metadata,
                        ) {
                            eprintln!("Failed to add tool error: {}", e);
                        }
                    }

                    self.tool_runtime_context.remove(&tool_call_id);
                }
                AgentUpdate::ConversationComplete { final_text: _ } => {
                    if let Some(idx) = self.current_ai_message_index {
                        let should_remove = self
                            .messages
                            .get(idx)
                            .map(|m| !m.is_user && m.content.trim().is_empty())
                            .unwrap_or(false);
                        if should_remove {
                            self.messages.remove(idx);
                            for anchored in &mut self.archived_tool_calls {
                                if anchored.anchor_index > idx {
                                    anchored.anchor_index -= 1;
                                } else if anchored.anchor_index == idx {
                                    anchored.anchor_index = idx.saturating_sub(1);
                                }
                            }
                        }
                    }
                    self.is_streaming = false;
                    self.current_streaming_id = None;
                    self.current_ai_message_index = None;
                    self.pending_llm_messages = None;
                    self.active_tool_calls.clear();
                    self.pending_tool_calls_for_history.clear();
                    self.tool_runtime_context.clear();
                }
                AgentUpdate::ModelError { error } => {
                        // Stop streaming and show error message
                        self.is_streaming = false;
                        self.current_streaming_id = None;
                        self.current_ai_message_index = None;
                        self.pending_llm_messages = None;
                        self.active_tool_calls.clear();
                    self.pending_tool_calls_for_history.clear();
                    self.tool_runtime_context.clear();
                        
                        // Add error message as a separate chat bubble
                        self.messages.push(ChatMessage { 
                            content: format!("❌ **Model Communication Error**\n\n{}", error), 
                            is_user: false,
                            is_error: true
                        });
                    }
            },
            Message::ToolCallStarted(tool_name, parameters) => {
                // Add tool call to active list
                self.active_tool_calls.push(ToolCallInfo {
                    id: None,
                    tool_name: tool_name.clone(),
                    parameters,
                    status: ToolCallStatus::Started,
                    result: None,
                    error: None,
                });
            }
            Message::ToolCallCompleted(tool_name, result) => {
                // Update tool call status
                if let Some(tool_call) = self.active_tool_calls.iter_mut().find(|tc| tc.tool_name == tool_name) {
                    tool_call.status = ToolCallStatus::Completed;
                    tool_call.result = Some(result);
                }
            }
            Message::ToolCallError(tool_name, error) => {
                // Update tool call status
                if let Some(tool_call) = self.active_tool_calls.iter_mut().find(|tc| tc.tool_name == tool_name) {
                    tool_call.status = ToolCallStatus::Error;
                    tool_call.error = Some(error);
                }
            }
            Message::ToolCallWidgetMessage(index, message) => {
                // Handle tool call widget interactions
                match message {
                    ToolCallMessage::ToggleExpanded => {
                        if self.expanded_tool_calls.contains(&index) {
                            self.expanded_tool_calls.remove(&index);
                        } else {
                            self.expanded_tool_calls.insert(index);
                        }
                    }
                }
            }
            Message::ScrollToBottom => {
                // For now, we'll rely on the spacer at the bottom to force scroll
                // The scrollable widget should automatically scroll to show new content
                // This is a placeholder for future scroll-to-bottom implementation
            }
            Message::ShowAbout => {
                // Toggle behavior: if About is already shown, hide it; otherwise show it
                // Pattern from msToDO for consistent UX
                if self.context_page == ContextPage::About && self.core.window.show_context {
                    self.core.window.show_context = false;  // Toggle off
                } else {
                    self.context_page = ContextPage::About;
                    self.core.window.show_context = true;   // Show
                }
            }
            Message::CloseAbout => {
                self.core.window.show_context = false;
            }
            Message::OpenUrl(url) => {
                let _ = webbrowser::open(&url);
            }
            Message::OpenSettings => {
                self.current_page = NavigationPage::Settings;
            }
            Message::Quit => {
                // TODO: Implement proper quit
                std::process::exit(0);
            }
            Message::ChangeDefaultProfile(profile_index) => {
                // Must sort the same way as in the view to maintain index consistency
                let mut profile_names: Vec<String> = self.config.profiles.keys().cloned().collect();
                profile_names.sort();
                if let Some(profile_name) = profile_names.get(profile_index) {
                    self.config.default = profile_name.clone();
                    self.settings_changed = true;
                    // Recreate LLM client for new default provider
                    if let Some(profile) = self.config.get_default_profile().cloned() {
                        let masked = if profile.api_key.len() > 6 { format!("{}...{}", &profile.api_key[..3], &profile.api_key[profile.api_key.len().saturating_sub(3)..]) } else { "***".to_string() };
                        println!("🔄 Switching default profile to '{}' model='{}' endpoint='{}' api_key='{}'", self.config.default, profile.model, profile.endpoint, masked);
                        self.llm_client = match profile.backend.as_str() {
                            "anthropic" => Arc::new(crate::llm::anthropic::AnthropicClient::new(profile)),
                            "deepseek" | "openai" => Arc::new(crate::llm::openai::OpenAIClient::new(profile)),
                            "ollama" => Arc::new(crate::llm::ollama::OllamaClient::new(profile)),
                            "gemini" => Arc::new(crate::llm::gemini::GeminiClient::new(profile)),
                            _ => Arc::new(crate::llm::openai::OpenAIClient::new(profile)),
                        };
                    }
                }
            }
            Message::SaveSettings => {
                if let Err(e) = self.config.save() {
                    eprintln!("Failed to save settings: {}", e);
                } else {
                    self.settings_changed = false;
                    println!("Settings saved successfully");
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
                        if self.config.profiles.contains_key(&name) {
                            self.config.default = name;
                            self.settings_changed = true;
                            if let Some(profile) = self.config.get_default_profile().cloned() {
                                self.llm_client = match profile.backend.as_str() {
                                    "anthropic" => Arc::new(crate::llm::anthropic::AnthropicClient::new(profile)),
                                    "deepseek" | "openai" => Arc::new(crate::llm::openai::OpenAIClient::new(profile)),
                                    "ollama" => Arc::new(crate::llm::ollama::OllamaClient::new(profile)),
                                    "gemini" => Arc::new(crate::llm::gemini::GeminiClient::new(profile)),
                                    _ => Arc::new(crate::llm::openai::OpenAIClient::new(profile)),
                                };
                            }
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
                    SimpleSettingsMessage::AddNewProfile => {
                        let name = self.settings_page.new_profile_name.trim().to_string();
                        let model = self.settings_page.new_profile_model.trim().to_string();
                        let endpoint = self.settings_page.new_profile_endpoint.trim().to_string();
                        if !name.is_empty() && !model.is_empty() {
                            let mut profile = LlmProfile::default();
                            profile.backend = "openai".to_string();
                            profile.model = model;
                            profile.endpoint = endpoint;
                            profile.temperature = Some(0.7);
                            profile.max_tokens = Some(1000);
                            self.config.profiles.insert(name.clone(), profile);
                            if self.config.default.is_empty() {
                                self.config.default = name.clone();
                            }
                            self.settings_changed = true;
                            // Clear inputs
                            self.settings_page.new_profile_name.clear();
                            self.settings_page.new_profile_model.clear();
                            self.settings_page.new_profile_endpoint.clear();
                        }
                    }
                }
            }
            Message::DialogAction(action) => {
                match action {
                    DialogAction::Close => {
                        self.dialog = None;
                    }
                    DialogAction::CopyText => {
                        // Copy the current dialog text to clipboard
                        if let Some(DialogPage::MessageText(content)) = &self.dialog {
                            let _ = cli_clipboard::set_contents(content.text());
                        }
                        // Keep dialog open for multiple copies
                    }
                    DialogAction::TextEditorAction(action) => {
                        // Handle text editor actions to enable selection
                        if let Some(DialogPage::MessageText(content)) = &mut self.dialog {
                            content.perform(action);
                        }
                    }
                }
            }
            Message::ShowMessageDialog(content) => {
                self.dialog = Some(DialogPage::MessageText(text_editor::Content::with_text(&content)));
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
                    println!("🔄 RefreshMCPTools: Found {} tools", tools.len());
                    self.available_mcp_tools = tools;
                    // Also sync tool states
                    self.tool_states = registry.get_tool_states();
                } else {
                    println!("🔄 RefreshMCPTools: Failed to get registry read lock");
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
            Message::ShowToolsContext => {
                self.show_tools_context = true;
                self.core.window.show_context = true;
            }
            Message::HideToolsContext => {
                self.show_tools_context = false;
                self.core.window.show_context = false;
            }
            Message::MarkdownLinkClicked(url) => {
                let _ = webbrowser::open(url.as_str());
            }
            Message::SearchChanged(query) => {
                self.search_query = query.clone();
                // Perform search if query is not empty
                if !query.trim().is_empty() {
                    // Perform search synchronously since Storage doesn't implement Clone
                    match self.storage.search_history(&query, 50) {
                        Ok(results) => {
                            self.search_results = results;
                        }
                        Err(e) => {
                            eprintln!("Search error: {}", e);
                            self.search_results.clear();
                        }
                    }
                } else {
                    // Clear search results if query is empty
                    self.search_results.clear();
                }
            }
            Message::SearchResults(results) => {
                self.search_results = results;
            }
        }
        
        app::Task::none()
    }

    fn view(&self) -> Element<Self::Message> {
        // Main layout with side panel and content area
        let mut content = cosmic::widget::row::with_capacity(1)
            .push(
                // Main content area
                match self.current_page {
                    NavigationPage::Chat => chat::chat_view(self),
                    NavigationPage::History => history::history_view(self),
                    NavigationPage::MCPConfig => mcp_config::mcp_config_view(self),
                    NavigationPage::Settings => self.settings_page.view(&self.config).map(Message::SettingsMessage),
                }
            );

        // Add dialog overlay if dialog is open
        if let Some(dialog_page) = &self.dialog {
            content = content.push(
                dialog_page.view(&self.dialog_text_input_id)
            );
        }

        content.into()
    }

    fn header_start(&self) -> Vec<Element<Self::Message>> {
        vec![self.create_menu_bar()]
    }

    fn nav_model(&self) -> Option<&widget::segmented_button::SingleSelectModel> {
        Some(&self.nav_model)
    }

    fn on_nav_select(&mut self, entity: widget::segmented_button::Entity) -> app::Task<Self::Message> {
        if let Some(page) = self.nav_model.data::<NavigationPage>(entity) {
            self.current_page = *page;
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
                cosmic::widget::menu::Item::Button(
                    "Settings",
                    None,
                    NavMenuAction::Settings,
                ),
                cosmic::widget::menu::Item::Button(
                    "About",
                    None,
                    NavMenuAction::About,
                ),
                cosmic::widget::menu::Item::Button(
                    "Quit",
                    None,
                    NavMenuAction::Quit,
                ),
            ],
        ))
    }

    fn context_drawer(&self) -> Option<app::context_drawer::ContextDrawer<<Self as Application>::Message>> {
        if !self.core.window.show_context {
            return None;
        }
        
        if self.show_tools_context {
            Some(app::context_drawer::context_drawer(
                tools::tools_context_view(self),
                Message::HideToolsContext,
            )
            .title("Tool Configuration"))
        } else {
            Some(match self.context_page {
                ContextPage::About => app::context_drawer::about(
                    &self.about,
                    |url| Message::OpenUrl(url.to_string()),
                    Message::CloseAbout,
                )
                .title(self.context_page.title()),  // Dynamic title from ContextPage (pattern from msToDO)
            })
        }
    }
}

impl CosmicLlmApp {

    fn create_menu_bar(&self) -> Element<Message> {
        use cosmic::widget::menu::{items, root, Item, ItemHeight, ItemWidth, MenuBar, Tree};
        use cosmic::widget::RcElementWrapper;
        
        MenuBar::new(vec![
            Tree::with_children(
                RcElementWrapper::new(Element::from(root("File"))),
                items(
                    &self.key_binds,
                    vec![
                        Item::Button(
                            "Quit",
                            None,
                            MenuAction::Quit,
                        ),
                    ],
                ),
            ),
            Tree::with_children(
                RcElementWrapper::new(Element::from(root("View"))),
                items(
                    &self.key_binds,
                    vec![
                        Item::Button(
                            "Settings",
                            None,
                            MenuAction::Settings,
                        ),
                    ],
                ),
            ),
            Tree::with_children(
                RcElementWrapper::new(Element::from(root("Help"))),
                items(
                    &self.key_binds,
                    vec![
                        Item::Button(
                            "About",
                            None,
                            MenuAction::About,
                        ),
                    ],
                ),
            ),
        ])
        .item_height(ItemHeight::Dynamic(40))
        .item_width(ItemWidth::Uniform(200))
        .spacing(4.0)
        .into()
    }




}

