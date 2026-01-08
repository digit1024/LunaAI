# ThinUI Analysis & Implementation Plan

## 🎯 Goal
Create a thin UI client that uses server connectivity (WS/HTTP) instead of direct LLM/storage/MCP access, similar to the mobile app.

## ✅ Requirements
1. ✅ Copy src folder and adjust Cargo.toml
2. ✅ Use WS/HTTP instead of direct connectivity
3. ✅ Keep TTS/STT as-is (D-Bus, independent)
4. ✅ Block MCP config page completely (not supported by server)
5. ✅ Remove manual summarization (not supported by server)
6. ✅ Simplify settings page to only connectivity (like mobile setup page)

## 📋 Analysis Results

### Current Architecture

#### Desktop App (Current)
- **Direct LLM Access**: Uses `llm_client` directly (OpenAI, Anthropic, etc.)
- **Local Storage**: SQLite database (`~/.local/share/cosmic_llm/conversations.db`)
- **MCP Registry**: Local MCP registry managing tools
- **Context Service**: Local summarization and context management
- **TTS/STT**: D-Bus integration (independent, stays)

#### Server Protocol (Available)
- **WebSocket** (port 8080): All commands/events
- **HTTP** (port 8081): File attachments only
- **Commands**: `HealthCheck`, `StartConversation`, `LoadConversation`, `ListConversations`, `DeleteConversation`, `StopStreaming`, `ChangeProfile`, `ListProfiles`, `SendMessage`
- **Events**: `HealthOk`, `Error`, `ConversationCreated`, `ConversationLoaded`, `ConversationsList`, `ProfileChanged`, `ProfilesList`, `MessageAccepted`, `StreamingStarted`, `AssistantDelta`, `AssistantComplete`, `ToolPlanned`, `ToolStarted`, `ToolResult`, `ToolError`, `ConversationComplete`, `StreamingStopped`

### Missing Features (Not in Server Protocol)
1. ❌ **Manual Summarization** - No `ManualSummarize` command
2. ❌ **MCP Tool Management** - No commands to list/toggle tools
3. ❌ **Settings Management** - No commands to read/write config

### Implementation Plan

#### Phase 1: Project Setup
- [x] Copy `src/` to new location (or create feature flag)
- [ ] Create new `Cargo.toml` with adjusted dependencies
- [ ] Remove dependencies: Direct LLM clients can stay for reference, but won't be used
- [ ] Add dependencies: WebSocket client, HTTP client (already in Cargo.toml via tokio-tungstenite, reqwest)

#### Phase 2: WebSocket Client
- [ ] Create `src/client/ws_client.rs` (similar to mobile `LunaWsClient`)
- [ ] Implement connection management
- [ ] Implement event streaming
- [ ] Implement command sending

#### Phase 3: HTTP Client
- [ ] Create `src/client/http_client.rs` for file attachments
- [ ] Implement file upload (`POST /api/attach-file`)
- [ ] Implement file deletion (`DELETE /api/attach-file/:file_id`)

#### Phase 4: Remove Direct Access
- [ ] Remove/disable direct LLM calls from handlers
- [ ] Remove/disable direct storage access
- [ ] Remove/disable direct MCP registry access
- [ ] Replace with WebSocket commands

#### Phase 5: UI Changes
- [ ] **Remove Manual Summarization**:
  - Remove `Message::ManualSummarize` from app.rs
  - Remove `perform_manual_summarization` method
  - Remove menu item/button
  - Remove `src/services/context_service.rs::perform_manual_summarization` usage

- [ ] **Block MCP Config Page**:
  - Remove `NavigationPage::MCPConfig` or disable navigation
  - Show "Not supported in server mode" message
  - Remove MCP config from settings page

- [ ] **Simplify Settings Page**:
  - Remove LLM profile management (server manages profiles)
  - Remove MCP server configuration
  - Keep only:
    - Server host
    - Server port
    - API key
    - Theme preference (local)
    - Voice settings (TTS/STT - local)

#### Phase 6: Keep TTS/STT
- [ ] Verify D-Bus integration remains untouched
- [ ] Ensure TTS/STT works independently (it does, via D-Bus)

## 🔍 Missing Something?

### Potential Issues

1. **File Attachments**: 
   - ✅ Current: Direct file access via file picker
   - ✅ Server: HTTP endpoint for file upload
   - ⚠️ Need to: Upload file to server first, then include `attachment_ids` in `SendMessage`

2. **Profile Switching**:
   - ✅ Current: Direct config access, switch profiles locally
   - ✅ Server: `ChangeProfile` command, `ProfileChanged` event
   - ⚠️ Need to: Store current profile in UI state, sync with server

3. **Conversation Loading**:
   - ✅ Current: Load from local SQLite
   - ✅ Server: `LoadConversation` command, `ConversationLoaded` event
   - ⚠️ Need to: Replace local storage access with server commands

4. **Search**:
   - ✅ Current: Local SQLite search
   - ✅ Server: `ListConversations { query }` command
   - ⚠️ Need to: Use server search instead

5. **Context State**:
   - ✅ Current: Local context management, manual summarization
   - ✅ Server: Automatic summarization (no manual trigger)
   - ⚠️ Need to: Remove manual controls, rely on server

6. **Navigation**:
   - ✅ Current: Local conversation list
   - ✅ Server: `ListConversations` command, `ConversationsList` event
   - ⚠️ Need to: Refresh from server

7. **Streaming**:
   - ✅ Current: Direct LLM streaming subscription
   - ✅ Server: `SendMessage` → `StreamingStarted` → `AssistantDelta` → `AssistantComplete`
   - ⚠️ Need to: Replace subscription with WebSocket events

8. **Tool Execution**:
   - ✅ Current: Local MCP registry, direct tool calls
   - ✅ Server: `ToolPlanned` → `ToolStarted` → `ToolResult` events
   - ⚠️ Need to: Display tools but don't manage them

9. **Error Handling**:
   - ✅ Current: Direct error handling
   - ✅ Server: `Error` event
   - ⚠️ Need to: Handle server errors

10. **Connection State**:
    - ✅ Current: Always connected (direct access)
    - ✅ Server: Connection state management (connecting, online, error)
    - ⚠️ Need to: Show connection status, handle reconnection

## ✅ Conclusion

**Analysis is complete!** The plan covers all requirements:

1. ✅ Server connectivity instead of direct access
2. ✅ TTS/STT remains independent (D-Bus)
3. ✅ MCP config blocked (not in server protocol)
4. ✅ Manual summarization removed (not in server protocol)
5. ✅ Settings simplified to connectivity only (like mobile)

**No missing pieces identified** - ready to implement!






