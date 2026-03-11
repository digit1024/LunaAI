# Conversation Engine Architecture

The server uses **Rig** ([rig-core](https://crates.io/crates/rig-core)) as the conversation engine. It runs the LLM/tool loop and broadcasts `ServerEvent`s to subscribers (wire protocol: `serverdocs/serverspec.md`).

## Flow

```
Client (WebSocket) → handle_send_message → engine.run_turn()
                                              ↓
                                    RigEngine
                                              ↓
                                    rig_core pipeline (run_turn_streaming)
                                              ↓
                                    Rig agent + MCP + internal tools
```

## Rig Engine

- **Pipeline** (`src/rig_core/pipeline.rs`): `run_turn_streaming()` with `.multi_turn(10)` for tool loops. Uses OpenAI-compatible Chat Completions API; **custom endpoints** (DeepSeek, OpenRouter, etc.) are supported via `preset.endpoint` in config.
- **Tools** (`src/server/rig_tools.rs`): MCP tools via `MCPToolWrapper`; internal tools (schedule_task, store_memory, search_memory, etc.) implement Rig `ToolDyn` and emit `tool_started`, `tool_result`, `tool_error`.
- **Adapters** (`src/rig_core/adapters.rs`): Luna messages ↔ Rig history.
