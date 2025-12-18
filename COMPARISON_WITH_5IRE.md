# LunaAI vs 5ire: Feature Comparison & Implementation Guide

## 📊 Feature Comparison Matrix

| Feature | LunaAI | 5ire | Status |
|---------|--------|------|--------|
| **Core Features** |
| MCP Integration | ✅ Stdio-based | ✅ Stdio-based | ✅ Match |
| Multiple LLM Providers | ✅ OpenAI, Anthropic, Gemini, Ollama, DeepSeek | ✅ OpenAI, Azure, Anthropic, Google, Mistral, Doubao, Grok, DeepSeek, Ollama | ⚠️ Missing: Azure, Mistral, Doubao, Grok |
| Conversation Storage | ✅ SQLite with FTS5 | ✅ (implied) | ✅ Match |
| Search | ✅ FTS5 full-text search | ✅ Quick search | ✅ Match |
| Mobile App | ✅ Flutter | ❌ Desktop only | ✅ LunaAI advantage |
| Server Mode | ✅ WebSocket server | ❌ | ✅ LunaAI advantage |
| **Advanced Features** |
| Local Knowledge Base/RAG | ❌ Schema ready, not implemented | ✅ bge-m3 embeddings + RAG | ❌ **MISSING** |
| Document Parsing | ✅ Basic (markdownify) | ✅ Full (docx, xlsx, pptx, pdf, txt, csv) | ⚠️ Partial |
| Usage Analytics | ❌ | ✅ API usage & cost tracking | ❌ **MISSING** |
| Prompts Library | ⚠️ Basic (system/profile prompts) | ✅ Library with variables | ⚠️ **MISSING** |
| Bookmarks | ❌ | ✅ Conversation bookmarks | ❌ **MISSING** |
| MCP Marketplace | ❌ | ✅ Discovery & one-click install | ❌ **MISSING** |
| Title Generation | ✅ Background auto-generation | ✅ (implied) | ✅ Match |

---

## 🚨 Critical Missing Features

### 1. **Local Knowledge Base / RAG** ⭐ HIGH PRIORITY

**Current State:**
- Database schema has `embedding BLOB` field in messages table
- No embedding generation or RAG implementation
- Documents are converted to markdown but not vectorized

**5ire Implementation:**
- Uses bge-m3 embedding model (multilingual)
- Parses: docx, xlsx, pptx, pdf, txt, csv
- Stores vectors locally
- RAG retrieval for context injection

**Implementation Plan:**

#### Step 1: Add Embedding Model Integration
```rust
// New dependency in Cargo.toml
candle-core = "0.3"
candle-nn = "0.3"
candle-transformers = "0.3"  // For bge-m3 or similar
// OR use ONNX runtime
ort = "2.0"  // For ONNX models
```

#### Step 2: Create Knowledge Base Module
```rust
// src/storage/knowledge_base.rs
pub struct KnowledgeBase {
    storage: SqliteStorage,
    embedding_model: EmbeddingModel,
}

pub struct Document {
    id: String,
    file_path: String,
    file_type: String,
    chunks: Vec<DocumentChunk>,
    metadata: HashMap<String, String>,
}

pub struct DocumentChunk {
    id: String,
    document_id: String,
    content: String,
    embedding: Vec<f32>,
    chunk_index: usize,
}
```

#### Step 3: Document Processing Pipeline
1. **Parse documents** (extend `file_utils.rs`):
   - PDF: Use `pdf-extract` or `lopdf`
   - DOCX: Use `docx-rs` or `calamine` for xlsx
   - PPTX: Use `pptx-rs`
   - TXT/CSV: Already handled

2. **Chunk documents**:
   - Split into semantic chunks (500-1000 tokens)
   - Preserve metadata (page numbers, headers, etc.)

3. **Generate embeddings**:
   - Use bge-m3 or similar local model
   - Store in database with document metadata

4. **RAG Retrieval**:
   - On user query, generate query embedding
   - Search similar chunks (cosine similarity)
   - Inject top-k chunks as context

#### Step 4: Database Schema Extension
```sql
CREATE TABLE IF NOT EXISTS documents (
    id TEXT PRIMARY KEY,
    file_path TEXT NOT NULL,
    file_type TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    metadata TEXT  -- JSON
);

CREATE TABLE IF NOT EXISTS document_chunks (
    id TEXT PRIMARY KEY,
    document_id TEXT NOT NULL,
    content TEXT NOT NULL,
    embedding BLOB NOT NULL,
    chunk_index INTEGER NOT NULL,
    FOREIGN KEY (document_id) REFERENCES documents(id) ON DELETE CASCADE
);

-- Vector similarity search index (using SQLite with extension or external)
CREATE VIRTUAL TABLE IF NOT EXISTS chunks_fts USING fts5(
    content,
    content = 'document_chunks'
);
```

**Recommended Libraries:**
- **Embeddings**: `candle-transformers` (Rust-native) or `ort` (ONNX runtime)
- **PDF**: `lopdf` or `pdf-extract`
- **DOCX**: `docx-rs`
- **XLSX**: `calamine`
- **Vector Search**: SQLite with `sqlite-vss` extension or `qdrant` (external)

---

### 2. **Usage Analytics** ⭐ HIGH PRIORITY

**Current State:**
- No tracking of API calls, tokens, or costs

**5ire Implementation:**
- Tracks API usage per provider
- Calculates costs based on token usage
- Shows spending over time

**Implementation Plan:**

#### Step 1: Create Usage Tracking Module
```rust
// src/storage/usage_analytics.rs
pub struct UsageRecord {
    id: String,
    timestamp: i64,
    profile_name: String,
    provider: String,
    model: String,
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
    cost_usd: f64,
    request_type: String,  // "chat", "title_generation", etc.
}

pub struct UsageAnalytics {
    storage: SqliteStorage,
    pricing: PricingConfig,
}
```

#### Step 2: Database Schema
```sql
CREATE TABLE IF NOT EXISTS usage_records (
    id TEXT PRIMARY KEY,
    timestamp INTEGER NOT NULL,
    profile_name TEXT NOT NULL,
    provider TEXT NOT NULL,
    model TEXT NOT NULL,
    prompt_tokens INTEGER NOT NULL,
    completion_tokens INTEGER NOT NULL,
    total_tokens INTEGER NOT NULL,
    cost_usd REAL NOT NULL,
    request_type TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_usage_timestamp ON usage_records(timestamp);
CREATE INDEX IF NOT EXISTS idx_usage_profile ON usage_records(profile_name);
```

#### Step 3: Integrate into LLM Clients
- Track token usage from API responses
- Calculate costs using pricing tables
- Store after each API call

#### Step 4: Pricing Configuration
```toml
# config.toml
[pricing]
[pricing.openai]
"gpt-4o" = { prompt = 0.0025, completion = 0.010 }
"gpt-4-turbo" = { prompt = 0.01, completion = 0.03 }
[pricing.anthropic]
"claude-3-5-sonnet" = { prompt = 0.003, completion = 0.015 }
```

#### Step 5: UI Dashboard
- Show daily/weekly/monthly usage
- Cost breakdown by provider/model
- Token usage charts
- Export to CSV

---

### 3. **Prompts Library with Variables** ⭐ MEDIUM PRIORITY

**Current State:**
- Basic system prompts and profile prompts
- No variable substitution
- No library/organization

**5ire Implementation:**
- Library of saved prompts
- Variable substitution (e.g., `{{variable_name}}`)
- Organization and categories

**Implementation Plan:**

#### Step 1: Extend Prompt System
```rust
// src/prompts.rs (extend existing)
pub struct PromptTemplate {
    id: String,
    name: String,
    category: Option<String>,
    template: String,  // With {{variable}} placeholders
    variables: Vec<PromptVariable>,
    created_at: i64,
}

pub struct PromptVariable {
    name: String,
    description: Option<String>,
    default_value: Option<String>,
}

impl PromptTemplate {
    pub fn render(&self, variables: &HashMap<String, String>) -> String {
        // Replace {{variable}} with values
    }
}
```

#### Step 2: Storage
```sql
CREATE TABLE IF NOT EXISTS prompt_templates (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    category TEXT,
    template TEXT NOT NULL,
    variables TEXT,  -- JSON array
    created_at INTEGER NOT NULL
);
```

#### Step 3: UI
- Prompt library page
- Create/edit prompts
- Variable input form
- Quick insert into chat

---

### 4. **Bookmarks** ⭐ MEDIUM PRIORITY

**Current State:**
- No bookmarking feature

**5ire Implementation:**
- Bookmark specific conversations
- Bookmarked content persists even if conversation deleted
- Quick access to bookmarks

**Implementation Plan:**

#### Step 1: Database Schema
```sql
CREATE TABLE IF NOT EXISTS bookmarks (
    id TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL,
    title TEXT NOT NULL,
    content TEXT NOT NULL,  -- Snapshot of bookmarked content
    created_at INTEGER NOT NULL,
    notes TEXT,  -- User notes about the bookmark
    FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_bookmarks_conversation ON bookmarks(conversation_id);
```

#### Step 2: API
```rust
// src/storage/storage_wrapper.rs
pub fn create_bookmark(
    &self,
    conversation_id: &str,
    title: &str,
    content: &str,
    notes: Option<&str>,
) -> Result<String>;

pub fn list_bookmarks(&self) -> Result<Vec<Bookmark>>;
pub fn delete_bookmark(&self, bookmark_id: &str) -> Result<bool>;
```

#### Step 3: UI
- Bookmark button in conversation view
- Bookmarks page/section
- Search bookmarks

---

### 5. **MCP Marketplace Integration** ⭐ LOW PRIORITY

**Current State:**
- Manual MCP server configuration
- No discovery mechanism

**5ire Implementation:**
- Marketplace for MCP servers
- One-click installation
- Community-driven

**Implementation Plan:**

#### Option 1: Integrate with Existing Marketplace
- Use MCPSvr API (if available)
- Fetch server list
- Display in UI

#### Option 2: Build Simple Registry
```rust
// src/mcp/marketplace.rs
pub struct MCPServerListing {
    id: String,
    name: String,
    description: String,
    author: String,
    install_command: String,
    install_args: Vec<String>,
    category: String,
}

pub struct Marketplace {
    servers: Vec<MCPServerListing>,
}
```

#### Implementation Steps:
1. Fetch server registry (JSON/API)
2. Display in UI with search/filter
3. One-click install (adds to mcp_config.json)
4. Auto-configure environment variables if needed

---

## 🎯 Implementation Priority

### Phase 1: High Impact (Weeks 1-4)
1. **Usage Analytics** - Quick win, high value
2. **Bookmarks** - Simple feature, immediate utility
3. **Prompts Library** - Extends existing system

### Phase 2: Advanced Features (Weeks 5-12)
4. **Local Knowledge Base/RAG** - Complex but powerful
   - Start with basic embedding model
   - Add document parsing incrementally
   - Implement RAG retrieval

### Phase 3: Ecosystem (Weeks 13+)
5. **MCP Marketplace** - Nice-to-have, community feature

---

## 🔧 Technical Recommendations

### For RAG Implementation:
1. **Start Simple**: Use ONNX runtime with pre-trained bge-m3 model
2. **Incremental**: Add document types one at a time
3. **Performance**: Consider external vector DB (Qdrant) for large-scale
4. **Chunking Strategy**: Use semantic chunking (sentence transformers) vs fixed-size

### For Usage Analytics:
1. **Lightweight**: Track in existing SQLite database
2. **Real-time**: Update on every API call
3. **Privacy**: Make opt-in/opt-out configurable

### For Prompts Library:
1. **Backward Compatible**: Extend existing prompt system
2. **Variables**: Use simple `{{var}}` syntax (handlebars-like)
3. **Validation**: Check for required variables before rendering

---

## 📝 Additional Notes

### LunaAI Advantages:
- ✅ Mobile app support
- ✅ Server mode for multi-device access
- ✅ Native Rust implementation (performance)
- ✅ COSMIC desktop integration

### 5ire Advantages:
- ✅ More LLM providers
- ✅ Complete RAG implementation
- ✅ Usage analytics
- ✅ MCP marketplace

### Recommended Approach:
Focus on **Usage Analytics** and **Bookmarks** first (quick wins), then tackle **RAG** as the major feature addition. The prompts library can be built incrementally alongside existing prompt system.








