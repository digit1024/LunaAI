# Luna AI Quick Setup – Documentation

## Goal

One-shot, **opinionated** setup so a user can quickly start using Luna AI: configure **server** (`config.toml`) and **thin_ui** (`server_config.toml`), plus optional MCP and personas.

## What gets configured

| Target | Content |
|--------|--------|
| `~/.local/share/cosmic_llm/config.toml` | `default`, `[profiles.<name>]`, `[server]` |
| `~/.local/share/cosmic_llm/system_prompt.md` | Global system prompt (from chosen persona) |
| `~/.local/share/cosmic_llm/profiles/<name>.md` | Per-profile prompt (same persona content) |
| `~/.config/luna_thin_ui/server_config.toml` | `host`, `port`, `api_key` for thin UI |
| `~/.local/share/cosmic_llm/mcp_config.json` | `mcpServers` (memory, shell, skills, filesystem, fetch, brave-search) |

## Flow (main CLI)

1. **Provider** – Choose: OpenAI, Claude, Gemini, DeepSeek, OpenRouter.
2. **Model** – Pick from catalog; default = “fast & cheap but not the fastest/cheapest” (e.g. GPT-4o Mini, Claude 3.5 Haiku, Mixtral 8x7B for OpenRouter).
3. **API key** – Prompt (hidden input).
4. **Personality** – 1 = Luna (sarcastic helpful), 2 = Vera (professional concise), 3 = Jude (wicked funny). Installs system prompt and profile prompt.
5. **Temperature** – Default 0.3.
6. **Max tokens** – Default 4000.
7. **Profile name** – Suggested: `{persona}_{model}` (e.g. `luna_gpt-4o-mini`).
8. **Profile creation** – Writes `[profiles.<name>]` with backend, api_key, model, endpoint, temperature, max_tokens, context_window_size, summarize_threshold, profile_prompt_file, hidden.
9. **Server API** – Host (default 0.0.0.0), port (8080), server API key. Writes `[server]` and thin_ui `server_config.toml`.
10. **MCP** – Ask to add default servers; if yes, derive paths and write `mcp_config.json`.

## Catalog (`catalog/providers_models.json`)

- **providers**: `openai`, `claude`, `gemini`, `deepseek`, `openrouter`.
- Each has: `label`, `backend`, `endpoint`, `default_model_id`, `models[]` with `id`, `label`, `context_window`, `note`.
- Backend/endpoint align with Luna’s Rust backends (openai, anthropic, gemini, deepseek; openrouter uses OpenAI-compatible client with OpenRouter endpoint).

## Sample data (`sample_data/personas/`)

- **luna.md** – Luna AI, sarcastic helpful.
- **vera.md** – Vera, professional concise.
- **jude.md** – Jude, wicked twisted funny.

## Profile TOML shape

```toml
[profiles.<name>]
backend = "..."
api_key = "..."
model = "..."
endpoint = "..."
temperature = 0.3
max_tokens = 4000
context_window_size = 128000   # when known from catalog
summarize_threshold = 0.7
profile_prompt_file = "profiles/<name>.md"
hidden = false
```

## Server TOML shape

```toml
[server]
enabled = true
host = "0.0.0.0"
port = 8080
api_key = "..."
stream_timeout_secs = 300
healthcheck_interval_secs = 30
wal_enabled = true
wal_autocheckpoint = 200
sqlite_busy_timeout_ms = 5000
```

## MCP default servers

| Server | Purpose | Command / env |
|--------|--------|----------------|
| shell | Safe shell commands | `uvx mcp-shell-server`, `ALLOW_COMMANDS` |
| cosmic-llm-memory | Luna history + memory | `mcp_luna_history`, `COSMIC_LLM_DB_PATH`. Binary: [releases](https://github.com/digit1024/mcp_luna_memory/releases/download/1.0/mcp_luna_history) |
| skills | Agent skills | `uvx agent-skills-mcp`, `SKILL_FOLDER`, `MODE=tool` |
| filesystem | Read/write files | `npx @modelcontextprotocol/server-filesystem`, home dir |
| fetch | Fetch URL → markdown | `uvx mcp-server-fetch` |
| brave-search | Web search | `npx @modelcontextprotocol/server-brave-search`, `BRAVE_API_KEY` |

Paths (binary for memory, DB, skills dir) are derived from `~/.local/share/cosmic_llm` and optional prompts.

## Architecture (modular)

- **paths** – All cosmic_llm and luna_thin_ui paths in one place.
- **profile_creator** – Build profile dict, load/save config.toml, add_or_update_profile.
- **server_config** – Merge [server] into config.toml; write thin_ui server_config.toml.
- **mcp_config** – default_mcp_servers(), load/save mcp_config.json, merge_default_mcp_servers().
- **prompts** – Persona list, install_system_prompt, install_persona_as_profile_prompt.
- **main** – Load catalog, ask questions, call modules, write all configs.

So: UI flow and “what to ask” live in `main`; “how to build and write config” lives in the other modules.

## What’s missing from a configuration perspective (self-review)

- **title_summary** – Not in quick setup. User may want `title_generation_profile` and related options later; could be a follow-up step or doc.
- **enabled_mcp** – We don’t set per-profile `enabled_mcp`; profile gets empty list. User can edit config to add e.g. `enabled_mcp = "cosmic-llm-memory,fetch"`.
- **prompts section** – We don’t write `[prompts] system_prompt_file` in config.toml; Luna still finds `system_prompt.md` in the config dir by convention. If Luna ever requires it in config, we should add it.
- **Brave API key** – We add brave-search with empty key; user must edit `mcp_config.json` or we could add one more question.
- **Optional MCP toggles** – We don’t ask “which of these MCP servers to enable”; we add all and user can remove from JSON. Acceptable for “quick” setup.
- **OpenRouter** – Luna uses OpenAI client for unknown backends; we use backend `openrouter` and endpoint `https://openrouter.ai/api/v1`. Tokenizer will use 4096 context unless we set `context_window_size` (we do from catalog).

## Running

From repo (LunaAI):

```bash
cd quick_setup
pip install -e .
python -m quick_setup.main
# or
luna-quick-setup
```

Requires: Python 3.10+, `toml` (pip install toml).
