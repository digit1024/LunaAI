# Cosmic LLM - DRY Violations Analysis Report

## Identified Code Duplication

### 1. Message Conversion Logic
**Location:** Multiple files with similar message conversion patterns

**Duplicated Code:**
- `src/ui/app.rs:450-480` - Message to UI element conversion
- `src/ui/app.rs:520-550` - Similar conversion with minor variations
- `src/server/handlers.rs:120-150` - Server-side message conversion

**Example:**
```rust
// Similar conversion logic repeated
fn convert_message_to_ui(message: &Message) -> Element<Message> {
    match message {
        Message::User(text) => text_input(text).into(),
        Message::Assistant(text) => text(text).into(),
        // ... repeated pattern
    }
}
```

**Fix:** Extract common message conversion logic into shared utility

### 2. Tool Call Processing
**Location:** Multiple handlers with similar tool execution patterns

**Duplicated Code:**
- `src/ui/app.rs:1200-1250` - Tool call handling in main app
- `src/agentic/loop_engine.rs:80-120` - Similar tool execution logic
- `src/server/handlers.rs:200-250` - Server-side tool processing

**Example:**
```rust
// Repeated tool execution pattern
async fn execute_tool(tool_call: &ToolCall) -> Result<ToolResult> {
    let registry = self.mcp_registry.read().await;
    let tool = registry.get_tool(&tool_call.name)?;
    let result = tool.execute(&tool_call.arguments).await?;
    Ok(result)
}
```

**Fix:** Create centralized `ToolExecutor` service

### 3. File Attachment Validation
**Location:** Multiple file validation implementations

**Duplicated Code:**
- `src/llm/file_utils.rs:33-50` - File reading and validation
- `src/ui/app.rs:800-850` - Similar file validation logic
- `src/server/handlers.rs:300-350` - Server-side file validation

**Example:**
```rust
// Similar validation repeated
fn validate_file_path(path: &Path) -> bool {
    path.exists() && path.is_file() && path.metadata().is_ok()
}
```

**Fix:** Extract common file validation utilities

### 4. Configuration Loading
**Location:** Multiple configuration loading patterns

**Duplicated Code:**
- `src/config/mod.rs:50-80` - Main configuration loading
- `src/mcp/registry.rs:100-130` - MCP configuration loading
- `src/server/mod.rs:30-60` - Server configuration loading

**Example:**
```rust
// Similar config loading patterns
fn load_config() -> Result<Config> {
    let config_dir = dirs::config_dir()
        .ok_or_else(|| anyhow::anyhow!("Cannot find config directory"))?
        .join("cosmic_llm");
    // ... repeated logic
}
```

**Fix:** Create unified configuration loader

### 5. Error Message Construction
**Location:** Widespread error message formatting

**Duplicated Code:**
- Multiple files with similar error message patterns
- Inconsistent error message formatting
- Repeated error context building

**Example:**
```rust
// Similar error construction
let error_msg = format!("Failed to {}: {}", operation, error);
// Repeated in multiple places with minor variations
```

**Fix:** Create error message builder utility

## DRY Violations by Module

### UI Module
- **Violations:** 4
- **Main Issues:** Message conversion, file validation, error handling
- **Impact:** High - affects maintainability and consistency

### LLM Module
- **Violations:** 2
- **Main Issues:** File utilities, API error handling
- **Impact:** Medium - affects code organization

### MCP Module
- **Violations:** 3
- **Main Issues:** Configuration loading, tool execution
- **Impact:** High - affects protocol consistency

### Server Module
- **Violations:** 2
- **Main Issues:** Message conversion, file handling
- **Impact:** Medium - affects API consistency

## Refactoring Recommendations

### High Priority Refactoring

#### 1. Create MessageConverter Service
```rust
pub struct MessageConverter;

impl MessageConverter {
    pub fn to_ui_element(message: &Message) -> Element<Message> {
        // Unified conversion logic
    }

    pub fn to_server_message(message: &Message) -> ServerMessage {
        // Unified server conversion
    }
}
```

#### 2. Create ToolExecutor Service
```rust
pub struct ToolExecutor {
    registry: Arc<RwLock<MCPServerRegistry>>,
}

impl ToolExecutor {
    pub async fn execute(&self, tool_call: &ToolCall) -> Result<ToolResult> {
        // Centralized tool execution
    }
}
```

#### 3. Create FileValidator Utility
```rust
pub struct FileValidator;

impl FileValidator {
    pub fn validate_path(path: &Path) -> Result<()> {
        // Unified file validation
    }

    pub fn validate_size(path: &Path) -> Result<()> {
        // Unified size validation
    }
}
```

### Medium Priority Refactoring

#### 4. Create ConfigLoader Service
```rust
pub struct ConfigLoader;

impl ConfigLoader {
    pub fn load_app_config() -> Result<AppConfig> {
        // Unified app config loading
    }

    pub fn load_mcp_config() -> Result<MCPConfig> {
        // Unified MCP config loading
    }
}
```

#### 5. Create ErrorMessageBuilder
```rust
pub struct ErrorMessageBuilder;

impl ErrorMessageBuilder {
    pub fn build(operation: &str, error: &dyn std::error::Error) -> String {
        // Unified error message formatting
    }
}
```

## Expected Benefits

### Code Quality Improvements
- **Reduced Code Size:** ~15-20% reduction in total lines
- **Improved Maintainability:** Single source of truth for common operations
- **Better Consistency:** Uniform behavior across modules
- **Easier Testing:** Centralized logic enables comprehensive testing

### Development Efficiency
- **Faster Development:** Reuse existing utilities
- **Fewer Bugs:** Eliminate copy-paste errors
- **Easier Refactoring:** Changes in one place affect all usages
- **Better Onboarding:** Clearer code organization

## Implementation Plan

### Phase 1 (Immediate)
1. Extract message conversion utilities
2. Create tool execution service
3. Implement file validation utilities

### Phase 2 (Short-term)
1. Refactor configuration loading
2. Standardize error message formatting
3. Update all call sites to use new utilities

### Phase 3 (Long-term)
1. Add comprehensive tests for new utilities
2. Document new service interfaces
3. Monitor for new duplication patterns

## DRY Compliance Score

**Current Score: 4/10** - Significant code duplication across modules
**Target Score: 8/10** - After implementing recommended refactoring

**Estimated Effort:** 2-3 weeks of focused refactoring work