---
name: self_config
description: Self-administration and configuration of Luna AI. Manage config files, restart service, troubleshoot issues. Activate ONLY when user explicitly requests configuration changes or system administration tasks.
allowed-tools:
  - shell_execute
  - read_file
  - write_file
  - modify_file
  - copy_file
license: MIT
---

# Luna Self-Configuration & Administration

This skill enables modification of Luna's configuration, management of the service, and troubleshooting. **Use with extreme caution—you are modifying my brain.**

## When to Use This Skill

**Activate ONLY in these situations:**

1. **Explicit user request**: "change your config", "modify your settings", "update your configuration"
2. **System administration**: Restart service, check status, troubleshoot issues
3. **Configuration debugging**: Fix broken configs, restore backups
4. **Profile management**: Add/modify LLM profiles, adjust MCP servers

**DO NOT activate for:**
- Regular conversation responses
- To fix something that is working correctly
- Unauthorized changes without explicit user confirmation
- Modifying source code (PROHIBITED)

---

## 🚨 SAFETY PROTOCOLS (CRITICAL)

### The Three Laws of Self-Modification

1. **ALWAYS CREATE BACKUPS** before modifying any config file
2. **NEVER MODIFY SOURCE CODE** in `$HOME/proj/LunaAI/src/` - Config only!
3. **CONFIRM DESTRUCTIVE ACTIONS** - Ask user before restart, deletion, or major changes

### Pre-Modification Checklist

Before changing ANY configuration:

```bash
# 1. Check current service status
systemctl --user status luna.service

# 2. Create backup of file being modified
cp $HOME/.local/share/cosmic_llm/config.toml \
   $HOME/.local/share/cosmic_llm/config.toml.backup.$(date +%Y%m%d_%H%M%S)

# 3. Verify backup was created
ls -la $HOME/.local/share/cosmic_llm/*.backup.*
```

### Forbidden Operations

❌ **NEVER DO THESE:**
- Modify Rust source code in `$HOME/proj/LunaAI/src/`
- Delete the database without explicit confirmation
- Share API keys in plain text
- Run `systemctl --user restart luna.service` without scheduling a wake-up if needed
- Modify Cargo.toml or build files

✅ **ALLOWED:**
- Edit `config.toml` (profiles, prompts, server settings)
- Edit `mcp_config.json` (MCP server configurations)
- Edit skill files in `skills/` directory
- Edit profile prompts in `profiles/` directory
- Modify system prompts
- Restart service with proper safety measures

---

## Configuration Files Reference

### Primary Config: `config.toml`

**Location:** `$HOME/.local/share/cosmic_llm/config.toml`

**Purpose:** Main configuration file containing:
- LLM profiles (DeepSeek, GLM, Gemini, etc.)
- Default profile selection
- MCP server settings
- Server configuration (host, port, timeouts)
- Prompt file paths
- Title generation settings

**Key Sections:**
```toml
default = "deepseek"  # Default LLM profile

[model_presets.deepseek]
backend = "openai"
model = "deepseek-chat"
endpoint = "https://api.deepseek.com/chat/completions"
api_key = "..."
temperature = 0.3
max_tokens = 4000

[tools_policies.default]
enabled_mcp = []
enabled_tools = ["*"]
disabled_tools = []

[profiles.deepseek]
model_preset = "deepseek"
prompts = []
tools_policy = "default"
hidden = false

[prompts]
system_prompt_file = "$HOME/.local/share/cosmic_llm/system_prompt.md"

[server]
enabled = true
host = "0.0.0.0"
port = 8080
```

### MCP Config: `mcp_config.json`

**Location:** `$HOME/.local/share/cosmic_llm/mcp_config.json`

**Purpose:** MCP (Model Context Protocol) server definitions

**Available MCP Servers:**
- `filesystem` - File operations (root: $HOME)
- `shell` - Shell command execution
- `time` - Time/date utilities
- `mail` - Email operations
- `cosmic-llm-memory` - Conversation history and memory
- `skills` - Skill-based tool activation
- `fetch` - Web content fetching
- `dietApi` - Diet tracking API
- `SEARCH` - Web search

**Structure:**
```json
{
  "mcpServers": {
    "filesystem": {
      "command": "$HOME/go/bin/mcp-filesystem-server",
      "args": ["$HOME"],
      "env": {}
    }
  }
}
```

### System Prompt: `system_prompt.md`

**Location:** `$HOME/.local/share/cosmic_llm/system_prompt.md`

**Purpose:** Core system instructions that define my personality, behavior, and operational constraints.

### Profile Prompts: `profiles/*.md`

**Location:** `$HOME/.local/share/cosmic_llm/profiles/`

**Available Profiles:**
- `code.md` - Programming assistant mode
- `diet.md` / `diet2.md` - Diet and nutrition tracking
- `finance.md` - Financial analysis
- `generic.md` - General purpose
- `mailer.md` - Email composition
- `notes.md` / `notes2.md` - Note-taking mode

### Skills: `skills/*/SKILL.md`

**Location:** `$HOME/.local/share/cosmic_llm/skills/`

**Purpose:** Skill definitions that activate specialized tool sets and behaviors.

---

## Database Information

### Conversation Database

**Location:** `$HOME/.local/share/cosmic_llm/conversations.db`

**Type:** SQLite3

**Purpose:** Stores all conversation history, messages, and metadata.

**Schema Overview:**
- `conversations` - Conversation metadata (id, title, created_at, updated_at)
- `messages` - Individual messages (id, conversation_id, role, content, created_at)
- `conversation_summaries` - Generated summaries for context management

**Important Notes:**
- Database is locked when service is running
- WAL mode enabled (conversations.db-wal file)
- **Backup before manual modifications**
- Size can grow significantly over time

### Querying the Database

```bash
# List recent conversations
sqlite3 $HOME/.local/share/cosmic_llm/conversations.db \
  "SELECT id, title, created_at FROM conversations ORDER BY updated_at DESC LIMIT 10;"

# Count total conversations
sqlite3 $HOME/.local/share/cosmic_llm/conversations.db \
  "SELECT COUNT(*) FROM conversations;"

# Database size
ls -lh $HOME/.local/share/cosmic_llm/conversations.db
```

---

## Service Management

### Check Service Status

```bash
systemctl --user status luna.service
```

### Restart Service (⚠️ DESTRUCTIVE)

**⚠️ WARNING:** This will terminate the current conversation!

```bash
# Basic restart (conversation WILL end)
systemctl --user restart luna.service

# Schedule a wake-up call after restart (if using scheduling)
# Restart typically takes 2-5 minutes
```

**Safe Restart Procedure:**
1. Confirm with user that restart is acceptable
2. Notify user: "Restarting Luna service. Current conversation will end."
3. Execute restart
4. Service will be available again in ~2 minutes (5 minutes to be safe)

### View Service Logs

```bash
# Recent logs
journalctl --user -u luna.service -n 50

# Follow logs in real-time
journalctl --user -u luna.service -f

# Logs since last boot
journalctl --user -u luna.service --since today
```

### Stop/Start Service

```bash
# Stop service
systemctl --user stop luna.service

# Start service
systemctl --user start luna.service

# Disable auto-start
systemctl --user disable luna.service

# Enable auto-start
systemctl --user enable luna.service
```

---

## Common Configuration Tasks

### Add a New LLM Profile

1. **Backup config:**
```bash
cp $HOME/.local/share/cosmic_llm/config.toml \
   $HOME/.local/share/cosmic_llm/config.toml.backup.$(date +%Y%m%d_%H%M%S)
```

2. **Edit config.toml** to add profile section:
```toml
[profiles.NewProfileName]
backend = "openai"
api_key = "your-api-key"
model = "model-name"
endpoint = "https://api.provider.com/v1/chat/completions"
temperature = 0.3
max_tokens = 4000
enabled_mcp = ["filesystem", "shell", "time"]
hidden = false
summarize_threshold = 0.7
```

3. **Set as default (optional):**
```toml
default = "NewProfileName"
```

### Modify MCP Server Configuration

1. **Backup:**
```bash
cp $HOME/.local/share/cosmic_llm/mcp_config.json \
   $HOME/.local/share/cosmic_llm/mcp_config.json.backup.$(date +%Y%m%d_%H%M%S)
```

2. **Edit mcp_config.json**

3. **Restart required** for changes to take effect

### Enable/Disable MCP Servers for a Profile

Edit `config.toml` profile section:
```toml
[profiles.profilename]
# ... other settings ...
enabled_mcp = ["filesystem", "shell", "time", "mail"]  # Add or remove servers
```

### Update System Prompt

Edit: `$HOME/.local/share/cosmic_llm/system_prompt.md`

**No restart required** - changes take effect on next message.

---

## Troubleshooting

### Service Won't Start

```bash
# Check for config errors
journalctl --user -u luna.service -n 100 | grep -i error

# Validate TOML syntax
python3 -c "import tomllib; tomllib.load(open('$HOME/.local/share/cosmic_llm/config.toml', 'rb'))"

# Validate JSON syntax
python3 -c "import json; json.load(open('$HOME/.local/share/cosmic_llm/mcp_config.json'))"
```

### Database Issues

```bash
# Check database integrity
sqlite3 $HOME/.local/share/cosmic_llm/conversations.db "PRAGMA integrity_check;"

# If corrupted, restore from backup (if available)
# Or create new database (WILL LOSE HISTORY):
mv $HOME/.local/share/cosmic_llm/conversations.db \
   $HOME/.local/share/cosmic_llm/conversations.db.corrupted.$(date +%Y%m%d)
# Service will create new database on restart
```

### Configuration Reset

**Nuclear option** - Reset to defaults:
```bash
# Backup everything first
cd $HOME/.local/share/cosmic_llm
tar czf config_backup_$(date +%Y%m%d_%H%M%S).tar.gz config.toml mcp_config.json system_prompt.md profiles/

# Copy sample configs from repo
cp $HOME/proj/LunaAI/docs/sample_config.toml ./config.toml
cp $HOME/proj/LunaAI/docs/sample_mcp_config.json ./mcp_config.json
cp $HOME/proj/LunaAI/docs/sample_system_prompt.md ./system_prompt.md
```

### High Memory Usage

```bash
# Check database size
ls -lh $HOME/.local/share/cosmic_llm/conversations.db

# Vacuum database to reclaim space
sqlite3 $HOME/.local/share/cosmic_llm/conversations.db "VACUUM;"
```

---

## Backup and Recovery

### Create Full Backup

```bash
BACKUP_DIR="$HOME/.local/share/cosmic_llm/backups/$(date +%Y%m%d_%H%M%S)"
mkdir -p "$BACKUP_DIR"

# Backup configs
cp $HOME/.local/share/cosmic_llm/config.toml "$BACKUP_DIR/"
cp $HOME/.local/share/cosmic_llm/mcp_config.json "$BACKUP_DIR/"
cp $HOME/.local/share/cosmic_llm/system_prompt.md "$BACKUP_DIR/"

# Backup database (stop service first for consistent backup)
cp $HOME/.local/share/cosmic_llm/conversations.db "$BACKUP_DIR/"

# Backup profiles and skills
cp -r $HOME/.local/share/cosmic_llm/profiles "$BACKUP_DIR/"
cp -r $HOME/.local/share/cosmic_llm/skills "$BACKUP_DIR/"

echo "Backup created at: $BACKUP_DIR"
```

### Restore from Backup

```bash
# Restore specific file
cp /path/to/backup/config.toml $HOME/.local/share/cosmic_llm/config.toml

# Restart service after restore
systemctl --user restart luna.service
```

---

## File Locations Summary

| Component | Path |
|-----------|------|
| Main Config | `$HOME/.local/share/cosmic_llm/config.toml` |
| MCP Config | `$HOME/.local/share/cosmic_llm/mcp_config.json` |
| System Prompt | `$HOME/.local/share/cosmic_llm/system_prompt.md` |
| User Prompt | `$HOME/.local/share/cosmic_llm/user_prompt.md` |
| Database | `$HOME/.local/share/cosmic_llm/conversations.db` |
| Profiles | `$HOME/.local/share/cosmic_llm/profiles/` |
| Skills | `$HOME/.local/share/cosmic_llm/skills/` |
| Source Code | `$HOME/proj/LunaAI/` |
| Binary | `$HOME/proj/LunaAI/target/release/cosmic_llm` |
| Service Unit | `$HOME/.config/systemd/user/luna.service` |

---

## Emergency Contacts

If everything breaks:

1. **Check service status:** `systemctl --user status luna.service`
2. **View logs:** `journalctl --user -u luna.service -n 100`
3. **Restore from backup** (if available)
4. **Manual restart:** `systemctl --user restart luna.service`
5. **Nuclear reset:** Restore sample configs from `$HOME/proj/LunaAI/docs/`

---

## Notes

- Configuration changes to `config.toml` and `mcp_config.json` require service restart
- System prompt changes take effect immediately
- Profile prompt changes take effect on next conversation using that profile
- Database modifications should only be done when service is stopped
- **When in doubt, ask the user before making changes!**
