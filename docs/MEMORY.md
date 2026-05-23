# Luna memory (overview)

Luna combines **curated memory tools**, **embedding-based RAG**, **conversation history MCP**, and **deep sleep** background maintenance.

Full documentation (setup defaults, config snippets, troubleshooting):

**[quick_setup/docs/MEMORY.md](../quick_setup/docs/MEMORY.md)**

Quick reference:

| Feature | Config |
|---------|--------|
| Keyword memory tools | Always available if `enabled_tools` allows |
| Semantic recall | `[embedding] enabled = true` |
| History search MCP | `cosmic-llm-memory` in `mcp_config.json` + `enabled_mcp` |
| Background upkeep | `[deep_sleep] enabled = true`, `profile = "..."` |
| Deep sleep LLM prompts | Optional `[deep_sleep] summarize_prompt`, `evaluate_prompt`, `extract_prompt` (built-in defaults if omitted) |

Bootstrap with `luna_ai_quick_setup` — Full Luna is **on by default**.
