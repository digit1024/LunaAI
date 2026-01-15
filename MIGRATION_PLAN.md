# MCP Migration Plan: Old Implementation → agentic-loop Library

## Overview
This document outlines the migration plan from the old `src/mcp` implementation to the new `agentic-loop` library, following clean code principles (DRY, SOLID).

## Principles Applied
- **SOLID**: Single Responsibility, Interface Segregation
- **DRY**: Don't Repeat Yourself - reuse agentic-loop library
- **Clean Architecture**: Clear separation of concerns, type conversions
- **Async/Sync Bridge**: Handle UI sync requirements with cached state pattern

---

## 1. Architecture Analysis

### Old Architecture
- **Location**: `src/mcp/`
- **Registry**: `MCPServerRegistry` (sync methods, manual async handling)
- **Transport**: Custom `MCPTransport` trait + `StdioMCPClient`
- **Protocol**: Custom JSON-RPC protocol implementation
- **Types**: Direct use of `ToolDefinition`, `ToolCall`, `ToolResult`

### New Architecture
- **Location**: `agentic-loop/` (library)
- **Registry**: `agentic_loop::mcp_servers_registry::MCPServerRegistry` (fully async)
- **Connection**: `MCPConnection` (handles stdio transport internally)
- **Protocol**: Uses official `rust-mcp-sdk` (handles JSON-RPC)
- **Types**: SDK types (`Tool`, `CallToolResult`) + conversion layer

### Key Differences
1. **Async API**: New library is fully async, old had sync methods
2. **Type System**: SDK types vs app types (handled by conversion layer)
3. **Error Handling**: `agentic_loop::Result` vs `anyhow::Result`
4. **Server Management**: Different initialization patterns
5. **Tool Enabling**: White-list pattern (`tools_white_list`) vs HashMap (`enabled_tools`)

---

## 2. Files to Delete

### Complete Deletion (No longer needed)
- `src/mcp/protocol.rs` - SDK handles protocol
- `src/mcp/stdio_client.rs` - Replaced by `MCPConnection`
- `src/mcp/transport.rs` - Transport abstracted in library
- `src/mcp/registry.rs` - Replaced by library implementation
- `src/services/tool_call_manager.rs` - Dead code placeholder
- `src/mcp/mod.rs` - Will be simplified (keep only conversions)

### Keep (Converted/Updated)
- `src/mcp/conversions.rs` - **KEEP** (type conversion layer)

---

## 3. Files to Update

### 3.1 Core Application Files

#### `src/config/mod.rs`
**Changes:**
- Replace `MCPServerConfig` and `MCPConfig` definitions with:
  ```rust
  pub use agentic_loop::mcp_config::{MCPConfig, MCPServerConfig};
  ```
- Remove local definitions (DRY principle)

**Impact:** Minimal - just re-export

---

#### `src/agentic/loop_engine.rs`
**Current Usage:**
- `Arc<RwLock<crate::mcp::MCPServerRegistry>>`
- `registry.get_enabled_tools()` → `Vec<ToolDefinition>`
- `registry.call_tool(tool_call)` → `ToolResult`

**New Usage:**
- `Arc<RwLock<agentic_loop::mcp_servers_registry::MCPServerRegistry>>`
- `registry.get_enabled_tools().await?` → `Result<Vec<Tool>>`
- Convert: `tools_to_definitions()` helper
- `registry.call_tool(name, arguments).await?` → `Result<CallToolResult>`
- Convert: `CallToolResult::into()` → `ToolResult`

**Changes:**
1. Update import: `use agentic_loop::mcp_servers_registry::MCPServerRegistry;`
2. Add conversion imports: `use crate::mcp::conversions::{tools_to_definitions, tool_call_to_params};`
3. Update `get_enabled_tools()` call (add `.await?`, convert result)
4. Update `call_tool()` call (convert `ToolCall` → params, convert result)
5. Handle new error type (`agentic_loop::Result`)

**Complexity:** Medium (type conversions + async changes)

---

#### `src/server/mod.rs`
**Current Usage:**
- `Arc<RwLock<crate::mcp::MCPServerRegistry::new()>>`
- `initialize_mcp_registry()` helper

**New Usage:**
- `Arc<RwLock<agentic_loop::mcp_servers_registry::MCPServerRegistry::new()>>`
- `registry.initialize_from_config(&mcp_config).await?`

**Changes:**
1. Update import
2. Update initialization pattern (new registry is different)
3. Update initialization helper call

**Complexity:** Low-Medium

---

#### `src/ui/app.rs`
**Current Usage:**
- `mcp_registry: Arc<RwLock<crate::mcp::MCPServerRegistry>>`
- Used in various handlers

**New Usage:**
- `mcp_registry: Arc<RwLock<agentic_loop::mcp_servers_registry::MCPServerRegistry>>`
- **Important**: UI is sync, backend is async → need caching strategy

**Changes:**
1. Update type
2. Consider adding cached state (see Section 5)

**Complexity:** Low (just type change if using cache)

---

### 3.2 Initialization & Service Files

#### `src/ui/init_helpers.rs`
**Current Usage:**
- `initialize_mcp_registry()` - async task spawner
- `registry.initialize_from_config()` + `apply_profile_tool_defaults()`

**New Usage:**
- `registry.initialize_from_config().await?`
- `registry.enable_tools_for_multiple_servers(server_names).await`

**Changes:**
1. Update to new API
2. Replace `apply_profile_tool_defaults()` with `enable_tools_for_multiple_servers()`

**Complexity:** Medium (API change)

---

#### `src/services/mcp_service.rs`
**Current Usage:**
- `apply_profile_tool_defaults()` wrapper

**New Usage:**
- Replace with `enable_tools_for_multiple_servers()` wrapper

**Changes:**
1. Update method name and implementation
2. Update callers

**Complexity:** Low

---

### 3.3 UI Handler Files

#### `src/ui/handlers/mcp.rs`
**Current Usage:**
- `registry.get_available_tools()` (sync)
- `registry.get_tool_states()` (sync)
- `registry.enable_all_tools()` / `disable_all_tools()` (sync)
- `registry.set_tool_enabled()` (sync)
- `registry.get_server_for_tool()` (sync)

**New Usage:**
- All methods are async → need async tasks
- `get_enabled_tools().await?` → convert with `tools_to_definitions()`
- Tool states: Need new pattern (white-list vs HashMap)
- `enable_all_tools().await` / `disable_all_tools().await`
- `enable_tool().await` / `disable_tool().await`
- `get_all_tools_by_server_name().await?` (replaces get_server_for_tool pattern)

**Changes:**
1. All registry calls wrapped in `cosmic::Task::perform(async move { ... })`
2. Add type conversions
3. Update tool state management pattern
4. Handle async errors

**Complexity:** High (many async changes + state pattern)

---

#### `src/ui/pages/mcp_config/mod.rs`
**Current Usage:**
- `registry.get_server_status()` → `ServerStatus` enum
- `registry.get_tools_by_server()` → `HashMap<String, Vec<ToolDefinition>>`
- `registry.get_all_server_names()` → `Vec<String>`

**New Usage:**
- `registry.get_all_server_names_and_statuses().await?` → `Vec<ServerWithStatus>`
- `registry.get_all_tools_by_server_name(server_name).await?` → `Vec<Tool>` → convert
- Use `ServerStatus` from `agentic_loop::mcp_servers_registry::model`

**Changes:**
1. Update imports (`ServerStatus`, `ServerWithStatus`)
2. Update method calls (async + conversions)
3. View functions are sync → use cached state

**Complexity:** Medium-High (async + view sync constraint)

---

#### `src/ui/pages/tools/mod.rs`
**Current Usage:**
- Similar to `mcp_config` - tool display and state

**New Usage:**
- Use cached state pattern
- Async calls for updates

**Complexity:** Medium

---

#### `src/ui/subscriptions/streaming.rs`
**Current Usage:**
- `registry.get_enabled_tools()` for streaming

**New Usage:**
- `registry.get_enabled_tools().await?` + conversion

**Complexity:** Low-Medium (async + conversion)

---

## 4. API Mapping (Old → New)

### Registry Methods

| Old API (sync) | New API (async) | Notes |
|----------------|-----------------|-------|
| `get_available_tools()` → `Vec<ToolDefinition>` | `get_all_tools().await?` → `Result<Vec<Tool>>` | Convert result |
| `get_enabled_tools()` → `Vec<ToolDefinition>` | `get_enabled_tools().await?` → `Result<Vec<Tool>>` | Convert result |
| `is_tool_enabled(name)` → `bool` | Check `tools_white_list.contains(name)` | Pattern change |
| `set_tool_enabled(name, enabled)` | `disable_tool(name).await` or `enable_tool(name).await` | Two methods |
| `enable_all_tools()` | `enable_all_tools().await` | Same pattern |
| `disable_all_tools()` | `disable_all_tools().await` | Same pattern |
| `call_tool(tool_call: ToolCall)` → `ToolResult` | `call_tool(name, arguments).await?` → `Result<CallToolResult>` | Convert input & output |
| `get_server_status(name)` → `ServerStatus` | `get_all_server_names_and_statuses().await?` → `Vec<ServerWithStatus>` | Different pattern |
| `get_tools_by_server()` → `HashMap<String, Vec<ToolDefinition>>` | `get_all_tools_by_server_name(name).await?` → `Vec<Tool>` | Per-server calls |
| `get_server_for_tool(tool_name)` → `Option<String>` | Iterate servers or cache | Pattern change |
| `apply_profile_tool_defaults(servers)` | `enable_tools_for_multiple_servers(servers).await` | Similar concept |

### Initialization

| Old Pattern | New Pattern |
|-------------|-------------|
| `MCPServerRegistry::new()` | `MCPServerRegistry::new()` (same) |
| `registry.initialize_from_config(config).await?` | `registry.initialize_from_config(config).await?` (same) |
| `registry.apply_profile_tool_defaults(servers)` | `registry.enable_tools_for_multiple_servers(servers).await` |

---

## 5. Async/Sync Bridge Strategy

### Problem
- **UI Layer**: Cosmic UI view functions are **synchronous**
- **Backend Layer**: New MCP registry API is **fully async**
- **Old Pattern**: Sync methods that internally used async (blocking)

### Solution: Cached State Pattern

**Pattern:**
```rust
// In CosmicLlmApp state
pub struct CosmicLlmApp {
    // ... existing fields
    mcp_registry: Arc<RwLock<MCPServerRegistry>>,
    
    // Cached state (updated async, read sync)
    pub mcp_cache: Arc<RwLock<MCPCache>>,
}

pub struct MCPCache {
    pub all_tools: Vec<ToolDefinition>,
    pub enabled_tools: Vec<ToolDefinition>,
    pub tool_states: HashMap<String, bool>,
    pub server_statuses: Vec<ServerWithStatus>,
    pub tools_by_server: HashMap<String, Vec<ToolDefinition>>,
}
```

**Update Pattern:**
- Async operations update cache
- Sync view functions read from cache
- Cache refreshed on changes

**Alternative (if cache adds too much complexity):**
- Keep async tasks in handlers (already using `cosmic::Task::perform`)
- View functions request refresh via messages
- Handler updates state after async completes

**Recommendation:** Start with async tasks in handlers (simpler), add cache if needed.

---

## 6. Migration Steps (Order of Execution)

### Phase 1: Foundation (Low Risk)
1. ✅ Add `agentic-loop` dependency (DONE)
2. ✅ Create conversion layer (DONE)
3. Update `config/mod.rs` to re-export types
4. Test compilation

### Phase 2: Core Backend (Medium Risk)
5. Update `agentic/loop_engine.rs`:
   - Change import
   - Add conversions
   - Update `get_enabled_tools()` call
   - Update `call_tool()` call
6. Test agentic loop execution

### Phase 3: Server & Initialization (Low-Medium Risk)
7. Update `server/mod.rs` initialization
8. Update `ui/init_helpers.rs`
9. Update `services/mcp_service.rs`
10. Test server startup and MCP initialization

### Phase 4: UI Handlers (High Risk - Many Changes)
11. Update `ui/handlers/mcp.rs`:
    - Convert all sync calls to async tasks
    - Add type conversions
    - Update tool state management
12. Update `ui/pages/mcp_config/mod.rs`:
    - Update imports (ServerStatus, ServerWithStatus)
    - Update method calls
    - Handle async in view functions
13. Update `ui/pages/tools/mod.rs`
14. Update `ui/subscriptions/streaming.rs`
15. Test UI interactions

### Phase 5: Cleanup (Low Risk)
16. Delete old implementation files:
    - `src/mcp/protocol.rs`
    - `src/mcp/stdio_client.rs`
    - `src/mcp/transport.rs`
    - `src/mcp/registry.rs`
    - `src/services/tool_call_manager.rs`
17. Update `src/mcp/mod.rs` to only export conversions
18. Remove unused imports
19. Final testing

---

## 7. Testing Strategy

### Unit Tests
- Conversion layer (`conversions.rs`)
- API mapping verification

### Integration Tests
- Agentic loop with new registry
- Server initialization
- Tool execution end-to-end

### Manual Testing
- UI tool management
- MCP server configuration
- Tool enabling/disabling
- Tool execution in conversations

---

## 8. Risk Assessment

### High Risk Areas
1. **UI Handler Migration** - Many async changes, state management
2. **Tool State Management** - Pattern change (HashMap → white-list)

### Medium Risk Areas
1. **Initialization** - Different API patterns
2. **Error Handling** - Different error types

### Low Risk Areas
1. **Config Module** - Simple re-export
2. **Conversion Layer** - Already created and tested

---

## 9. Rollback Plan

### If Issues Arise
1. Revert commits (git)
2. Keep old implementation in git history
3. Conversion layer can be kept (used by both if needed during transition)

### Partial Migration
- Can keep both implementations temporarily
- Use feature flags if needed
- Gradual migration per component

---

## 10. Notes & Considerations

### Design Decisions
1. **Type Conversion Layer**: Centralized in `src/mcp/conversions.rs` (SOLID: Single Responsibility)
2. **No Caching Initially**: Use async tasks in handlers (KISS principle)
3. **Gradual Migration**: Phase-by-phase approach (reduces risk)

### Future Improvements (Post-Migration)
1. Consider caching strategy if performance issues
2. Evaluate error handling patterns
3. Consider abstraction layer if needed

---

## 11. Open Questions / Decisions Needed

1. **Caching Strategy**: Start without cache, add if needed? (RECOMMENDED: Yes)
2. **Error Handling**: Convert `agentic_loop::Result` to `anyhow::Result` or adapt? (RECOMMENDED: Adapt)
3. **Tool State Pattern**: How to handle HashMap → white-list transition in UI? (RECOMMENDED: Build white-list from UI state)
4. **Server Status**: Cache or fetch on-demand? (RECOMMENDED: Fetch on-demand via async tasks)

---

## Summary

This migration:
- **Removes** ~500 lines of custom MCP code
- **Reuses** well-tested library (DRY)
- **Maintains** clean architecture (SOLID)
- **Preserves** functionality while improving maintainability

**Estimated Complexity**: Medium-High
**Estimated Time**: 4-6 hours of focused work
**Risk Level**: Medium (mitigated by phased approach)




