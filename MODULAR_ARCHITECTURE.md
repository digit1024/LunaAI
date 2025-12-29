# Modular Architecture Plan

**Goal:** Break down `app.rs` (3723 lines) into modular components following libcosmic pattern.

---

## 🎯 Strategy: Divide and Win

Following the [libcosmic modules pattern](https://pop-os.github.io/libcosmic-book/modules.html), we'll extract:

1. **Page Modules** - Each page owns its state and messages
2. **State Modules** - Shared state extracted into focused modules
3. **Service Modules** - Business logic extracted into services

---

## 📦 Module Structure

### 1. Page Modules (libcosmic pattern)

Each page becomes a self-contained module with:
- `Page` struct (own state)
- `Message` enum (own messages)
- `view()` method
- `update()` method

#### `src/ui/pages/chat/mod.rs` → `src/ui/pages/chat/page.rs`

**Current:** Just view functions  
**Target:** Full module with state

```rust
pub struct Page {
    // Chat-specific state
    input: String,
    input_content: text_editor::Content,
    messages: Vec<ChatMessage>,
    attached_files: Vec<String>,
    // ... chat-specific fields
}

pub enum Message {
    InputChanged(String),
    SendMessage,
    AttachFile,
    // ... chat-specific messages
}

impl Page {
    pub fn view(&self) -> Element<Message> { ... }
    pub fn update(&mut self, message: Message) -> Task<Action<Message>> { ... }
}
```

#### `src/ui/pages/history/mod.rs` → `src/ui/pages/history/page.rs`

**Current:** Just view function  
**Target:** Full module with state

```rust
pub struct Page {
    conversations: Vec<Conversation>,
    search_query: String,
    search_results: Vec<Snippet>,
    // ... history-specific fields
}

pub enum Message {
    SelectConversation(Uuid),
    DeleteConversation(Uuid),
    SearchChanged(String),
    // ... history-specific messages
}
```

#### `src/ui/pages/settings/page.rs` (already exists, enhance)

**Current:** Partial module  
**Target:** Complete module with all settings state

#### `src/ui/pages/mcp_config/mod.rs` → `src/ui/pages/mcp_config/page.rs`

**Current:** Just view function  
**Target:** Full module with state

---

### 2. State Modules

Extract shared state into focused modules:

#### `src/ui/state/conversation.rs`

```rust
pub struct ConversationState {
    pub current_conversation_id: Option<Uuid>,
    pub messages: Vec<ChatMessage>,
    pub recent_conversations: Vec<(Uuid, String)>,
    pub context_usage_cache: HashMap<Uuid, Option<u32>>,
}

impl ConversationState {
    pub fn load_conversation(&mut self, id: Uuid, storage: &Storage) -> Result<()> { ... }
    pub fn create_conversation(&mut self, storage: &Storage) -> Result<Uuid> { ... }
    pub fn delete_conversation(&mut self, id: Uuid, storage: &Storage) -> Result<()> { ... }
}
```

#### `src/ui/state/tool_calls.rs`

```rust
pub struct ToolCallState {
    pub active_tool_calls: Vec<ToolCallInfo>,
    pub archived_tool_calls: Vec<AnchoredToolCall>,
    pub expanded_tool_calls: HashSet<usize>,
    pub expanded_tool_summaries: HashSet<(usize, String)>,
    pub current_ai_message_index: Option<usize>,
    pub pending_tool_calls_for_history: Vec<ToolCall>,
    pub tool_runtime_context: HashMap<String, ToolRuntimeContext>,
}

impl ToolCallState {
    pub fn add_tool_call(&mut self, tool_call: ToolCallInfo) { ... }
    pub fn complete_tool_call(&mut self, id: &str, result: String) { ... }
    pub fn archive_tool_call(&mut self, index: usize) { ... }
}
```

#### `src/ui/state/attachments.rs`

```rust
pub struct AttachmentState {
    pub attached_files: Vec<String>,
    pub pending_llm_messages: Option<Vec<LlmMessage>>,
}

impl AttachmentState {
    pub fn add_file(&mut self, path: String) -> Result<()> { ... }
    pub fn remove_file(&mut self, path: &str) { ... }
    pub fn create_attachments(&self) -> Result<Vec<Attachment>> { ... }
}
```

#### `src/ui/state/context.rs`

```rust
pub struct ContextState {
    pub expanded_reasoning: HashSet<usize>,
    pub expanded_summaries: HashSet<usize>,
    pub show_tools_context: bool,
}

impl ContextState {
    pub fn toggle_reasoning(&mut self, index: usize) { ... }
    pub fn toggle_summary(&mut self, index: usize) { ... }
}
```

---

### 3. Service Modules

Extract business logic into services:

#### `src/services/message_converter.rs`

**Purpose:** Single source of truth for DB ↔ LLM conversion

```rust
pub struct MessageConverter;

impl MessageConverter {
    pub fn db_to_llm(
        db_messages: &[StorageMessage],
        skip_summarized: bool,
    ) -> Vec<LlmMessage> {
        // Single implementation of conversion logic
        // Currently duplicated in:
        // - app.rs:1344-1454 (SendMessage)
        // - app.rs:1574-1644 (After summarization)
        // - app.rs:641-727 (rebuild_conversation_view)
        // - server/handlers.rs:824-953 (build_llm_messages)
    }
}
```

#### `src/services/context_service.rs`

**Purpose:** Unified context management (truncation, summarization)

```rust
pub struct ContextService;

impl ContextService {
    pub fn prepare_context(
        &self,
        messages: &[LlmMessage],
        profile: &LlmProfile,
        prompt_manager: &PromptManager,
    ) -> Result<Vec<LlmMessage>> {
        // Single implementation of context preparation
        // Currently duplicated in:
        // - app.rs (desktop)
        // - server/handlers.rs (server)
    }
}
```

#### `src/services/tool_call_manager.rs`

**Purpose:** Tool call lifecycle management

```rust
pub struct ToolCallManager {
    registry: Arc<RwLock<MCPServerRegistry>>,
}

impl ToolCallManager {
    pub async fn execute_tool_call(
        &self,
        tool_call: &ToolCall,
    ) -> Result<ToolCallResult> { ... }
}
```

---

## 🔄 Main App Structure

After refactoring, `app.rs` becomes a coordinator:

```rust
pub struct CosmicLlmApp {
    pub core: Core,
    pub config: AppConfig,
    pub storage: Storage,
    pub prompt_manager: PromptManager,
    pub mcp_registry: Arc<RwLock<MCPServerRegistry>>,
    pub llm_client: Arc<dyn LlmClient>,
    
    // Page modules
    pub chat_page: chat::Page,
    pub history_page: history::Page,
    pub settings_page: settings::Page,
    pub mcp_config_page: mcp_config::Page,
    
    // State modules
    pub conversation_state: ConversationState,
    pub tool_call_state: ToolCallState,
    pub attachment_state: AttachmentState,
    pub context_state: ContextState,
    
    // Services (stateless, can be Arc)
    pub message_converter: Arc<MessageConverter>,
    pub context_service: Arc<ContextService>,
    pub tool_call_manager: Arc<ToolCallManager>,
    
    // Navigation
    pub current_page: NavigationPage,
    pub nav_model: widget::segmented_button::SingleSelectModel,
    
    // UI state
    pub dialog: Option<DialogPage>,
    pub about: widget::about::About,
    // ... minimal UI state
}

#[derive(Debug, Clone)]
pub enum Message {
    // Navigation
    NavigateTo(NavigationPage),
    
    // Page messages (delegated)
    ChatPage(chat::Message),
    HistoryPage(history::Message),
    SettingsPage(settings::Message),
    MCPConfigPage(mcp_config::Message),
    
    // Global actions
    AgentUpdate(AgentUpdate),
    RefreshConversationList,
    // ... minimal global messages
}

impl Application for CosmicLlmApp {
    fn update(&mut self, message: Self::Message) -> Task<Self::Message> {
        match message {
            Message::NavigateTo(page) => {
                self.current_page = page;
                Task::none()
            }
            Message::ChatPage(msg) => {
                self.chat_page.update(msg)
                    .map(Message::ChatPage)
            }
            Message::HistoryPage(msg) => {
                self.history_page.update(msg)
                    .map(Message::HistoryPage)
            }
            // ... delegate to pages
        }
    }
    
    fn view(&self) -> Element<Self::Message> {
        match self.current_page {
            NavigationPage::Chat => {
                self.chat_page.view().map(Message::ChatPage)
            }
            NavigationPage::History => {
                self.history_page.view().map(Message::HistoryPage)
            }
            // ... delegate to pages
        }
    }
}
```

---

## 📋 Implementation Phases

### Phase 1: Extract Services ✅ **COMPLETE**
1. ✅ Create `MessageConverter` service
2. ✅ Create `ContextService` service
3. ✅ Create `ToolCallManager` service
4. ⏳ Replace duplicated logic with service calls (next step)

### Phase 2: Extract State Modules ✅ **COMPLETE**
1. ✅ Create `ConversationState` - Manages conversations, messages, recent list, context cache
2. ✅ Create `ToolCallState` - Manages active/archived tool calls, expansion state
3. ✅ Create `AttachmentState` - Manages file attachments and LLM message preparation
4. ✅ Create `ContextState` - Manages UI context (expanded reasoning, summaries, tools panel)
5. ⏳ Move state from `app.rs` to state modules (next step - integration)

### Phase 3: Extract Page Modules (Higher Risk)
1. ✅ Convert `chat` page to full module
2. ✅ Convert `history` page to full module
3. ✅ Enhance `settings` page module
4. ✅ Convert `mcp_config` page to full module

### Phase 4: Refactor Main App (Final)
1. ✅ Update `app.rs` to coordinate modules
2. ✅ Reduce to < 1000 lines
3. ✅ Test all functionality

---

## ✅ Success Criteria

- [ ] `app.rs` < 1000 lines (currently 3723)
- [ ] Each page module < 500 lines
- [ ] Each state module < 300 lines
- [ ] Each service module < 400 lines
- [ ] No duplicated message conversion logic
- [ ] No duplicated context management logic
- [ ] Clear separation of concerns
- [ ] All tests pass
- [ ] All functionality preserved

---

## 🎯 Benefits

1. **Maintainability:** Each module has a single responsibility
2. **Testability:** Modules can be tested independently
3. **Reusability:** Services can be used by both desktop and server
4. **Clarity:** Clear boundaries between concerns
5. **Performance:** Smaller match statements, better compiler optimization
6. **Collaboration:** Multiple developers can work on different modules

---

## 📝 Notes

- Follow libcosmic pattern exactly for page modules
- Use `From`/`Into` traits for message conversions
- Keep services stateless where possible (use `Arc` for sharing)
- State modules should be owned by the app, not `Arc` (they're mutable)
- Page modules own their state but can access shared state via app reference

