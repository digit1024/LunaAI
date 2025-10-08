use cosmic::{
    app::{self, Core},
    iced::{Length, Subscription},
    widget::{self, text_input, scrollable, menu, text_editor, markdown},
    Application, Element,
    dialog::file_chooser::{self, FileFilter},
};
use futures::StreamExt;
use std::sync::{Arc, Mutex};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::{
    config::{AppConfig, LlmProfile},
    storage::Storage,
    llm::LlmClient,
    mcp::MCPServerRegistry,
    prompts::PromptManager,
    ui::context::ContextPage,
    ui::pages::settings::{SimpleSettingsPage, SimpleSettingsMessage},
    ui::widgets::{ToolCallWidget, ToolCallMessage},
    ui::dialogs::{DialogAction, DialogPage},
};
use crate::agentic::protocol::AgentUpdate;

#[derive(Debug, Clone)]
pub enum Message {
    InputChanged(String),
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

#[derive(Debug, Clone)]
pub struct Turn {
    pub id: Uuid,
    pub iteration: u32,
    pub text: String,
    pub complete: bool,
    pub tools: Vec<ToolCallInfo>,
}

pub struct CosmicLlmApp {
    core: Core,
    config: AppConfig,
    storage: Storage,
    prompt_manager: PromptManager,
    input: String,
    messages: Vec<ChatMessage>,
    input_id: cosmic::widget::Id,
    current_page: NavigationPage,
    current_conversation_id: Option<Uuid>,
    mcp_registry: Arc<RwLock<MCPServerRegistry>>,
    llm_client: Arc<dyn LlmClient>,
    is_streaming: bool,
    current_streaming_id: Option<Uuid>,
    active_tool_calls: Vec<ToolCallInfo>,
    // Anchors tool calls under the AI message that executed them
    current_ai_message_index: Option<usize>,
    archived_tool_calls: Vec<AnchoredToolCall>,
    expanded_tool_calls: std::collections::HashSet<usize>,
    scrollable_id: cosmic::widget::Id,
    key_binds: std::collections::HashMap<menu::KeyBind, MenuAction>,
    settings_changed: bool,
    title_sender: Option<tokio::sync::mpsc::UnboundedSender<(Uuid, String)>>,
    settings_page: SimpleSettingsPage,
    context_page: ContextPage,
    about: widget::about::About,
    // Navigation model to integrate with COSMIC shell nav bar (pattern from msToDO)
    nav_model: widget::segmented_button::SingleSelectModel,
    // New agent protocol view model
    turns: Vec<Turn>,
    // When true, ignore legacy StreamingUpdate to avoid duplicate UI events
    agent_mode_active: bool,
    // Dialog state
    dialog: Option<DialogPage>,
    dialog_text_input_id: widget::Id,
    // MCP tools cache
    available_mcp_tools: Vec<crate::llm::ToolDefinition>,
    // Tool enable/disable state (tool_name -> enabled)
    tool_states: std::collections::HashMap<String, bool>,
    // Show tools context panel
    show_tools_context: bool,
    // Store last user message for retry functionality
    last_user_message: Option<String>,
    // Store attached files
    attached_files: Vec<String>,
    // Store current error message
    current_error: Option<String>,
    // Store prepared LLM messages with attachments for the current request
    pending_llm_messages: Option<Vec<crate::llm::Message>>,
}

#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub content: String,
    pub is_user: bool,
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

impl CosmicLlmApp {
    pub fn new(core: Core, config: AppConfig, storage: Storage, prompt_manager: PromptManager, mcp_registry: Arc<RwLock<MCPServerRegistry>>, llm_client: Arc<dyn LlmClient>) -> Self {
        // Create title sender channel
        let (title_sender, mut title_receiver) = tokio::sync::mpsc::unbounded_channel::<(Uuid, String)>();
        
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
                    .data(NavigationPage::Chat);
                model
                    .insert()
                    .text("History")
                    .data(NavigationPage::History)
                    .divider_above(true);
                model
                    .insert()
                    .text("MCP Config")
                    .data(NavigationPage::MCPConfig);
                model
                    .insert()
                    .text("Settings")
                    .data(NavigationPage::Settings)
                    .divider_above(true);
                // Activate first item - collect entity first to avoid borrow issues
                let first_entity = model.iter().next();
                if let Some(first) = first_entity {
                    model.activate(first);
                }
                model
            },
            turns: Vec::new(),
            agent_mode_active: true,
            dialog: None,
            dialog_text_input_id: widget::Id::unique(),
            available_mcp_tools: Vec::new(),
            tool_states: std::collections::HashMap::new(),
            show_tools_context: false,
            last_user_message: None,
            attached_files: Vec::new(),
            current_error: None,
            pending_llm_messages: None,
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
                        // Send error via AgentUpdate
                        let _ = tx_agent.send(AgentUpdate::EndConversation { 
                            final_text: format!("Error: {}", e) 
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
                    
                    // Do not create a placeholder bubble; BeginTurn will create the assistant bubble
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
                // Load conversation messages
                if let Ok(Some(conv)) = self.storage.get_conversation(&id) {
                    self.messages = conv.messages.iter().map(|msg| {
                        ChatMessage {
                            content: msg.content.clone(),
                            is_user: msg.role == "user",
                        }
                    }).collect();
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
            }
            Message::AgentUpdate(u) => {
                match u {
                    AgentUpdate::BeginTurn { conversation_id: _, turn_id, iteration, plan_summary } => {
                        // Start a new turn bubble
                        self.turns.push(Turn { id: turn_id, iteration, text: plan_summary.unwrap_or_default(), complete: false, tools: Vec::new() });
                        // Always create a fresh assistant message bubble for this turn
                        self.messages.push(ChatMessage { content: String::from(""), is_user: false });
                        self.current_ai_message_index = Some(self.messages.len() - 1);
                    }
                    AgentUpdate::AssistantDelta { turn_id: _, text_chunk, seq: _ } => {
                        if let Some(turn) = self.turns.last_mut() {
                            turn.text.push_str(&text_chunk);
                        }
                        if let Some(idx) = self.current_ai_message_index {
                            if let Some(msg) = self.messages.get_mut(idx) {
                                msg.content.push_str(&text_chunk);
                            }
                        }
                    }
                    AgentUpdate::AssistantComplete { turn_id: _, full_text } => {
                        if let Some(turn) = self.turns.last_mut() {
                            turn.text = full_text.clone();
                        }
                        // Mirror to legacy bubble
                        let mut wrote = false;
                        if let Some(last_msg) = self.messages.last_mut() {
                            if !last_msg.is_user {
                                last_msg.content = full_text.clone();
                                wrote = true;
                            }
                        }
                        if !wrote {
                            self.messages.push(ChatMessage { content: full_text.clone(), is_user: false });
                            self.current_ai_message_index = Some(self.messages.len() - 1);
                        }
                        if !full_text.trim().is_empty() {
                            if let Some(conv_id) = self.current_conversation_id {
                                if let Err(e) = self.storage.add_message_to_conversation(&conv_id, "assistant".to_string(), full_text) {
                                    eprintln!("Failed to add message to conversation: {}", e);
                                }
                            }
                        }
                    }
                    AgentUpdate::ToolPlanned { turn_id: _, plan_items: _ } => {
                        // Do not create placeholder rows; spinner covers planned state
                    }
                    AgentUpdate::ToolStarted { turn_id: _, tool_call_id, name, params_json } => {
                        // De-duplicate by id if already present (from previous start events)
                        if let Some(existing) = self.active_tool_calls.iter_mut().find(|tc| tc.id.as_ref().map(|s| s == &tool_call_id).unwrap_or(false)) {
                            existing.tool_name = name;
                            existing.parameters = params_json;
                            existing.status = ToolCallStatus::Started;
                            existing.result = None;
                            existing.error = None;
                        } else {
                            self.active_tool_calls.push(ToolCallInfo { id: Some(tool_call_id), tool_name: name, parameters: params_json, status: ToolCallStatus::Started, result: None, error: None });
                        }
                    }
                    AgentUpdate::ToolResult { turn_id: _, tool_call_id, name, result_json } => {
                        if let Some(tc) = self.active_tool_calls.iter_mut().find(|tc| tc.id.as_ref().map(|s| s == &tool_call_id).unwrap_or(false) || tc.tool_name == name) {
                            tc.status = ToolCallStatus::Completed;
                            tc.result = Some(result_json);
                        }
                    }
                    AgentUpdate::ToolError { turn_id: _, tool_call_id, name, error, retryable: _ } => {
                        if let Some(tc) = self.active_tool_calls.iter_mut().find(|tc| tc.id.as_ref().map(|s| s == &tool_call_id).unwrap_or(false) || tc.tool_name == name) {
                            tc.status = ToolCallStatus::Error;
                            tc.error = Some(error);
                        }
                    }
                    AgentUpdate::EndTurn { turn_id: _ } => {
                        // Archive active tools under current AI bubble
                        if let Some(anchor) = self.current_ai_message_index {
                            for tc in self.active_tool_calls.drain(..) {
                                self.archived_tool_calls.push(AnchoredToolCall { anchor_index: anchor, tool_call: tc });
                            }
                            // If the assistant bubble has no text, remove it and shift anchors
                            let should_remove = self.messages.get(anchor).map(|m| !m.is_user && m.content.trim().is_empty()).unwrap_or(false);
                            if should_remove {
                                self.messages.remove(anchor);
                                for anchored in &mut self.archived_tool_calls {
                                    if anchored.anchor_index > anchor {
                                        anchored.anchor_index -= 1;
                                    } else if anchored.anchor_index == anchor {
                                        anchored.anchor_index = anchor.saturating_sub(1);
                                    }
                                }
                            }
                        } else {
                            self.active_tool_calls.clear();
                        }
                        if let Some(turn) = self.turns.last_mut() { 
                            turn.complete = true;
                            // Persist turn to storage
                            if let Some(conv_id) = self.current_conversation_id {
                                let storage_tools: Vec<crate::storage::conversation_storage::ToolCallInfo> = turn.tools.iter().map(|tc| {
                                    crate::storage::conversation_storage::ToolCallInfo {
                                        id: tc.id.clone(),
                                        tool_name: tc.tool_name.clone(),
                                        parameters: tc.parameters.clone(),
                                        status: match tc.status {
                                            ToolCallStatus::Started => crate::storage::conversation_storage::ToolCallStatus::Started,
                                            ToolCallStatus::Completed => crate::storage::conversation_storage::ToolCallStatus::Completed,
                                            ToolCallStatus::Error => crate::storage::conversation_storage::ToolCallStatus::Error,
                                        },
                                        result: tc.result.clone(),
                                        error: tc.error.clone(),
                                    }
                                }).collect();
                                
                                let storage_turn = crate::storage::conversation_storage::Turn {
                                    id: turn.id,
                                    iteration: turn.iteration,
                                    text: turn.text.clone(),
                                    complete: turn.complete,
                                    tools: storage_tools,
                                };
                                self.storage.add_turn_to_conversation(&conv_id, storage_turn);
                            }
                        }
                        self.current_ai_message_index = None;
                    }
                    AgentUpdate::EndConversation { final_text: _ } => {
                        self.is_streaming = false;
                        self.current_streaming_id = None;
                        self.current_ai_message_index = None;
                        self.pending_llm_messages = None; // Clear prepared messages
                        // Clear any leftover active tool rows (e.g., from placeholders)
                        self.active_tool_calls.clear();
                    }
                    AgentUpdate::Heartbeat { turn_id: _, ts_ms: _ } => {}
                }
            }
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
                    SimpleSettingsMessage::ProfileSelected(index) => {
                        let profile_names: Vec<String> = self.config.profiles.keys().cloned().collect();
                        if let Some(profile_name) = profile_names.get(index) {
                            self.config.default = profile_name.clone();
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
                            let profile = LlmProfile {
                                backend: "openai".to_string(), // Default backend
                                api_key: String::new(),
                                model,
                                endpoint,
                                temperature: Some(0.7),
                                max_tokens: Some(1000),
                            };
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
        }
        
        app::Task::none()
    }

    fn view(&self) -> Element<Self::Message> {
        // Main layout with side panel and content area
        let mut content = cosmic::widget::row::with_capacity(1)
            .push(
                // Main content area
                match self.current_page {
                    NavigationPage::Chat => self.chat_view(),
                    NavigationPage::History => self.history_view(),
                    NavigationPage::MCPConfig => self.mcp_config_view(),
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
                self.tools_context_view(),
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

    fn chat_view(&self) -> Element<Message> {
        use cosmic::iced::{Length, Padding};
        
        cosmic::widget::column::with_capacity(3)
            .push(
                // Combined top panel with tools
                self.combined_top_panel()
            )
            .push(
                // Spacing between top panel and messages
                cosmic::widget::Space::with_height(Length::Fixed(16.0))
            )
            .push(
                // Messages area with better styling
                {
                    let mut column = cosmic::widget::column::with_capacity(self.messages.len()).spacing(12);
                    
                    // Add regular chat messages
                    for (i, msg) in self.messages.iter().enumerate() {
                        let content = msg.content.clone();
                        let message_widget = cosmic::widget::container(
                            {
                                let content_widget: Element<Message> = if msg.is_user {
                                    widget::container(
                                        cosmic::widget::text(&msg.content)
                                            .size(14)
                                            .class(cosmic::style::Text::Color(cosmic::iced::Color::WHITE))
                                    )
                                    .width(Length::Fill)
                                    .into()
                                } else {
                                    widget::container(
                                        widget::lazy(&msg.content, |_| {
                                            let items = markdown::parse(&msg.content).collect::<Vec<_>>();
                                            let style = widget::markdown::Style {
                                                inline_code_padding: cosmic::iced::Padding::from([1, 2]),
                                                inline_code_highlight: widget::markdown::Highlight {
                                                    background: cosmic::iced::Background::Color(cosmic::iced::Color::from_rgb(0.1, 0.1, 0.1)),
                                                    border: cosmic::iced::Border::default().rounded(2),
                                                },
                                                inline_code_color: cosmic::iced::Color::WHITE,
                                                link_color: cosmic::iced::Color::from_rgb(0.3, 0.6, 1.0),
                                            };
                                            widget::markdown(&items, widget::markdown::Settings::default(), style)
                                                .map(Message::MarkdownLinkClicked)
                                        })
                                    )
                                    .width(Length::Fill)
                                    .into()
                                };
                                
                                cosmic::widget::row::with_capacity(2)
                                .push(content_widget)
                                .push(
                                    cosmic::widget::button::text("📋")
                                        .on_press(Message::ShowMessageDialog(content))
                                        .padding(4)
                                        .class(cosmic::style::Button::Text)
                                )
                            }
                        )
                        .padding(Padding::from([12, 16]))
                        .class(if msg.is_user {
                            cosmic::style::Container::Primary
                        } else {
                            cosmic::style::Container::Card
                        })
                        .width(Length::FillPortion(7)); // 70% width
                        
                        let message_row = if msg.is_user {
                            // User messages: right-aligned
                            cosmic::widget::row::with_capacity(2)
                                .push(cosmic::widget::Space::with_width(Length::FillPortion(3)))
                                .push(message_widget)
                        } else {
                            // AI messages: left-aligned
                            cosmic::widget::row::with_capacity(2)
                                .push(message_widget)
                                .push(cosmic::widget::Space::with_width(Length::FillPortion(3)))
                        };
                        // Push the message first
                        column = column.push(message_row);
                        // If there are archived tool calls anchored to this message, render them right after
                        for (idx, anchored) in self.archived_tool_calls.iter().enumerate() {
                            if anchored.anchor_index == i {
                                let is_expanded = self.expanded_tool_calls.contains(&idx);
                                let tool_call = &anchored.tool_call;
                                let tool_name = tool_call.tool_name.clone();
                                let parameters = tool_call.parameters.clone();
                                let status = match tool_call.status {
                                    ToolCallStatus::Started => crate::ui::widgets::ToolCallStatus::Started,
                                    ToolCallStatus::Completed => crate::ui::widgets::ToolCallStatus::Completed,
                                    ToolCallStatus::Error => crate::ui::widgets::ToolCallStatus::Error,
                                };
                                let result = tool_call.result.clone();
                                let error = tool_call.error.clone();
                                let widget = Box::leak(Box::new(ToolCallWidget {
                                    tool_name,
                                    parameters,
                                    status,
                                    result,
                                    error,
                                    is_expanded,
                                }));
                                let widget_element = widget.view().map(move |msg| Message::ToolCallWidgetMessage(idx, msg));
                                let tool_call_row = cosmic::widget::row::with_capacity(2)
                                    .push(widget_element)
                                    .push(cosmic::widget::Space::with_width(Length::Fill));
                                column = column.push(tool_call_row);
                            }
                        }
                        // If we're on the currently streaming AI message, also render active tool calls inline
                        if let Some(anchor) = self.current_ai_message_index {
                            if anchor == i {
                                let offset = self.archived_tool_calls.len();
                                for (j, tool_call) in self.active_tool_calls.iter().enumerate() {
                                    let idx = offset + j;
                                    let is_expanded = self.expanded_tool_calls.contains(&idx);
                                    let tool_name = tool_call.tool_name.clone();
                                    let parameters = tool_call.parameters.clone();
                                    let status = match tool_call.status {
                                        ToolCallStatus::Started => crate::ui::widgets::ToolCallStatus::Started,
                                        ToolCallStatus::Completed => crate::ui::widgets::ToolCallStatus::Completed,
                                        ToolCallStatus::Error => crate::ui::widgets::ToolCallStatus::Error,
                                    };
                                    let result = tool_call.result.clone();
                                    let error = tool_call.error.clone();
                                    let widget = Box::leak(Box::new(ToolCallWidget {
                                        tool_name,
                                        parameters,
                                        status,
                                        result,
                                        error,
                                        is_expanded,
                                    }));
                                    let widget_element = widget.view().map(move |msg| Message::ToolCallWidgetMessage(idx, msg));
                                    let tool_call_row = cosmic::widget::row::with_capacity(2)
                                        .push(widget_element)
                                        .push(cosmic::widget::Space::with_width(Length::Fill));
                                    column = column.push(tool_call_row);
                                }
                                // If there are no active tool calls yet, but the current turn is not complete, show a spinner row
                                if self.active_tool_calls.is_empty() {
                                    if let Some(current_turn) = self.turns.last() {
                                        if !current_turn.complete {
                                            let spinner = cosmic::widget::text("Working…").size(12);
                                            let row = cosmic::widget::row::with_capacity(2)
                                                .push(spinner)
                                                .push(cosmic::widget::Space::with_width(Length::Fill));
                                            column = column.push(row);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    
                    // Add spacer at bottom to force scroll to bottom
                    column = column.push(
                        cosmic::widget::Space::with_height(Length::Fixed(1.0))
                            .width(Length::Fill)
                    );
                    
                    scrollable(column)
                        .scrollbar_width(8)
                        .scrollbar_padding(4)
                        .id(self.scrollable_id.clone())
                }
                .height(Length::Fill)
                .width(Length::Fill)
            )
            .push(
                // Spacing between messages and input area
                cosmic::widget::Space::with_height(Length::Fixed(16.0))
            )
            .push(
                // Input area with better styling
                cosmic::widget::container(
                    cosmic::widget::column::with_capacity(3)
                        .push(
                            // Attached files display
                            if !self.attached_files.is_empty() {
                                cosmic::widget::column::with_children(
                                    self.attached_files.iter().map(|file_path| {
                                        let file_name = std::path::Path::new(file_path)
                                            .file_name()
                                            .and_then(|name| name.to_str())
                                            .unwrap_or(file_path);
                                        
                                        cosmic::widget::row::with_children(vec![
                                            cosmic::widget::text(format!("📎 {}", file_name)).size(12).into(),
                                            cosmic::widget::Space::with_width(Length::Fill).into(),
                                            cosmic::widget::button::standard("✕")
                                                .on_press(Message::RemoveFile(file_path.clone()))
                                                .padding([4, 8])
                                                .into(),
                                        ])
                                        .spacing(8)
                                        .align_y(cosmic::iced::Alignment::Center)
                                        .into()
                                    }).collect()
                                )
                                .spacing(4)
                            } else {
                                cosmic::widget::column::with_children(vec![
                                    cosmic::widget::text("").size(12).into()
                                ])
                            }
                        )
                        .push(
                            // Text input for message
                            text_input("Type your message and press Enter to send...", &self.input)
                                .id(self.input_id.clone())
                                .on_input(Message::InputChanged)
                                .on_submit(|_| Message::SendMessage)
                                .width(Length::Fill)
                                .padding(12)
                        )
                        .push(
                            // Button row
                            cosmic::widget::row::with_capacity(6)
                                .push(
                                    // Send button
                                    widget::button::suggested("Send")
                                        .on_press(Message::SendMessage)
                                )
                                .push(
                                    // Attach file button
                                    widget::button::icon(widget::icon::from_name("document-attach-symbolic"))
                                        .on_press(Message::AttachFile)
                                )
                                .push(
                                    // Stop button (only visible when streaming)
                                    if self.is_streaming {
                                        widget::button::icon(widget::icon::from_name("process-stop-symbolic"))
                                            .class(widget::button::ButtonClass::Destructive)
                                            .on_press(Message::StopMessage)
                                    } else {
                                        widget::button::icon(widget::icon::from_name("process-stop-symbolic"))
                                            .class(widget::button::ButtonClass::Destructive)
                                    }
                                )
                                .push(
                                    // Retry button (only visible when there's a last message)
                                    if self.last_user_message.is_some() && !self.is_streaming {
                                        widget::button::icon(widget::icon::from_name("view-refresh-symbolic"))
                                            .on_press(Message::RetryMessage)
                                    } else {
                                        widget::button::icon(widget::icon::from_name("view-refresh-symbolic"))
                                    }
                                )
                                .push(
                                    cosmic::widget::Space::with_width(Length::Fill)
                                )
                                .spacing(8)
                                .align_y(cosmic::iced::Alignment::Center)
                        )
                        .spacing(8)
                )
                .padding(16)
                .width(Length::Fill)
                .class(cosmic::style::Container::Card)
            )
            .into()
    }

    fn combined_top_panel(&self) -> Element<Message> {
        use cosmic::iced::Length;
        
        // Count enabled/disabled tools
        let total_tools = self.available_mcp_tools.len();
        let enabled_count = self.available_mcp_tools.iter()
            .filter(|tool| self.tool_states.get(&tool.name).copied().unwrap_or(true))
            .count();
        
        // Conversation info
        let (title, created_text, msg_count) = if let Some(id) = self.current_conversation_id {
            if let Ok(Some(conv)) = self.storage.get_conversation(&id) {
                let created = conv.created_at.format("%Y-%m-%d %H:%M").to_string();
                // Prefer the latest title from the on-disk index (updated by background tasks)
                let index = self.storage.list_conversations_from_index().unwrap_or_else(|e| {
                    eprintln!("Failed to list conversations: {}", e);
                    Vec::new()
                });
                let latest_title = index
                    .into_iter()
                    .find(|ci| ci.id == id)
                    .map(|ci| ci.title)
                    .unwrap_or_else(|| conv.title.clone());
                (latest_title, Some(created), conv.messages.len())
            } else {
                ("New Chat".to_string(), None, self.messages.len())
            }
        } else {
            ("New Chat".to_string(), None, self.messages.len())
        };
        
        let created_label = created_text.unwrap_or_else(|| "".to_string());
        
        cosmic::widget::container(
            cosmic::widget::column::with_capacity(2)
                .push(
                    // Top row: conversation info and profile
                    cosmic::widget::row::with_capacity(5)
                        .push(
                            cosmic::widget::text(title)
                                .size(18)
                        )
                        .push(cosmic::widget::Space::with_width(Length::Fill))
                        .push(
                            // Profile selection dropdown
                            {
                                let mut names: Vec<String> = self.config.profiles.keys().cloned().collect();
                                names.sort();
                                let idx = names.iter().position(|k| k == &self.config.default);
                                widget::dropdown(names, idx, Message::ChangeDefaultProfile)
                            }
                        )
                        .push(
                            cosmic::widget::text(
                                if created_label.is_empty() { "".to_string() } else { format!("Created: {}", created_label) }
                            )
                                .size(12)
                                .class(cosmic::style::Text::Color(cosmic::iced::Color::from_rgb(0.4, 0.4, 0.4)))
                        )
                        .push(
                            cosmic::widget::text(format!("Messages: {}", msg_count))
                                .size(12)
                                .class(cosmic::style::Text::Color(cosmic::iced::Color::from_rgb(0.4, 0.4, 0.4)))
                        )
                        .push(
                            widget::button::suggested("New Chat")
                                .on_press(Message::NewConversation)
                        )
                        .spacing(12)
                        .align_y(cosmic::iced::Alignment::Center)
                )
                .push(
                    // Bottom row: tool controls
                    if total_tools == 0 {
                        // Show a message when no tools are configured
                        cosmic::widget::row::with_capacity(2)
                            .push(
                                cosmic::widget::text("🔧 No MCP tools configured")
                                    .size(12)
                                    .class(cosmic::style::Text::Color(cosmic::iced::Color::from_rgb(0.5, 0.5, 0.5)))
                            )
                            .push(cosmic::widget::horizontal_space())
                            .push(
                                cosmic::widget::button::text("Configure")
                                    .on_press(Message::ShowToolsContext)
                                    .padding(4)
                                    .class(cosmic::style::Button::Text)
                            )
                            .spacing(8)
                            .align_y(cosmic::iced::Alignment::Center)
                    } else {
                        // Tool controls
                        cosmic::widget::row::with_capacity(4)
                            .push(
                                cosmic::widget::text(format!("🔧 Tools: {} / {} enabled", enabled_count, total_tools))
                                    .size(12)
                            )
                            .push(cosmic::widget::horizontal_space())
                            .push(
                                cosmic::widget::row::with_capacity(3)
                                    .push(
                                        cosmic::widget::button::text("Enable All")
                                            .on_press(Message::ToggleAllTools(true))
                                            .padding(4)
                                            .class(cosmic::style::Button::Text)
                                    )
                                    .push(
                                        cosmic::widget::button::text("Disable All")
                                            .on_press(Message::ToggleAllTools(false))
                                            .padding(4)
                                            .class(cosmic::style::Button::Text)
                                    )
                                    .push(
                                        cosmic::widget::button::text("Configure")
                                            .on_press(Message::ShowToolsContext)
                                            .padding(4)
                                            .class(cosmic::style::Button::Text)
                                    )
                                    .spacing(8)
                            )
                            .spacing(8)
                            .align_y(cosmic::iced::Alignment::Center)
                    }
                )
                .spacing(8)
        )
        .padding(12)
        .class(cosmic::style::Container::Card)
        .into()
    }

    fn tool_controls_inline(&self) -> Element<Message> {
        
        // Count enabled/disabled tools
        let total_tools = self.available_mcp_tools.len();
        let enabled_count = self.available_mcp_tools.iter()
            .filter(|tool| self.tool_states.get(&tool.name).copied().unwrap_or(true))
            .count();
        
        if total_tools == 0 {
            // Show a message when no tools are configured
            return cosmic::widget::container(
                cosmic::widget::row::with_capacity(2)
                    .push(
                        cosmic::widget::text("🔧 No MCP tools configured")
                            .size(12)
                            .class(cosmic::style::Text::Color(cosmic::iced::Color::from_rgb(0.5, 0.5, 0.5)))
                    )
                    .push(cosmic::widget::horizontal_space())
                    .push(
                        cosmic::widget::button::text("Configure")
                            .on_press(Message::ShowToolsContext)
                            .padding(4)
                            .class(cosmic::style::Button::Text)
                    )
                    .spacing(8)
                    .align_y(cosmic::iced::Alignment::Center)
            )
            .padding(8)
            .class(cosmic::style::Container::Card)
            .into();
        }
        
        // Inline tool controls in top panel
        cosmic::widget::container(
            cosmic::widget::row::with_capacity(4)
                .push(
                    cosmic::widget::text(format!("🔧 Tools: {} / {} enabled", enabled_count, total_tools))
                        .size(12)
                )
                .push(cosmic::widget::horizontal_space())
                .push(
                    cosmic::widget::row::with_capacity(3)
                        .push(
                            cosmic::widget::button::text("Enable All")
                                .on_press(Message::ToggleAllTools(true))
                                .padding(4)
                                .class(cosmic::style::Button::Text)
                        )
                        .push(
                            cosmic::widget::button::text("Disable All")
                                .on_press(Message::ToggleAllTools(false))
                                .padding(4)
                                .class(cosmic::style::Button::Text)
                        )
                        .push(
                            cosmic::widget::button::text("Configure")
                                .on_press(Message::ShowToolsContext)
                                .padding(4)
                                .class(cosmic::style::Button::Text)
                        )
                        .spacing(8)
                )
                .spacing(8)
                .align_y(cosmic::iced::Alignment::Center)
        )
        .padding(8)
        .class(cosmic::style::Container::Card)
        .into()
    }

    fn tools_context_view(&self) -> Element<Message> {
        use cosmic::iced::Length;
        
        let total_tools = self.available_mcp_tools.len();
        let enabled_count = self.available_mcp_tools.iter()
            .filter(|tool| self.tool_states.get(&tool.name).copied().unwrap_or(true))
            .count();
        
        cosmic::widget::column::with_capacity(3)
            .push(
                // Header with summary and controls
                cosmic::widget::container(
                    cosmic::widget::column::with_capacity(2)
                        .push(
                            cosmic::widget::text(format!("🔧 Tools: {} / {} enabled", enabled_count, total_tools))
                                .size(16)
                        )
                        .push(
                            cosmic::widget::row::with_capacity(2)
                                .push(
                                    cosmic::widget::button::text("Enable All")
                                        .on_press(Message::ToggleAllTools(true))
                                        .padding(6)
                                        .class(cosmic::style::Button::Text)
                                )
                                .push(
                                    cosmic::widget::button::text("Disable All")
                                        .on_press(Message::ToggleAllTools(false))
                                        .padding(6)
                                        .class(cosmic::style::Button::Text)
                                )
                                .spacing(8)
                        )
                        .spacing(8)
                )
                .padding(16)
                .class(cosmic::style::Container::Card)
            )
            .push(
                // Tool list
                if self.available_mcp_tools.is_empty() {
                    Element::from(
                        cosmic::widget::container(
                            cosmic::widget::column::with_capacity(2)
                                .push(
                                    cosmic::widget::text("No tools available")
                                        .size(14)
                                )
                                .push(
                                    cosmic::widget::text("Configure MCP servers to see tools here")
                                        .size(12)
                                        .class(cosmic::style::Text::Color(cosmic::iced::Color::from_rgb(0.6, 0.6, 0.6)))
                                )
                                .spacing(4)
                        )
                        .padding(16)
                        .class(cosmic::style::Container::Card)
                    )
                } else {
                    let mut tool_list = cosmic::widget::column::with_capacity(self.available_mcp_tools.len())
                        .spacing(4);
                    
                    for tool in &self.available_mcp_tools {
                        let is_enabled = self.tool_states.get(&tool.name).copied().unwrap_or(true);
                        let tool_row = cosmic::widget::container(
                            cosmic::widget::column::with_capacity(3)
                                .push(
                                    cosmic::widget::row::with_capacity(2)
                                        .push(
                                            cosmic::widget::toggler(is_enabled)
                                                .on_toggle(|enabled| Message::ToggleTool(tool.name.clone(), enabled))
                                        )
                                        .push(
                                            cosmic::widget::text(&tool.name)
                                                .size(14)
                                                .class(if is_enabled {
                                                    cosmic::style::Text::Default
                                                } else {
                                                    cosmic::style::Text::Color(cosmic::iced::Color::from_rgb(0.5, 0.5, 0.5))
                                                })
                                        )
                                        .spacing(8)
                                        .align_y(cosmic::iced::Alignment::Center)
                                )
                                .push(
                                    cosmic::widget::text(&tool.description)
                                        .size(12)
                                        .class(cosmic::style::Text::Color(cosmic::iced::Color::from_rgb(0.6, 0.6, 0.6)))
                                )
                                .spacing(4)
                        )
                        .padding(12)
                        .class(cosmic::style::Container::Card);
                        
                        tool_list = tool_list.push(tool_row);
                    }
                    
                    cosmic::widget::scrollable(tool_list)
                        .height(Length::Fill)
                        .into()
                }
            )
            .spacing(8)
            .into()
    }

    fn history_view(&self) -> Element<Message> {
        let conversations = self.storage.list_conversations_from_index().unwrap_or_else(|e| {
            eprintln!("Failed to list conversations: {}", e);
            Vec::new()
        });
        
        cosmic::widget::column::with_capacity(2)
            .push(
                cosmic::widget::container(
                    cosmic::widget::text("Conversation History")
                        .size(20)
                )
                .padding(16)
            )
            .push(
                {
                    let mut column = cosmic::widget::column::with_capacity(conversations.len().max(1));
                    if conversations.is_empty() {
                        column = column.push(
                            cosmic::widget::text("No conversations yet. Start a new chat to create your first conversation!")
                                .size(14)
                        );
                    } else {
                        for conv in conversations {
                            let title = conv.title.clone();
                            let date_str = conv.updated_at.format("%Y-%m-%d %H:%M").to_string();
                            let button_text = format!("{} - {}", title, date_str);
                            let row = cosmic::widget::row::with_capacity(3)
                                .push(
                                    widget::button::text(button_text)
                                        .on_press(Message::SelectConversation(conv.id))
                                )
                                .push(cosmic::widget::Space::with_width(Length::Fill))
                                .push(
                                    widget::button::standard("🗑️")
                                        .on_press(Message::DeleteConversation(conv.id))
                                ).padding(16);
                            column = column.push(row);
                        }
                    }
                    scrollable(column)
                }
                .height(Length::Fill)
                .width(Length::Fill)
            )
            .into()
    }

    fn mcp_config_view(&self) -> Element<Message> {
        // Load the actual MCP config (same as startup)
        let mcp_config = crate::config::MCPConfig::load_from_json()
            .unwrap_or_else(|_| self.config.mcp.clone());
        
        let server_count = mcp_config.servers.len();
        let server_count_text = format!("Configured MCP Servers ({})", server_count);
        
        // Get available tools from MCP registry
        let tools = &self.available_mcp_tools;
        let tool_count_text = format!("Available Tools ({})", tools.len());
        
        // Build server list with owned data
        let mut server_column = cosmic::widget::column::with_capacity(mcp_config.servers.len());
        for (server_name, server_config) in mcp_config.servers {
            let command_text = format!("Command: {} {}", 
                server_config.command,
                server_config.args.join(" ")
            );
            
            let server_widget = cosmic::widget::container(
                cosmic::widget::column::with_capacity(3)
                    .push(
                        cosmic::widget::text(server_name)
                            .size(14)
                    )
                    .push(
                        cosmic::widget::text("Type: stdio")
                            .size(12)
                    )
                    .push(
                        cosmic::widget::text(command_text)
                            .size(10)
                    )
            )
            .padding(8)
            .class(cosmic::style::Container::Card);
            
            server_column = server_column.push(server_widget);
        }
        
        cosmic::widget::column::with_capacity(4)
            .push(
                cosmic::widget::container(
                    cosmic::widget::text("MCP Configuration")
                        .size(20)
                )
                .padding(16)
            )
            .push(
                cosmic::widget::container(
                    cosmic::widget::text(server_count_text)
                        .size(16)
                )
                .padding(16)
            )
            .push(
                scrollable(server_column)
                    .height(Length::FillPortion(2))
                    .width(Length::Fill)
            )
            .push(
                cosmic::widget::container(
                    cosmic::widget::row::with_capacity(2)
                        .push(
                            cosmic::widget::text(tool_count_text)
                                .size(16)
                        )
                        .push(cosmic::widget::horizontal_space())
                        .push(
                            cosmic::widget::button::icon(
                                cosmic::widget::icon::from_name("view-refresh-symbolic")
                            )
                            .on_press(Message::RefreshMCPTools)
                            .padding(4)
                        )
                        .spacing(8)
                        .align_y(cosmic::iced::Alignment::Center)
                )
                .padding(16)
            )
            .push(
                {
                    let mut column = cosmic::widget::column::with_capacity(tools.len());
                    if tools.is_empty() {
                        column = column.push(
                            cosmic::widget::container(
                                cosmic::widget::column::with_capacity(2)
                                    .push(
                                        cosmic::widget::text("No tools discovered yet")
                                            .size(14)
                                    )
                                    .push(
                                        cosmic::widget::text("Tools will appear here once MCP servers are connected")
                                            .size(12)
                                            .class(cosmic::style::Text::Color(cosmic::iced::Color::from_rgb(0.6, 0.6, 0.6)))
                                    )
                                    .spacing(4)
                            )
                            .padding(16)
                            .class(cosmic::style::Container::Card)
                        );
                    } else {
                        for tool in tools.iter() {
                            // Build input schema text
                            let input_text = if let Some(properties) = tool.parameters.get("properties") {
                                if let Some(props_obj) = properties.as_object() {
                                    let params: Vec<String> = props_obj.keys()
                                        .map(|k| k.to_string())
                                        .collect();
                                    if params.is_empty() {
                                        "No parameters".to_string()
                                    } else {
                                        format!("Parameters: {}", params.join(", "))
                                    }
                                } else {
                                    "Parameters: (schema)".to_string()
                                }
                            } else {
                                "No parameters defined".to_string()
                            };

                            column = column.push(
                                cosmic::widget::container(
                                    cosmic::widget::column::with_capacity(3)
                                        .push(
                                            cosmic::widget::text(&tool.name)
                                                .size(14)
                                                .font(cosmic::font::Font::MONOSPACE)
                                        )
                                        .push(
                                            cosmic::widget::text(&tool.description)
                                                .size(12)
                                        )
                                        .push(
                                            cosmic::widget::text(input_text)
                                                .size(10)
                                                .class(cosmic::style::Text::Color(cosmic::iced::Color::from_rgb(0.5, 0.5, 0.5)))
                                        )
                                        .spacing(4)
                                )
                                .padding(12)
                                .class(cosmic::style::Container::Card)
                            );
                        }
                    }
                    scrollable(column)
                }
                .height(Length::FillPortion(2))
                .width(Length::Fill)
            )
            .into()
    }

    fn settings_view(&self) -> Element<Message> {
        let current_profile = self.config.default.clone();
        
        cosmic::widget::column::with_capacity(6)
            .push(
                cosmic::widget::container(
                    cosmic::widget::text("Settings")
                        .size(24)
                )
                .padding(16)
            )
            .push(
                // LLM Profile Selection
                cosmic::widget::container(
                    cosmic::widget::column::with_capacity(4)
                        .push(
                            cosmic::widget::text("Default LLM Profile")
                                .size(18)
                        )
                        .push(
                            cosmic::widget::text("Select the default LLM profile to use for new conversations")
                                .size(14)
                        )
                        .push(
                            cosmic::widget::text(format!("Current: {}", current_profile))
                                .size(16)
                        )
                        .push(
                            cosmic::widget::text("Available profiles:")
                                .size(14)
                        )
                )
                .padding(16)
                .class(cosmic::style::Container::Card)
            )
            .push(
                // Profile List
                cosmic::widget::container(
                    {
                        let mut column = cosmic::widget::column::with_capacity(self.config.profiles.len());
                        for (name, profile) in &self.config.profiles {
                            let is_current = name == &current_profile;
                            let status_text = if is_current { "✓ Current" } else { "Click to select" };
                            column = column.push(
                                cosmic::widget::container(
                                    cosmic::widget::column::with_capacity(2)
                                        .push(
                                            cosmic::widget::text(format!("• {}: {} ({})", name, profile.model, profile.endpoint))
                                                .size(12)
                                        )
                                        .push(
                                            cosmic::widget::text(status_text)
                                                .size(10)
                                        )
                                )
                                .padding(8)
                                .class(cosmic::style::Container::Card)
                            );
                        }
                        column
                    }
                )
                .padding(16)
                .class(cosmic::style::Container::Card)
            )
            .push(
                // Profile Details
                cosmic::widget::container(
                    {
                        if let Some(profile) = self.config.profiles.get(&current_profile) {
                            cosmic::widget::column::with_capacity(3)
                                .push(
                                    cosmic::widget::text(format!("Profile: {}", current_profile))
                                        .size(16)
                                )
                                .push(
                                    cosmic::widget::text(format!("Model: {}", profile.model))
                                        .size(14)
                                )
                                .push(
                                    cosmic::widget::text(format!("Endpoint: {}", profile.endpoint))
                                        .size(14)
                                )
                        } else {
                            cosmic::widget::column::with_capacity(1)
                                .push(
                                    cosmic::widget::text("No profile selected")
                                        .size(14)
                                )
                        }
                    }
                )
                .padding(16)
                .class(cosmic::style::Container::Card)
            )
            .push(
                // MCP Servers Section
                cosmic::widget::container(
                    cosmic::widget::column::with_capacity(2)
                        .push(
                            cosmic::widget::text("MCP Servers")
                                .size(18)
                        )
                        .push(
                            cosmic::widget::text(format!("{} servers configured", self.config.mcp.servers.len()))
                                .size(14)
                        )
                )
                .padding(16)
                .class(cosmic::style::Container::Card)
            )
            .push(
                // MCP Server List
                cosmic::widget::container(
                    {
                        let mut column = cosmic::widget::column::with_capacity(self.config.mcp.servers.len());
                        for (server_name, server_config) in &self.config.mcp.servers {
                            column = column.push(
                                cosmic::widget::container(
                                    cosmic::widget::column::with_capacity(2)
                                        .push(
                                            cosmic::widget::text(server_name)
                                                .size(14)
                                        )
                                        .push(
                                            cosmic::widget::text(format!("Type: stdio | Command: {}", 
                                                server_config.command
                                            ))
                                                .size(12)
                                        )
                                )
                                .padding(8)
                                .class(cosmic::style::Container::Card)
                            );
                        }
                        if self.config.mcp.servers.is_empty() {
                            column = column.push(
                                cosmic::widget::text("No MCP servers configured")
                                    .size(14)
                            );
                        }
                        scrollable(column)
                    }
                )
                .padding(16)
                .class(cosmic::style::Container::Card)
            )
            .push(
                // Action Buttons
                cosmic::widget::container(
                    cosmic::widget::row::with_capacity(3)
                        .push(
                            widget::button::suggested("Save Settings")
                                .on_press(Message::SaveSettings)
                        )
                        .push(
                            widget::button::standard("Reset to Defaults")
                                .on_press(Message::ResetSettings)
                        )
                        .push(
                            if self.settings_changed {
                                cosmic::widget::text("⚠️ Unsaved changes")
                                    .size(12)
                            } else {
                                cosmic::widget::text("✓ All changes saved")
                                    .size(12)
                            }
                        )
                )
                .padding(16)
                .class(cosmic::style::Container::Card)
            )
            .into()
    }
}

