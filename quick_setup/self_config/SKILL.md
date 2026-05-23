---
name: self_config
description: Self-administration and configuration of Luna AI. Manage config files, memory/embedding/deep sleep, restart service, troubleshoot. Activate ONLY when user explicitly requests configuration changes or system administration tasks.
allowed-tools:
  - shell_execute
  - read_file
  - write_file
  - modify_file
  - copy_file
license: MIT
---

# Luna Self-Configuration & Administration

Modify Luna's **configuration** (not Rust source), manage the **user systemd service**, and troubleshoot memory, MCP, and embeddings. **Use with caution—you are modifying runtime behavior.**

## When to Use This Skill

**Activate ONLY when:**

1. User explicitly asks to change config, profiles, MCP, memory, or service
2. System administration (restart, status, logs, backups)
3. Fixing broken `config.toml` / `mcp_config.json` after manual edits

**DO NOT activate for:** normal chat, unauthorized changes, or editing `$HOME/proj/LunaAI/src/`.

---

## Quick setup baseline

Fresh installs should run **`luna_ai_quick_setup`** (or `python -m quick_setup.main` from `quick_setup/`). That writes:

- `[model_presets.*]`, `[profiles.*]`, `[server]`
- `[tools_policies.default]` with **`enabled_mcp`** matching selected MCP servers
- `[embedding]`, `[deep_sleep]`, `[title_summary]` when Full Luna is enabled (default **yes**)
- `mcp_config.json` + optional `cosmic-llm-memory` binary
- `self_config` skill (this file) under `skills/self_config/`

See `quick_setup/docs/QUICK_SETUP.md` and `quick_setup/docs/MEMORY.md`.

---

## Memory (three layers)

| Layer | What | Config |
|-------|------|--------|
| **Facts + keyword search** | `store_memory`, `search_memory`, `search_memory_by_category` | Internal tools; SQLite `memory` + FTS |
| **Semantic recall (RAG)** | Injects relevant memories each turn | `[embedding] enabled = true` |
| **History MCP** | Search past conversations via MCP | `cosmic-llm-memory` in `mcp_config.json` **and** `enabled_mcp` |
| **Deep sleep** | Background summarize / prune / extract | `[deep_sleep] enabled = true`, `profile = "..."` |

**Common mistake:** MCP servers listed in `mcp_config.json` but **`enabled_mcp = []`** in `tools_policies` — agent gets zero MCP tools. Fix:

```toml
[tools_policies.default]
enabled_mcp = ["shell", "filesystem", "fetch", "skills", "markitdown", "cosmic-llm-memory"]
enabled_tools = ["*"]
```

Manual checks:

```bash
cosmic_llm --deep-sleep              # one deep sleep cycle
cosmic_llm --reorganize-memories     # rebuild memory_vec (needs [embedding])
```

---

## Safety protocols

1. **Backup** before editing `config.toml` or `mcp_config.json`
2. **Never modify** `$HOME/proj/LunaAI/src/` or Cargo files
3. **Confirm** before restart (ends active WebSocket sessions)

```bash
cp $HOME/.local/share/cosmic_llm/config.toml \
   $HOME/.local/share/cosmic_llm/config.toml.backup.$(date +%Y%m%d_%H%M%S)
systemctl --user status luna-server.service
```

---

## Configuration files

### `config.toml`

**Path:** `$HOME/.local/share/cosmic_llm/config.toml`

**Shape (config refinement):** profiles reference **model presets** and **tools policies** — not inline `backend`/`model` on profiles.

```toml
default = "luna_gpt5mini"

[model_presets.luna_gpt5mini]
backend = "openai"
model = "gpt-5-mini"
endpoint = "https://api.openai.com/v1/chat/completions"
api_key = "..."
temperature = 0.3
max_tokens = 4000

[tools_policies.default]
enabled_mcp = ["shell", "filesystem", "fetch", "skills", "markitdown", "cosmic-llm-memory"]
enabled_tools = ["*"]
disabled_tools = []

[profiles.luna_gpt5mini]
model_preset = "luna_gpt5mini"
prompts = ["profiles/luna_gpt5mini.md"]
tools_policy = "default"
summarize_threshold = 0.7
hidden = false

[embedding]
enabled = true
endpoint = "https://api.openai.com/v1/embeddings"
model = "text-embedding-3-small"
dimensions = 1536
api_key = "..."

[deep_sleep]
enabled = true
profile = "luna_gpt5mini"

[title_summary]
title_generation_profile = "luna_gpt5mini"

[server]
enabled = true
host = "0.0.0.0"
port = 8080
api_key = ""
```

Global system prompt: `$HOME/.local/share/cosmic_llm/system_prompt.md` (default path; optional `[prompts]` section).

Thin UI: `$HOME/.config/luna_thin_ui/server_config.toml`

### `mcp_config.json`

**Path:** `$HOME/.local/share/cosmic_llm/mcp_config.json`

**Quick-setup catalog servers (typical):**

| id | Purpose |
|----|---------|
| `shell` | Whitelisted commands (`uvx mcp-shell-server`) |
| `filesystem` | Home directory (`npx @modelcontextprotocol/server-filesystem`) |
| `fetch` | URL → markdown |
| `skills` | `skills/` folder (`agent-skills-mcp`) |
| `markitdown` | Office/PDF/URL → markdown |
| `cosmic-llm-memory` | Luna history binary (`mcp_luna_history`, `COSMIC_LLM_DB_PATH`) |

Example:

```json
{
  "mcpServers": {
    "shell": {
      "command": "uvx",
      "args": ["mcp-shell-server"],
      "env": { "ALLOW_COMMANDS": "ls,cat,..." }
    },
    "cosmic-llm-memory": {
      "command": "/home/USER/.local/share/cosmic_llm/bin/mcp_luna_history",
      "args": [],
      "env": { "COSMIC_LLM_DB_PATH": "/home/USER/.local/share/cosmic_llm/conversations.db" }
    }
  }
}
```

Restart server after MCP JSON changes.

---

## Database

**Path:** `$HOME/.local/share/cosmic_llm/conversations.db`

| Object | Role |
|--------|------|
| `conversations`, `messages` | Chat history |
| `memory` | Curated facts |
| `memory_fts` | Keyword search |
| `memory_vec` | Embeddings (if `[embedding]` enabled) |
| `deep_sleep_state` | Maintenance checkpoints |

```bash
sqlite3 $HOME/.local/share/cosmic_llm/conversations.db \
  "SELECT COUNT(*) FROM memory;"
sqlite3 $HOME/.local/share/cosmic_llm/conversations.db \
  "PRAGMA integrity_check;"
```

Stop service before manual DB surgery.

---

## Service management

**Unit:** `$HOME/.config/systemd/user/luna-server.service` (installed by quick setup)

```bash
systemctl --user status luna-server.service
systemctl --user restart luna-server.service
journalctl --user -u luna-server.service -n 50
loginctl enable-linger $USER    # run at boot without login
```

Config/MCP changes usually need **restart**. System prompt file changes apply on next message.

---

## Common tasks

### Enable MCP servers for the default profile

Edit **`[tools_policies.default]`** (not the profile block):

```toml
[tools_policies.default]
enabled_mcp = ["filesystem", "shell", "cosmic-llm-memory"]
enabled_tools = ["*"]
```

### Add a new LLM profile

1. Backup `config.toml`
2. Add `[model_presets.name]` and `[profiles.name]` with `model_preset = "name"`, `tools_policy = "default"`
3. Optional: `profiles/name.md` under `profiles/`
4. Set `default = "name"` if desired
5. Restart service

### Enable or fix memory RAG

```toml
[embedding]
enabled = true
endpoint = "https://api.openai.com/v1/embeddings"
model = "text-embedding-3-small"
dimensions = 1536
api_key = "sk-..."
```

Restart; verify logs: `Embedding enabled for memory vector search`.

### Enable deep sleep

```toml
[deep_sleep]
enabled = true
profile = "your_profile_name"
```

### Re-run quick setup (merge)

```bash
luna_ai_quick_setup
# or: cd $HOME/proj/LunaAI/quick_setup && python -m quick_setup.main
```

MCP merge adds missing servers; `finalize` updates `enabled_mcp` and Full Luna blocks.

---

## Troubleshooting

```bash
# Config syntax
python3 -c "import tomllib; tomllib.load(open('$HOME/.local/share/cosmic_llm/config.toml','rb'))"
python3 -c "import json; json.load(open('$HOME/.local/share/cosmic_llm/mcp_config.json'))"

# Logs: MCP failed, embedding, deep sleep
journalctl --user -u luna-server.service -n 100 | grep -iE 'error|mcp|embedding|deep.sleep'
```

| Symptom | Likely cause |
|---------|----------------|
| MCP tools never appear | `enabled_mcp` empty or wrong server id |
| No semantic memory recall | `[embedding] enabled = false` or bad API key |
| Deep sleep never runs | `[deep_sleep] enabled = false` or missing `profile` |
| History MCP useless | Binary missing or not in `enabled_mcp` |

---

## File locations

| Component | Path |
|-----------|------|
| Main config | `$HOME/.local/share/cosmic_llm/config.toml` |
| MCP config | `$HOME/.local/share/cosmic_llm/mcp_config.json` |
| System prompt | `$HOME/.local/share/cosmic_llm/system_prompt.md` |
| Database | `$HOME/.local/share/cosmic_llm/conversations.db` |
| Memory MCP binary | `$HOME/.local/share/cosmic_llm/bin/mcp_luna_history` |
| Profiles / skills | `profiles/`, `skills/` under cosmic_llm |
| Thin UI | `$HOME/.config/luna_thin_ui/server_config.toml` |
| Service unit | `$HOME/.config/systemd/user/luna-server.service` |
| Source (read-only) | `$HOME/proj/LunaAI/` |

---

## Notes

- `tools_policies` control MCP + internal tools; profiles only **reference** a policy name.
- Embedding uses **OpenAI-compatible** API even when chat uses Claude/Gemini.
- When in doubt, **ask the user** before destructive changes.
