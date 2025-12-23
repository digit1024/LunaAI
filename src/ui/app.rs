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
    pub expanded_tool_summaries: std::collections::HashSet<(usize, String)>,
    pub expanded_reasoning: std::collections::HashSet<usize>, // message indices with expanded reasoning
    pub expanded_summaries: std::collections::HashSet<usize>, // message indices with expanded summaries
    pub expanded_mcp_servers: std::collections::HashSet<String>,
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
    pub dialog_text_content: Option<text_editor::Content>,
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
    // Typing indicator animation state
    pub typing_indicator_progress: f32,
    // Cache for context usage percentage per conversation (to avoid blocking UI)
    pub context_usage_cache: std::collections::HashMap<Uuid, Option<u32>>,
    pub typing_indicator_start_time: Option<cosmic::iced::time::Instant>,
    // Recent conversations for nav bar (last 10)
    pub recent_conversations: Vec<(Uuid, String)>, // (id, title)
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
        crate::ui::icons::ICON_CACHE
            .set(Mutex::new(crate::ui::icons::IconCache::new()))
            .unwrap();

        let mut settings_page = SimpleSettingsPage::new();
        settings_page.load_from_config(&config);
        
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
            expanded_tool_summaries: std::collections::HashSet::new(),
            expanded_reasoning: std::collections::HashSet::new(),
            expanded_summaries: std::collections::HashSet::new(),
            expanded_mcp_servers: std::collections::HashSet::new(),
            scrollable_id: cosmic::widget::Id::unique(),
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
            pending_tool_calls_for_history: Vec::new(),
            tool_runtime_context: std::collections::HashMap::new(),
            show_tools_context: false,
            last_user_message: None,
            attached_files: Vec::new(),
            current_error: None,
            pending_llm_messages: None,
            search_query: String::new(),
            search_results: Vec::new(),
            typing_indicator_progress: 0.0,
            typing_indicator_start_time: None,
            recent_conversations: Vec::new(),
            context_usage_cache: std::collections::HashMap::new(),
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
        let messages = self.messages.clone();
        let mcp_registry = self.mcp_registry.clone();
        let pending_messages = self.pending_llm_messages.clone();
        let profile = self.config.get_default_profile().cloned();
        let profile_prompt_path = self.config.get_default_profile().and_then(|profile| {
            profile
                .profile_prompt_file
                .as_ref()
                .map(|path| crate::config::AppConfig::resolve_config_path(path))
        });

        Subscription::run_with_id(
            id,
            stream::channel(100, move |mut output| async move {
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
                            system_prompt.to_string(),
                        ));
                    }

                    // Add profile prompt if configured
                    if let Some(profile_prompt_path) = profile_prompt_path.clone() {
                        let resolved = profile_prompt_path.to_string_lossy().to_string();
                        match prompt_manager.load_profile_prompt(&resolved) {
                            Ok(prompt) => {
                                llm_messages.push(crate::llm::Message::new(
                                    crate::llm::Role::System,
                                    prompt,
                                ));
                            }
                            Err(err) => {
                                let message = match &err {
                                    ProfilePromptError::NotFound(_) => {
                                        format!("Profile prompt not found: {}", err.path())
                                    }
                                    _ => err.to_string(),
                                };
                                let _ = output.send(Message::InlineError(message)).await;
                            }
                        }
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

                // === CONTEXT MANAGEMENT ===
                // Apply token counting and truncation to prevent API context overflow
                let final_messages = if let Some(ref prof) = profile {
                    use crate::llm::tokenizer::TokenCounter;
                    use crate::llm::context_manager::SmartContextManager;
                    
                    let token_counter = TokenCounter::new(prof);
                    let context_limit = token_counter.get_context_limit(prof);
                    let safe_limit = token_counter.get_safe_context_limit(prof);
                    
                    let total_tokens: usize = llm_messages.iter()
                        .map(|msg| token_counter.count_message_tokens(msg))
                        .sum();
                    
                    println!("📊 Desktop context: {} tokens / {} limit (safe: {})", 
                        total_tokens, context_limit, safe_limit);
                    
                    if total_tokens > safe_limit {
                        println!("⚠️ Context exceeds safe limit, applying smart truncation...");
                        let _ = output.send(Message::InlineError(format!(
                            "Context size ({} tokens) exceeds safe limit ({}). Applying smart truncation.",
                            total_tokens, safe_limit
                        ))).await;
                        
                        // Apply smart context selection
                        let truncated = SmartContextManager::select_context(
                            llm_messages,
                            &token_counter,
                            prof,
                        );
                        
                        let new_tokens: usize = truncated.iter()
                            .map(|msg| token_counter.count_message_tokens(msg))
                            .sum();
                        println!("✂️ Truncated to {} tokens ({} messages)", new_tokens, truncated.len());
                        
                        truncated
                    } else {
                        llm_messages
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
        self.expanded_tool_summaries.clear();
        self.expanded_reasoning.clear();
        self.expanded_summaries.clear();

        let mut archived_indices: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();

        for stored in conversation.messages {
            // Tool role messages should NOT be added as regular chat messages -
            // they only update the archived tool calls with results
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

                            // Prefer tool_result_json over content (content is legacy/redundant)
                            let result_text = stored
                                .tool_result_json
                                .as_ref()
                                .map(|value| {
                                    serde_json::to_string_pretty(value)
                                        .unwrap_or_else(|_| value.to_string())
                                })
                                .unwrap_or_else(|| stored.content.clone());
                            if entry.tool_call.status == ToolCallStatus::Error {
                                entry.tool_call.error = Some(result_text);
                            } else {
                                entry.tool_call.result = Some(result_text);
                            }
                        }
                    }
                }
                continue; // Skip adding tool messages to self.messages
            }

            let is_user = stored.role == "user";
            self.messages.push(ChatMessage {
                content: stored.content.clone(),
                is_user,
                is_error: false,
                reasoning_content: stored.reasoning_content.clone(),
                is_summary: stored.is_summary,
                is_summarized: stored.is_summarized,
                summarized_count: stored.summarized_count,
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
                    self.archived_tool_calls.push(AnchoredToolCall {
                        anchor_index,
                        tool_call: info,
                    });
                    archived_indices.insert(call.id.clone(), self.archived_tool_calls.len() - 1);
                }
            }
        }
        
        // Update context usage cache for this conversation
        self.update_context_usage_cache(conversation.id);
    }

    /// Update the context usage cache for a conversation
    /// This is called when conversations are loaded/changed to avoid blocking UI during rendering
    fn update_context_usage_cache(&mut self, conversation_id: Uuid) {
        if let Ok(Some(conv)) = self.storage.get_conversation(&conversation_id) {
            let usage_pct = crate::ui::pages::chat::top_panel::calculate_context_usage(
                &conv,
                &self.config,
                &self.prompt_manager,
            );
            self.context_usage_cache.insert(conversation_id, usage_pct);
        }
    }

    /// Perform manual summarization on the current conversation
    fn perform_manual_summarization(&mut self, conv_id: Uuid) {
        println!("📝 Manual summarization triggered for conversation {}", conv_id);
        
        if let Some(profile) = self.config.get_default_profile() {
            // Load messages from DB for summarization
            if let Ok(db_messages) = self.storage.load_conversation_messages(&conv_id.to_string()) {
                // Filter to regular messages (exclude summaries, tools, and already summarized messages)
                let regular_messages: Vec<_> = db_messages.iter()
                    .filter(|msg| !msg.is_summary && !msg.is_summarized && msg.role != "tool")
                    .collect();
                
                if regular_messages.is_empty() {
                    println!("⚠️ No messages available to summarize");
                    return;
                }
                
                let keep_recent_count = 10;
                let messages_to_summarize_count = regular_messages.len().saturating_sub(keep_recent_count);
                
                if messages_to_summarize_count == 0 {
                    println!("ℹ️ All messages are recent (keeping last {}), nothing to summarize", keep_recent_count);
                    return;
                }
                
                println!("📝 Will summarize {} messages (keeping last {})", 
                    messages_to_summarize_count, keep_recent_count);
                
                // Get IDs to summarize
                let ids_to_summarize: Vec<i64> = regular_messages[..messages_to_summarize_count]
                    .iter()
                    .map(|msg| msg.id)
                    .collect();
                
                // Get full messages to summarize
                let msgs_to_summarize: Vec<_> = db_messages.iter()
                    .filter(|msg| ids_to_summarize.contains(&msg.id))
                    .cloned()
                    .collect();
                
                // Convert to LlmMessage for summarization
                let llm_msgs_to_summarize: Vec<crate::llm::Message> = msgs_to_summarize.iter()
                    .filter_map(|msg| {
                        let role = match msg.role.as_str() {
                            "user" => crate::llm::Role::User,
                            "assistant" => crate::llm::Role::Assistant,
                            "system" => crate::llm::Role::System,
                            _ => return None,
                        };
                        Some(crate::llm::Message::new(role, msg.content.clone()))
                    })
                    .collect();
                
                if !llm_msgs_to_summarize.is_empty() {
                    // Generate summary synchronously (blocking but necessary for desktop)
                    println!("🤖 Generating summary...");
                    let llm_client = self.llm_client.clone();
                    let profile_clone = profile.clone();
                    
                    // Use tokio runtime for async summarization
                    let summary_result = tokio::task::block_in_place(|| {
                        tokio::runtime::Handle::current().block_on(async {
                            crate::llm::context_manager::SmartContextManager::summarize_messages(
                                llm_msgs_to_summarize,
                                &profile_clone,
                                llm_client.as_ref(),
                            ).await
                        })
                    });
                    
                    match summary_result {
                        Ok(summary_msg) => {
                            println!("✅ Summary generated: {} chars", summary_msg.content.len());
                            
                            // Perform database summarization
                            if let Err(e) = self.storage.perform_summarization(
                                &conv_id.to_string(),
                                &msgs_to_summarize,
                                &summary_msg.content,
                            ) {
                                eprintln!("❌ Failed to save summary to DB: {}", e);
                            } else {
                                println!("💾 Summary saved to database");
                                
                                // Rebuild UI messages from DB to show the summary
                                if let Ok(Some(conv)) = self.storage.get_conversation(&conv_id) {
                                    self.rebuild_conversation_view(conv);
                                    // Update context usage cache
                                    self.update_context_usage_cache(conv_id);
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("❌ Summarization failed: {}", e);
                        }
                    }
                }
            } else {
                eprintln!("❌ Failed to load messages from database");
            }
        } else {
            eprintln!("❌ No profile configured for summarization");
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
            let masked = if p.api_key.len() > 6 {
                format!(
                    "{}...{}",
                    &p.api_key[..3],
                    &p.api_key[p.api_key.len().saturating_sub(3)..]
                )
            } else {
                "***".to_string()
            };
            println!(
                "🔧 Default profile details → model='{}' endpoint='{}' api_key='{}'",
                p.model, p.endpoint, masked
            );
        } else {
            println!("❗ No default profile found; using fallback defaults");
        }
        let initial_profile_mcp_servers = config
            .get_default_profile()
            .map(|profile| profile.enabled_mcp.clone())
            .unwrap_or_default();
        let sqlite_settings = SqliteSettings::from(&config.server);
        let storage =
            Storage::new_default_with_settings(sqlite_settings.clone()).unwrap_or_else(|e| {
                eprintln!("Failed to initialize SQLite storage: {}", e);
                // Fallback to a temporary database
                Storage::new_with_settings(
                    std::env::temp_dir().join("cosmic_llm_temp.db"),
                    sqlite_settings,
                )
                .expect("Failed to create temporary database")
            });

        // Initialize prompt manager
        let prompt_manager = crate::prompts::PromptManager::load_from_config(&config.prompts)
            .unwrap_or_else(|e| {
                eprintln!("Failed to load prompts: {}", e);
                crate::prompts::PromptManager::load_from_config(
                    &crate::prompts::PromptConfig::default(),
                )
                .unwrap()
            });

        // Initialize MCP registry (non-blocking)
        let mcp_registry = Arc::new(RwLock::new(MCPServerRegistry::new()));
        let mcp_registry_clone = mcp_registry.clone();

        // Try to load MCP config from JSON file (new Claude Desktop format)
        // Falls back to embedded TOML format if JSON doesn't exist
        let mcp_config = crate::config::MCPConfig::load_from_json().unwrap_or_else(|e| {
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
            } else {
                // Always apply profile defaults, even if empty (empty = enable all)
                registry.apply_profile_tool_defaults(&initial_profile_mcp_servers);
            }
        });

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
        // Note: We'll handle this in the main thread instead of async task
        // since Storage is not cloneable
        println!("🔍 Checking for conversations with 'Generating title...'");
        let conversations = app.storage.list_conversations().unwrap_or_else(|e| {
            eprintln!("Failed to list conversations: {}", e);
            Vec::new()
        });
        let conversation_ids: Vec<_> = conversations
            .into_iter()
            .filter(|conv| conv.title == "Generating title...")
            .map(|conv| conv.id)
            .collect();

        for conv_id in conversation_ids {
            println!(
                "🔄 Found conversation {} with 'Generating title...', retrying...",
                conv_id
            );

            // Get the first user message to generate title from
            if let Ok(Some(conversation)) = app.storage.get_conversation(&conv_id) {
                if let Some(first_user_msg) =
                    conversation.messages.iter().find(|msg| msg.role == "user")
                {
                    let message_text = &first_user_msg.content;
                    println!("📝 Retrying title generation for: '{}'", message_text);

                    // Create a simple title based on first few words
                    let fallback_title = if message_text.len() > 50 {
                        format!("{}...", &message_text[..47])
                    } else {
                        message_text.clone()
                    };

                    if let Err(e) = app
                        .storage
                        .update_conversation_title(&conv_id, fallback_title.clone())
                    {
                        eprintln!("Failed to update conversation title: {}", e);
                    }
                    println!("💾 Updated title to: {}", fallback_title);
                }
            }
        }
        println!("✅ Finished checking for conversations with 'Generating title...'");


        // Load recent conversations and update nav model
        app.load_recent_conversations();
        app.update_nav_model();

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

        let mut tasks = vec![load_tools_task];
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
        
        Subscription::batch(vec![streaming_sub, animation_sub, conversation_refresh_sub])
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
                println!(
                    "🔍 DEBUG: SendMessage received. Input: '{}', Attachments: {}",
                    self.input,
                    self.attached_files.len()
                );
                // Allow sending if there's text OR if there are attachments
                if !self.input.trim().is_empty() || !self.attached_files.is_empty() {
                    // Create new conversation if none exists
                    if self.current_conversation_id.is_none() {
                        let current_profile_name = Some(self.config.default.as_str());
                        let conv_id = self
                            .storage
                            .create_conversation_with_profile("Generating title...".to_string(), current_profile_name)
                            .unwrap_or_else(|e| {
                                eprintln!("Failed to create conversation: {}", e);
                                Uuid::new_v4()
                            });
                        self.current_conversation_id = Some(conv_id);
                        // Update nav model to reflect new conversation
                        self.load_recent_conversations();
                        self.update_nav_model();

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
                        if let Err(e) = self
                            .storage
                            .update_conversation_title(&conv_id, fallback_title.clone())
                        {
                            eprintln!("Failed to update conversation title: {}", e);
                        }
                        println!(
                            "💾 Saved title to storage for conversation {}: {}",
                            conv_id, fallback_title
                        );
                    }

                    // Create user message content
                    let message_content = self.input.clone();

                    // Add user message
                    let user_msg = ChatMessage {
                        content: message_content,
                        is_user: true,
                        is_error: false,
                        reasoning_content: None,
                        is_summary: false,
            is_summarized: false,
                        summarized_count: None,
                    };
                    self.messages.push(user_msg.clone());

                    // Play sent sound
                    crate::ui::audio::AudioService::play_sound("sent.mp3");
                    
                    // Trigger scroll to bottom for new user message
                    // The scroll will be handled by anchor_bottom() and widget operations

                    // Add to storage
                    if let Some(conv_id) = self.current_conversation_id {
                        if let Err(e) = self.storage.add_message_to_conversation(
                            &conv_id,
                            "user".to_string(),
                            self.input.clone(),
                        ) {
                            eprintln!("Failed to add message to conversation: {}", e);
                        } else {
                            // Update context usage cache after adding message
                            self.update_context_usage_cache(conv_id);
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
                    println!(
                        "🔍 DEBUG: Processing {} attached files: {:?}",
                        self.attached_files.len(),
                        self.attached_files
                    );
                    for file_path in &self.attached_files {
                        println!("🔍 DEBUG: Processing file: {}", file_path);
                        match crate::llm::file_utils::create_attachment(file_path) {
                            Ok(attachment) => {
                                println!("🔍 DEBUG: Created attachment: {:?}", attachment);
                                // Validate file for LLM
                                if let Err(e) =
                                    crate::llm::file_utils::validate_file_for_llm(&attachment)
                                {
                                    println!("❌ DEBUG: File validation failed: {}", e);
                                    self.current_error = Some(format!(
                                        "File validation error for {}: {}",
                                        file_path, e
                                    ));
                                    return app::Task::none();
                                }
                                println!("✅ DEBUG: File validation passed");
                                attachments.push(attachment);
                            }
                            Err(e) => {
                                println!("❌ DEBUG: Failed to create attachment: {}", e);
                                self.current_error =
                                    Some(format!("Failed to process file {}: {}", file_path, e));
                                return app::Task::none();
                            }
                        }
                    }
                    println!("🔍 DEBUG: Final attachments count: {}", attachments.len());

                    // Convert messages to LLM format
                    let profile_prompt = self.load_active_profile_prompt();
                    let mut llm_messages = Vec::new();

                    // Add system prompt if available
                    if let Some(system_prompt) = self.prompt_manager.get_system_prompt() {
                        llm_messages.push(crate::llm::Message::new(
                            crate::llm::Role::System,
                            system_prompt.to_string(),
                        ));
                    }

                    if let Some(ref profile_prompt) = profile_prompt {
                        llm_messages.push(crate::llm::Message::new(
                            crate::llm::Role::System,
                            profile_prompt.clone(),
                        ));
                    }

                    // Load messages from database to get full tool_result_json data
                    if let Some(conv_id) = self.current_conversation_id {
                        match self.storage.load_conversation_messages(&conv_id.to_string()) {
                            Ok(db_messages) => {
                                // First pass: collect all valid tool_call_ids from assistant messages
                                let mut valid_tool_call_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
                                for msg in &db_messages {
                                    if msg.role == "assistant" {
                                        if let Some(ref tool_calls) = msg.tool_calls {
                                            for tc in tool_calls {
                                                valid_tool_call_ids.insert(tc.id.clone());
                                            }
                                        }
                                    }
                                }
                                
                                // Second pass: build messages, skipping orphaned tool results and summarized messages
                                let mut skipped_orphans = 0;
                                let mut skipped_summarized = 0;
                                for msg in db_messages {
                                    // Skip messages that have been summarized (but keep summary messages themselves)
                                    if msg.is_summarized && !msg.is_summary {
                                        skipped_summarized += 1;
                                        continue;
                                    }
                                    
                                    let role = match msg.role.as_str() {
                                        "user" => crate::llm::Role::User,
                                        "assistant" => crate::llm::Role::Assistant,
                                        "system" => crate::llm::Role::System,
                                        "tool" => {
                                            // Check if this tool result has a matching tool_call
                                            if let Some(ref tool_call_id) = msg.tool_call_id {
                                                if !valid_tool_call_ids.contains(tool_call_id) {
                                                    skipped_orphans += 1;
                                                    continue; // Skip orphaned tool result
                                                }
                                            } else {
                                                skipped_orphans += 1;
                                                continue; // No tool_call_id, skip
                                            }
                                            crate::llm::Role::Tool
                                        }
                                        _ => continue,
                                    };
                                    
                                    // For tool messages, combine content with tool_result_json
                                    let content = if role == crate::llm::Role::Tool {
                                        let mut combined = msg.content.clone();
                                        if let Some(ref result_json) = msg.tool_result_json {
                                            if !combined.is_empty() {
                                                combined.push_str("\n");
                                            }
                                            combined.push_str(&result_json.to_string());
                                        }
                                        combined
                                    } else {
                                        msg.content.clone()
                                    };
                                    
                                    let mut llm_msg = crate::llm::Message::new(role.clone(), content);
                                    
                                    // Preserve tool call metadata
                                    if role == crate::llm::Role::Tool {
                                        llm_msg.tool_call_id = msg.tool_call_id.clone();
                                    }
                                    if let Some(ref tool_calls) = msg.tool_calls {
                                        llm_msg.tool_calls = Some(tool_calls.iter().map(|tc| {
                                            crate::llm::ToolCall {
                                                id: tc.id.clone(),
                                                name: tc.name.clone(),
                                                parameters: tc.parameters.clone(),
                                            }
                                        }).collect());
                                    }
                                    llm_msg.reasoning_content = msg.reasoning_content.clone();
                                    
                                    llm_messages.push(llm_msg);
                                }
                                
                                if skipped_orphans > 0 {
                                    println!("⚠️ Skipped {} orphaned tool results (no matching tool_call)", skipped_orphans);
                                }
                                if skipped_summarized > 0 {
                                    println!("📄 Skipped {} summarized messages (using summaries instead)", skipped_summarized);
                                }
                            }
                            Err(e) => {
                                eprintln!("Failed to load messages from DB, falling back to UI messages: {}", e);
                                // Fallback to UI messages
                                for msg in &self.messages {
                                    let role = if msg.is_user {
                                        crate::llm::Role::User
                                    } else {
                                        crate::llm::Role::Assistant
                                    };
                                    llm_messages.push(crate::llm::Message::new(role, msg.content.clone()));
                                }
                            }
                        }
                    } else {
                        // No conversation yet, use UI messages
                        for msg in &self.messages {
                            let role = if msg.is_user {
                                crate::llm::Role::User
                            } else {
                                crate::llm::Role::Assistant
                            };
                            llm_messages.push(crate::llm::Message::new(role, msg.content.clone()));
                        }
                    }

                    // Create the current user message with attachments
                    let current_user_message = if attachments.is_empty() {
                        crate::llm::Message::new(crate::llm::Role::User, input_text.clone())
                    } else {
                        crate::llm::Message::new_with_attachments(
                            crate::llm::Role::User,
                            input_text.clone(),
                            attachments,
                        )
                    };

                    // Debug: Print the final message that will be sent to LLM
                    println!(
                        "🔍 DEBUG: Final LLM message with attachments: {:?}",
                        current_user_message
                    );

                    llm_messages.push(current_user_message.clone());

                    // Clear attached files after processing
                    self.attached_files.clear();

                    // === DESKTOP CONTEXT MANAGEMENT ===
                    // Check token count and trigger summarization if needed
                    if let Some(profile) = self.config.get_default_profile() {
                        use crate::llm::tokenizer::TokenCounter;
                        
                        let token_counter = TokenCounter::new(profile);
                        let total_tokens: usize = llm_messages.iter()
                            .map(|msg| token_counter.count_message_tokens(msg))
                            .sum();
                        
                        let context_limit = token_counter.get_context_limit(profile);
                        let summarize_threshold_tokens = token_counter.get_summarize_threshold_tokens(profile);
                        let safe_limit = token_counter.get_safe_context_limit(profile);
                        
                        let percentage = (total_tokens as f32 / context_limit as f32) * 100.0;
                        println!("📊 Desktop context: {} tokens ({:.1}% of {} limit)", 
                            total_tokens, percentage, context_limit);
                        println!("   Summarize threshold: {} tokens, Safe limit: {} tokens", 
                            summarize_threshold_tokens, safe_limit);
                        
                        // Check if summarization is needed
                        if total_tokens > summarize_threshold_tokens {
                            println!("🔄 Summarization threshold exceeded! ({} > {})", 
                                total_tokens, summarize_threshold_tokens);
                            
                            if let Some(conv_id) = self.current_conversation_id {
                                // Load messages from DB for summarization
                                if let Ok(db_messages) = self.storage.load_conversation_messages(&conv_id.to_string()) {
                                    // Filter to regular messages (exclude summaries, tools, and already summarized messages)
                                    let regular_messages: Vec<_> = db_messages.iter()
                                        .filter(|msg| !msg.is_summary && !msg.is_summarized && msg.role != "tool")
                                        .collect();
                                    
                                    let keep_recent_count = 10;
                                    let messages_to_summarize_count = regular_messages.len().saturating_sub(keep_recent_count);
                                    
                                    if messages_to_summarize_count > 0 {
                                        println!("📝 Will summarize {} messages (keeping last {})", 
                                            messages_to_summarize_count, keep_recent_count);
                                        
                                        // Get IDs to summarize
                                        let ids_to_summarize: Vec<i64> = regular_messages[..messages_to_summarize_count]
                                            .iter()
                                            .map(|msg| msg.id)
                                            .collect();
                                        
                                        // Get full messages to summarize
                                        let msgs_to_summarize: Vec<_> = db_messages.iter()
                                            .filter(|msg| ids_to_summarize.contains(&msg.id))
                                            .cloned()
                                            .collect();
                                        
                                        // Convert to LlmMessage for summarization
                                        let llm_msgs_to_summarize: Vec<crate::llm::Message> = msgs_to_summarize.iter()
                                            .filter_map(|msg| {
                                                let role = match msg.role.as_str() {
                                                    "user" => crate::llm::Role::User,
                                                    "assistant" => crate::llm::Role::Assistant,
                                                    "system" => crate::llm::Role::System,
                                                    _ => return None,
                                                };
                                                Some(crate::llm::Message::new(role, msg.content.clone()))
                                            })
                                            .collect();
                                        
                                        if !llm_msgs_to_summarize.is_empty() {
                                            // Generate summary synchronously (blocking but necessary for desktop)
                                            println!("🤖 Generating summary...");
                                            let llm_client = self.llm_client.clone();
                                            let profile_clone = profile.clone();
                                            
                                            // Use tokio runtime for async summarization
                                            let summary_result = tokio::task::block_in_place(|| {
                                                tokio::runtime::Handle::current().block_on(async {
                                                    crate::llm::context_manager::SmartContextManager::summarize_messages(
                                                        llm_msgs_to_summarize,
                                                        &profile_clone,
                                                        llm_client.as_ref(),
                                                    ).await
                                                })
                                            });
                                            
                                            match summary_result {
                                                Ok(summary_msg) => {
                                                    println!("✅ Summary generated: {} chars", summary_msg.content.len());
                                                    
                                                    // Perform database summarization
                                                    if let Err(e) = self.storage.perform_summarization(
                                                        &conv_id.to_string(),
                                                        &msgs_to_summarize,
                                                        &summary_msg.content,
                                                    ) {
                                                        eprintln!("❌ Failed to save summary to DB: {}", e);
                                                    } else {
                                                        println!("💾 Summary saved to database");
                                                        
                                                        // Rebuild llm_messages from the updated database
                                                        if let Ok(updated_msgs) = self.storage.load_conversation_messages(&conv_id.to_string()) {
                                                            llm_messages.clear();
                                                            
                                                            // Re-add system prompts
                                                            if let Some(system_prompt) = self.prompt_manager.get_system_prompt() {
                                                                llm_messages.push(crate::llm::Message::new(
                                                                    crate::llm::Role::System,
                                                                    system_prompt.to_string(),
                                                                ));
                                                            }
                                                            if let Some(ref profile_prompt) = profile_prompt {
                                                                llm_messages.push(crate::llm::Message::new(
                                                                    crate::llm::Role::System,
                                                                    profile_prompt.clone(),
                                                                ));
                                                            }
                                                            
                                                            // Collect valid tool_call_ids
                                                            let mut valid_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
                                                            for msg in &updated_msgs {
                                                                if msg.role == "assistant" {
                                                                    if let Some(ref tcs) = msg.tool_calls {
                                                                        for tc in tcs {
                                                                            valid_ids.insert(tc.id.clone());
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                            
                                                            // Add updated messages from DB, skipping orphaned tool results and summarized messages
                                                            for msg in updated_msgs {
                                                                // Skip messages that have been summarized (but keep summary messages themselves)
                                                                if msg.is_summarized && !msg.is_summary {
                                                                    continue;
                                                                }
                                                                
                                                                let role = match msg.role.as_str() {
                                                                    "user" => crate::llm::Role::User,
                                                                    "assistant" => crate::llm::Role::Assistant,
                                                                    "system" => crate::llm::Role::System,
                                                                    "tool" => {
                                                                        // Skip orphaned tool results
                                                                        if let Some(ref tid) = msg.tool_call_id {
                                                                            if !valid_ids.contains(tid) { continue; }
                                                                        } else { continue; }
                                                                        crate::llm::Role::Tool
                                                                    }
                                                                    _ => continue,
                                                                };
                                                                
                                                                let content = if role == crate::llm::Role::Tool {
                                                                    let mut combined = msg.content.clone();
                                                                    if let Some(ref result_json) = msg.tool_result_json {
                                                                        if !combined.is_empty() {
                                                                            combined.push_str("\n");
                                                                        }
                                                                        combined.push_str(&result_json.to_string());
                                                                    }
                                                                    combined
                                                                } else {
                                                                    msg.content.clone()
                                                                };
                                                                
                                                                let mut llm_msg = crate::llm::Message::new(role.clone(), content);
                                                                if role == crate::llm::Role::Tool {
                                                                    llm_msg.tool_call_id = msg.tool_call_id.clone();
                                                                }
                                                                llm_msg.reasoning_content = msg.reasoning_content.clone();
                                                                llm_messages.push(llm_msg);
                                                            }
                                                            
                                                            // Re-add current user message
                                                            llm_messages.push(current_user_message.clone());
                                                            
                                                            let new_tokens: usize = llm_messages.iter()
                                                                .map(|msg| token_counter.count_message_tokens(msg))
                                                                .sum();
                                                            println!("📊 After summarization: {} tokens", new_tokens);
                                                            
                                                            // Rebuild UI messages from DB
                                                            if let Ok(Some(conv)) = self.storage.get_conversation(&conv_id) {
                                                                self.rebuild_conversation_view(conv);
                                                            }
                                                        }
                                                    }
                                                }
                                                Err(e) => {
                                                    eprintln!("❌ Summarization failed: {}", e);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    
                    // Debug: Print all messages being sent to LLM
                    println!("🔍 DEBUG: All LLM messages being sent:");
                    for (i, msg) in llm_messages.iter().enumerate() {
                        println!(
                            "  Message {}: role={:?}, content_len={}, attachments={:?}",
                            i, msg.role, msg.content.len(), msg.attachments.is_some()
                        );
                    }

                    // Store the prepared messages for the subscription to use
                    self.pending_llm_messages = Some(llm_messages);

                    // Start streaming LLM response
                    let streaming_id = uuid::Uuid::new_v4();
                    self.current_streaming_id = Some(streaming_id);
                    self.is_streaming = true;
                    // Initialize typing indicator animation
                    self.typing_indicator_start_time = Some(cosmic::iced::time::Instant::now());
                    self.typing_indicator_progress = 0.0;
                    
                    // Play typing sound
                    crate::ui::audio::AudioService::play_sound("typing.mp3");

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
                    self.typing_indicator_start_time = None;
                    self.typing_indicator_progress = 0.0;

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
                        self.typing_indicator_start_time = None;
                        self.typing_indicator_progress = 0.0;
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
                                Message::FileChooserError(Arc::new(
                                    file_chooser::Error::UrlAbsolute,
                                ))
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
                    println!(
                        "🔍 DEBUG: File added to attached_files. Current count: {}",
                        self.attached_files.len()
                    );
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
                    // Switch to the conversation's profile, or default if not set/present
                    let profile_name_to_use = conv.profile_name.as_deref()
                        .and_then(|name| {
                            if self.config.profiles.contains_key(name) {
                                Some(name)
                            } else {
                                None
                            }
                        })
                        .unwrap_or(&self.config.default);
                    
                    // Only switch if different from current default
                    let profile_changed = if profile_name_to_use != &self.config.default {
                        if let Some(profile) = self.config.get_profile(profile_name_to_use).cloned() {
                            let masked = if profile.api_key.len() > 6 {
                                format!(
                                    "{}...{}",
                                    &profile.api_key[..3],
                                    &profile.api_key[profile.api_key.len().saturating_sub(3)..]
                                )
                            } else {
                                "***".to_string()
                            };
                            println!("🔄 Switching to conversation's profile '{}' model='{}' endpoint='{}' api_key='{}'", profile_name_to_use, profile.model, profile.endpoint, masked);
                            self.config.default = profile_name_to_use.to_string();
                            self.llm_client = llm::build_llm_client(&profile);
                            true
                        } else {
                            false
                        }
                    } else {
                        // Ensure LLM client is using the current default profile
                        if let Some(profile) = self.config.get_default_profile().cloned() {
                            self.llm_client = llm::build_llm_client(&profile);
                        }
                        false
                    };
                    
                    self.rebuild_conversation_view(conv);
                    
                    // Return profile tool defaults task if profile changed
                    if profile_changed {
                        if let Some(task) = self.profile_tool_defaults_task() {
                            // Update nav model to reflect current conversation
                            self.load_recent_conversations();
                            self.update_nav_model();
                            return task;
                        }
                    }
                }
                // Update nav model to reflect current conversation
                self.load_recent_conversations();
                self.update_nav_model();
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
                // Update nav model to reflect deleted conversation
                self.load_recent_conversations();
                self.update_nav_model();
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
                self.expanded_tool_summaries.clear();
                self.expanded_reasoning.clear();
                // Update nav model to reflect new conversation
                self.load_recent_conversations();
                self.update_nav_model();
            }
            Message::AgentUpdate(u) => {
                match u {
                    AgentUpdate::AssistantStreamingStarted => {
                        self.pending_tool_calls_for_history.clear();
                        self.tool_runtime_context.clear();
                        self.active_tool_calls.clear();
                        self.messages.push(ChatMessage {
                            content: String::new(),
                            is_user: false,
                            is_error: false,
                            reasoning_content: None,
                            is_summary: false,
            is_summarized: false,
                            summarized_count: None,
                        });
                        self.current_ai_message_index = Some(self.messages.len() - 1);
                    }
                    AgentUpdate::AssistantDelta { text_chunk, .. } => {
                        if let Some(idx) = self.current_ai_message_index {
                            if let Some(msg) = self.messages.get_mut(idx) {
                                msg.content.push_str(&text_chunk);
                            }
                        }
                        // Trigger scroll to bottom during streaming
                        // The scroll will be handled by anchor_bottom() and widget operations
                    }
                    AgentUpdate::ReasoningContentDelta { chunk } => {
                        if let Some(idx) = self.current_ai_message_index {
                            if let Some(msg) = self.messages.get_mut(idx) {
                                // Accumulate reasoning content during streaming
                                match &mut msg.reasoning_content {
                                    Some(existing) => {
                                        existing.push_str(&chunk);
                                    }
                                    None => {
                                        msg.reasoning_content = Some(chunk.clone());
                                    }
                                }
                            }
                        }
                    }
                    AgentUpdate::AssistantComplete { full_text, reasoning_content } => {
                        if let Some(idx) = self.current_ai_message_index {
                            if let Some(msg) = self.messages.get_mut(idx) {
                                msg.content = full_text.clone();
                                msg.reasoning_content = reasoning_content.clone();
                            }
                        } else {
                            self.messages.push(ChatMessage {
                                content: full_text.clone(),
                                is_user: false,
                                is_error: false,
                                reasoning_content: reasoning_content.clone(),
                                is_summary: false,
            is_summarized: false,
                                summarized_count: None,
                            });
                            self.current_ai_message_index = Some(self.messages.len() - 1);
                        }
                        if let Some(conv_id) = self.current_conversation_id {
                            let tool_calls_slice = if self.pending_tool_calls_for_history.is_empty()
                            {
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
                                reasoning_content: reasoning_content.as_deref(),
                            };
                            if let Err(e) = self.storage.add_message_with_metadata(
                                &conv_id,
                                "assistant".to_string(),
                                full_text,
                                None,
                                metadata,
                            ) {
                                eprintln!("Failed to add assistant message: {}", e);
                            } else {
                                // Update context usage cache after adding message
                                self.update_context_usage_cache(conv_id);
                            }
                        }
                        self.pending_tool_calls_for_history.clear();
                    }
                    AgentUpdate::ToolPlanned { plan_items } => {
                        let anchor = self
                            .current_ai_message_index
                            .unwrap_or_else(|| self.messages.len().saturating_sub(1));
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
                        if let Some(pos) = self.active_tool_calls.iter().position(|tc| {
                            tc.id.as_ref().map(|s| s == &tool_call_id).unwrap_or(false)
                        }) {
                            let mut info = self.active_tool_calls.remove(pos);
                            info.status = ToolCallStatus::Completed;
                            info.result = Some(result_display.clone());
                            archived_entry = Some(info);
                        }
                        if archived_entry.is_none() {
                            let params_pretty = context
                                .as_ref()
                                .and_then(|ctx| ctx.params.as_ref())
                                .map(|value| {
                                    serde_json::to_string_pretty(value)
                                        .unwrap_or_else(|_| value.to_string())
                                })
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
                            self.archived_tool_calls.push(AnchoredToolCall {
                                anchor_index: anchor,
                                tool_call: entry,
                            });
                            
                            // Play tool completion sound
                            crate::ui::audio::AudioService::play_sound("tool.mp3");
                        }

                        if let Some(conv_id) = self.current_conversation_id {
                            let params_owned = context.as_ref().and_then(|ctx| ctx.params.clone());
                            let params_ref = params_owned.as_ref();
                            let result_value = Self::coerce_value(&result_json);
                            let metadata = MessageMetadata {
                                tool_calls: None,
                                tool_call_id: Some(tool_call_id.as_str()),
                                tool_name: Some(name.as_str()),
                                tool_status: Some("success"),
                                tool_params_json: params_ref,
                                tool_result_json: Some(&result_value),
                                reasoning_content: None,
                            };
                            // Use empty content - tool_result_json holds the actual data
                            if let Err(e) = self.storage.add_message_with_metadata(
                                &conv_id,
                                "tool".to_string(),
                                String::new(),
                                None,
                                metadata,
                            ) {
                                eprintln!("Failed to add tool result: {}", e);
                            } else {
                                // Update context usage cache after adding tool message
                                self.update_context_usage_cache(conv_id);
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
                        if let Some(pos) = self.active_tool_calls.iter().position(|tc| {
                            tc.id.as_ref().map(|s| s == &tool_call_id).unwrap_or(false)
                        }) {
                            let mut info = self.active_tool_calls.remove(pos);
                            info.status = ToolCallStatus::Error;
                            info.error = Some(error.clone());
                            archived_entry = Some(info);
                        }
                        if archived_entry.is_none() {
                            let params_pretty = context
                                .as_ref()
                                .and_then(|ctx| ctx.params.as_ref())
                                .map(|value| {
                                    serde_json::to_string_pretty(value)
                                        .unwrap_or_else(|_| value.to_string())
                                })
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
                            self.archived_tool_calls.push(AnchoredToolCall {
                                anchor_index: anchor,
                                tool_call: entry,
                            });
                        }

                        if let Some(conv_id) = self.current_conversation_id {
                            let params_owned = context.as_ref().and_then(|ctx| ctx.params.clone());
                            let params_ref = params_owned.as_ref();
                            let error_value = Value::String(error.clone());
                            let metadata = MessageMetadata {
                                tool_calls: None,
                                tool_call_id: Some(tool_call_id.as_str()),
                                tool_name: Some(name.as_str()),
                                tool_status: Some("error"),
                                tool_params_json: params_ref,
                                tool_result_json: Some(&error_value),
                                reasoning_content: None,
                            };
                            // Use empty content - tool_result_json holds the error
                            if let Err(e) = self.storage.add_message_with_metadata(
                                &conv_id,
                                "tool".to_string(),
                                String::new(),
                                None,
                                metadata,
                            ) {
                                eprintln!("Failed to add tool error: {}", e);
                            } else {
                                // Update context usage cache after adding tool error message
                                self.update_context_usage_cache(conv_id);
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
                        self.typing_indicator_start_time = None;
                        self.typing_indicator_progress = 0.0;
                        self.active_tool_calls.clear();
                        self.pending_tool_calls_for_history.clear();
                        self.tool_runtime_context.clear();
                        
                        // Play completion sound
                        crate::ui::audio::AudioService::play_sound("done.mp3");
                    }
                    AgentUpdate::ModelError { error } => {
                        // Stop streaming and show error message
                        self.is_streaming = false;
                        self.current_streaming_id = None;
                        self.current_ai_message_index = None;
                        self.pending_llm_messages = None;
                        self.typing_indicator_start_time = None;
                        self.typing_indicator_progress = 0.0;
                        self.active_tool_calls.clear();
                        self.pending_tool_calls_for_history.clear();
                        self.tool_runtime_context.clear();

                        // Add error message as a separate chat bubble
                        self.messages.push(ChatMessage {
                            content: format!("❌ **Model Communication Error**\n\n{}", error),
                            is_user: false,
                            is_error: true,
                            reasoning_content: None,
                            is_summary: false,
            is_summarized: false,
                            summarized_count: None,
                        });
                    }
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
                if let Some(tool_call) = self
                    .active_tool_calls
                    .iter_mut()
                    .find(|tc| tc.tool_name == tool_name)
                {
                    tool_call.status = ToolCallStatus::Completed;
                    tool_call.result = Some(result);
                }
            }
            Message::ToolCallError(tool_name, error) => {
                // Update tool call status
                if let Some(tool_call) = self
                    .active_tool_calls
                    .iter_mut()
                    .find(|tc| tc.tool_name == tool_name)
                {
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
            Message::ToggleToolSummary(message_idx, summary_id) => {
                let key = (message_idx, summary_id);
                if self.expanded_tool_summaries.contains(&key) {
                    self.expanded_tool_summaries.remove(&key);
                } else {
                    self.expanded_tool_summaries.insert(key);
                }
            }
            Message::ToggleReasoning(message_idx) => {
                if self.expanded_reasoning.contains(&message_idx) {
                    self.expanded_reasoning.remove(&message_idx);
                } else {
                    self.expanded_reasoning.insert(message_idx);
                }
            }
            Message::ToggleSummary(message_idx) => {
                if self.expanded_summaries.contains(&message_idx) {
                    self.expanded_summaries.remove(&message_idx);
                } else {
                    self.expanded_summaries.insert(message_idx);
                }
            }
            Message::ToggleMCPServer(server_name) => {
                if self.expanded_mcp_servers.contains(&server_name) {
                    self.expanded_mcp_servers.remove(&server_name);
                } else {
                    self.expanded_mcp_servers.insert(server_name);
                }
            }
            Message::OpenMCPConfig => {
                // Get MCP config file path
                let mcp_config_path = dirs::data_dir()
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join("cosmic_llm")
                    .join("mcp_config.json");
                
                // Open file in cosmic-edit
                let path_str = mcp_config_path.to_string_lossy().to_string();
                
                match std::process::Command::new("cosmic-edit")
                    .arg(&path_str)
                    .spawn()
                {
                    Ok(_) => {
                        println!("Opened MCP config file in cosmic-edit: {}", path_str);
                    }
                    Err(e) => {
                        eprintln!("Failed to open MCP config file in cosmic-edit: {}", e);
                        // Show error dialog to user
                        self.dialog = Some(DialogPage::message_text(format!(
                            "Failed to open MCP config file in cosmic-edit:\n{}\n\nError: {}\n\nMake sure cosmic-edit is installed.",
                            path_str, e
                        )));
                    }
                }
            }
            Message::OpenConfigFile => {
                // Get config file path
                let config_path = AppConfig::config_toml_path();
                let path_str = config_path.to_string_lossy().to_string();
                
                match std::process::Command::new("cosmic-edit")
                    .arg(&path_str)
                    .spawn()
                {
                    Ok(_) => {
                        println!("Opened config file in cosmic-edit: {}", path_str);
                    }
                    Err(e) => {
                        eprintln!("Failed to open config file in cosmic-edit: {}", e);
                        self.dialog = Some(DialogPage::message_text(format!(
                            "Failed to open config file in cosmic-edit:\n{}\n\nError: {}\n\nMake sure cosmic-edit is installed.",
                            path_str, e
                        )));
                    }
                }
            }
            Message::OpenProfilePrompt(profile_name) => {
                // Get prompt file path for the profile
                if let Some(profile) = self.config.profiles.get(&profile_name) {
                    if let Some(prompt_file) = &profile.profile_prompt_file {
                        let prompt_path = AppConfig::resolve_config_path(prompt_file);
                        let path_str = prompt_path.to_string_lossy().to_string();
                        
                        match std::process::Command::new("cosmic-edit")
                            .arg(&path_str)
                            .spawn()
                        {
                            Ok(_) => {
                                println!("Opened prompt file in cosmic-edit: {}", path_str);
                            }
                            Err(e) => {
                                eprintln!("Failed to open prompt file in cosmic-edit: {}", e);
                                self.dialog = Some(DialogPage::message_text(format!(
                                    "Failed to open prompt file in cosmic-edit:\n{}\n\nError: {}\n\nMake sure cosmic-edit is installed.",
                                    path_str, e
                                )));
                            }
                        }
                    } else {
                        self.dialog = Some(DialogPage::message_text(format!(
                            "Profile '{}' does not have a prompt file configured.",
                            profile_name
                        )));
                    }
                }
            }
            Message::ScrollToBottom => {
                // Trigger scroll operation to bottom
                // The actual scrolling will be handled in the view method using widget operations
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
            Message::OpenSettings => {
                self.current_page = NavigationPage::Settings;
                // Reload config from file to ensure we have the latest values
                if let Ok(fresh_config) = AppConfig::load() {
                    self.config = fresh_config;
                }
                // Load current config values into settings page (this also initializes staged config)
                self.settings_page.load_from_config(&self.config);
            }
            Message::ManualSummarize => {
                if let Some(conv_id) = self.current_conversation_id {
                    self.perform_manual_summarization(conv_id);
                } else {
                    eprintln!("⚠️ No active conversation to summarize");
                }
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
                        println!("🔄 Switching default profile to '{}' model='{}' endpoint='{}' api_key='{}'", self.config.default, profile.model, profile.endpoint, masked);
                        self.llm_client = llm::build_llm_client(&profile);
                        
                        // Update active conversation's profile in database if there is one
                        if let Some(conv_id) = self.current_conversation_id {
                            if let Err(e) = self.storage.update_conversation_profile(&conv_id, Some(&new_profile)) {
                                eprintln!("Failed to update conversation profile: {}", e);
                            } else {
                                println!("✅ Updated conversation {} profile to '{}'", conv_id, new_profile);
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
                            self.settings_page.staged_default = self.settings_page.staged_profiles.keys().next().unwrap().clone();
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
                            eprintln!("Failed to save settings: {}", e);
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
                            if let Some(conv_id) = self.current_conversation_id {
                                if let Err(e) = self.storage.update_conversation_profile(&conv_id, Some(&profile_changed)) {
                                    eprintln!("Failed to update conversation profile: {}", e);
                                } else {
                                    println!("✅ Updated conversation {} profile to '{}'", conv_id, profile_changed);
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
                            eprintln!("Failed to save config: {}", e);
                        }
                        
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
            Message::InlineError(error) => {
                self.current_error = Some(error);
            }
            Message::DismissError => {
                self.current_error = None;
            }
            Message::TypingIndicatorTick(instant) => {
                if let Some(start_time) = self.typing_indicator_start_time {
                    let elapsed = instant.duration_since(start_time);
                    // Update animation progress (cycles every 1.2 seconds)
                    self.typing_indicator_progress = (elapsed.as_secs_f32() / 1.2) % 1.0;
                }
            }
            Message::RefreshConversationList => {
                self.load_recent_conversations();
                self.update_nav_model();
            }
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
                    if self.current_conversation_id.is_some() {
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

        if self.show_tools_context {
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
    fn update_nav_model(&mut self) {
        // Clear and rebuild the nav model
        let mut model = widget::segmented_button::ModelBuilder::default().build();
        
        let current_conv_id = self.current_conversation_id;
        
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
            let is_in_recent = self.recent_conversations.iter()
                .any(|(id, _)| *id == active_conv_id);
            
            if !is_in_recent {
                // Fetch the active conversation's title from storage
                self.storage.get_conversation(&active_conv_id)
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
        for (conv_id, title) in &self.recent_conversations {
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
        
        self.nav_model = model;
    }
    
    /// Load recent conversations from storage (last 11 to accommodate active conversation)
    fn load_recent_conversations(&mut self) {
        match self.storage.list_conversations_paginated(None, Some(11)) {
            Ok(conversations) => {
                self.recent_conversations = conversations
                    .into_iter()
                    .map(|c| (c.id, c.title))
                    .collect();
            }
            Err(e) => {
                eprintln!("Failed to load recent conversations: {}", e);
                self.recent_conversations.clear();
            }
        }
    }
    
    pub(crate) fn error_banner(&self) -> Option<Element<Message>> {
        self.current_error.as_ref().map(|error| {
            let content = widget::row::with_children(vec![
                crate::ui::icons::get_icon("dialog-warning-symbolic", 16).into(),
                widget::text(error.clone()).size(14).into(),
                widget::Space::with_width(cosmic::iced::Length::Fill).into(),
                widget::button::standard("Dismiss")
                    .on_press(Message::DismissError)
                    .padding([4, 12])
                    .into(),
            ])
            .spacing(12)
            .align_y(cosmic::iced::Alignment::Center);

            widget::container(content)
                .padding(12)
                .width(cosmic::iced::Length::Fill)
                .class(cosmic::style::Container::Card)
                .into()
        })
    }

    fn load_active_profile_prompt(&mut self) -> Option<String> {
        let profile = self.config.get_default_profile()?;
        let path = profile.profile_prompt_file.as_deref()?;

        let resolved_path = crate::config::AppConfig::resolve_config_path(path);
        let resolved = resolved_path.to_string_lossy().to_string();

        match self.prompt_manager.load_profile_prompt(&resolved) {
            Ok(content) => {
                if self
                    .current_error
                    .as_deref()
                    .map(|msg| msg.starts_with("Profile prompt"))
                    .unwrap_or(false)
                {
                    self.current_error = None;
                }
                Some(content)
            }
            Err(err) => {
                let message = match &err {
                    ProfilePromptError::NotFound(_) => {
                        format!("Profile prompt not found: {}", resolved)
                    }
                    _ => err.to_string(),
                };
                self.current_error = Some(message);
                None
            }
        }
    }

    fn profile_tool_defaults_task(&self) -> Option<app::Task<Message>> {
        let profile = self.config.get_default_profile()?;
        // Always apply profile defaults, even if enabled_mcp is empty
        // (empty list means enable all tools)
        let allowed_servers = profile.enabled_mcp.clone();
        let registry = self.mcp_registry.clone();

        Some(cosmic::Task::perform(
            async move {
                let mut registry = registry.write().await;
                registry.apply_profile_tool_defaults(&allowed_servers);
                cosmic::Action::App(Message::RefreshMCPTools)
            },
            |msg| msg,
        ))
    }

    fn create_menu_bar(&self) -> Element<Message> {
        use cosmic::widget::menu::{items, root, Item, ItemHeight, ItemWidth, MenuBar, Tree};
        use cosmic::widget::RcElementWrapper;

        MenuBar::new(vec![
            Tree::with_children(
                RcElementWrapper::new(Element::from(root("File"))),
                items(
                    &self.key_binds,
                    vec![
                        Item::Button("Summarize Conversation", None, MenuAction::SummarizeConversation),
                        Item::Button("Quit", None, MenuAction::Quit),
                    ],
                ),
            ),
            Tree::with_children(
                RcElementWrapper::new(Element::from(root("View"))),
                items(
                    &self.key_binds,
                    vec![Item::Button("Settings", None, MenuAction::Settings)],
                ),
            ),
            Tree::with_children(
                RcElementWrapper::new(Element::from(root("Help"))),
                items(
                    &self.key_binds,
                    vec![Item::Button("About", None, MenuAction::About)],
                ),
            ),
        ])
        .item_height(ItemHeight::Dynamic(40))
        .item_width(ItemWidth::Uniform(200))
        .spacing(4.0)
        .into()
    }
}
