# Cosmic LLM Configuration Guide

This guide explains how to configure Cosmic LLM for your needs.

## Configuration Files Location

Cosmic LLM stores configuration files in the following locations:

- **Main Configuration**: `~/.local/share/cosmic_llm/config.toml`
- **MCP Configuration**: `~/.local/share/cosmic_llm/mcp_config.json`
- **System Prompt**: `~/.local/share/cosmic_llm/system_prompt.md`
- **User Prompt**: `~/.local/share/cosmic_llm/user_prompt.md`

## Quick Setup

1. **Create the configuration directory**:
   ```bash
   mkdir -p ~/.local/share/cosmic_llm
   ```

2. **Copy sample configuration files**:
   ```bash
   # Copy main config
   cp docs/sample_config.toml ~/.local/share/cosmic_llm/config.toml

   # Copy MCP config (optional)
   cp docs/sample_mcp_config.json ~/.local/share/cosmic_llm/mcp_config.json

   # Copy prompt files (optional)
   cp docs/sample_system_prompt.md ~/.local/share/cosmic_llm/system_prompt.md
   cp docs/sample_user_prompt.md ~/.local/share/cosmic_llm/user_prompt.md
   ```

3. **Edit the configuration files** with your actual API keys and preferences

## Main Configuration (config.toml)

The main configuration file uses TOML format and contains the following sections:

### Default Profile
```toml
default = "openai"  # Name of the default LLM profile
```

### LLM Profiles
Configure multiple AI providers and switch between them:

```toml
[profiles.openai]
backend = "openai"
api_key = "your-openai-api-key"
model = "gpt-4o"
endpoint = "https://api.openai.com/v1"
temperature = 0.7
max_tokens = 4000
profile_prompt_file = "~/.local/share/cosmic_llm/profiles/diet.md"
enabled_mcp = "filesystem,weather"

[profiles.anthropic]
backend = "anthropic"
api_key = "your-anthropic-api-key"
model = "claude-3-5-sonnet-20241022"
endpoint = "https://api.anthropic.com"
temperature = 0.7
max_tokens = 4000

[profiles.ollama]
backend = "ollama"
api_key = ""  # Not needed for Ollama
model = "llama3.1:8b"
endpoint = "http://localhost:11434"
temperature = 0.7
max_tokens = 4000

[profiles.gemini]
backend = "gemini"
api_key = "your-google-ai-api-key"
model = "gemini-1.5-pro"
endpoint = "https://generativelanguage.googleapis.com"
temperature = 0.7
max_tokens = 4000
```

### Supported Backends
- **openai**: OpenAI API (GPT-4, GPT-3.5, etc.)
- **anthropic**: Anthropic Claude models
- **ollama**: Local models via Ollama
- **gemini**: Google Gemini models

### Per-profile Behavior
- `profile_prompt_file` (optional) lets you load an extra system prompt whenever that profile is active. Paths can be absolute or relative to the config directory (`~/.local/share/cosmic_llm/`), so `profiles/diet.md` resolves to `~/.local/share/cosmic_llm/profiles/diet.md`. Cosmic LLM adds the content after the global system prompt if the file exists; missing files show a warning instead of failing silently.
- `enabled_mcp` (optional) lists MCP server names to auto-enable for this profile. The value accepts a comma-separated string or a TOML list. Tools from other servers stay visible but default to disabled so you can opt in manually.

### Prompt Configuration
```toml
[prompts]
system_prompt_file = "~/.local/share/cosmic_llm/system_prompt.md"
user_prompt_file = "~/.local/share/cosmic_llm/user_prompt.md"
```

### MCP Configuration
```toml
[mcp]
[mcp.servers]
# Add MCP server configurations here
```

### Title & Summary Configuration
Configure automatic conversation title generation (server mode only):

**Note:** Title generation is only enabled if `title_generation_profile` is specified. If this field is missing or commented out, the feature will not start.

```toml
[title_summary]
# Required: Profile name to use for title generation
# Feature is disabled if this is not specified
title_generation_profile = "openai"

# Maximum characters to include in conversation transcript for title generation
summary_chars = 1000

# Sleep interval in seconds for background title generation thread
# The server will check for conversations without titles every N seconds
summary_loop_sleep_seconds = 15

# System prompt for title generation
title_generation_system_prompt = "Your task is to generate a conversation title that will describe the topic easily. Keep original conversation language and tone. YOU SHOULD ALWAYS ANSWER ONLY WITH TITLE. MAXIMUM 100CHARS. You will receive a part of the conversation transcript in next message"
```

**Title Generation Behavior:**
- Only runs in server mode (`--server` flag)
- Automatically generates titles for conversations that don't have them
- Uses the first 5 messages (excluding tool role messages) from each conversation
- Formats messages as "User: CONTENT\nAssistant: CONTENT\n..." up to `summary_chars` limit
- Generated titles are truncated to 100 characters maximum
- Runs in a background thread that checks for untitled conversations periodically

## MCP Configuration (mcp_config.json)

MCP (Model Context Protocol) allows Cosmic LLM to connect to external tools and services. The configuration uses JSON format compatible with Claude Desktop:

```json
{
  "mcpServers": {
    "filesystem": {
      "command": "npx",
      "args": ["@modelcontextprotocol/server-filesystem", "/home/user/documents"],
      "env": {}
    },
    "weather": {
      "command": "npx",
      "args": ["@modelcontextprotocol/server-weather"],
      "env": {
        "OPENWEATHER_API_KEY": "your-api-key"
      }
    }
  }
}
```

### Available MCP Servers

- **filesystem**: File operations in specified directories
- **weather**: Weather information and forecasts
- **github**: GitHub repository operations
- **postgres**: Database queries and operations
- **brave-search**: Web search capabilities
- **many more**: Check [MCP registry](https://github.com/modelcontextprotocol/registry) for available servers

## Prompt Files

### System Prompt (system_prompt.md)

This file contains the system prompt that guides the AI's behavior. It's loaded at startup and sets the assistant's personality and capabilities.

### User Prompt (user_prompt.md)

This file contains templates and common prompts that users might want to use consistently. It's useful for maintaining consistent prompt patterns.

## Environment Variables

Cosmic LLM supports environment variable expansion in MCP configuration using `${env:VAR_NAME}` syntax:

```json
{
  "mcpServers": {
    "github": {
      "command": "npx",
      "args": ["@modelcontextprotocol/server-github"],
      "env": {
        "GITHUB_PERSONAL_ACCESS_TOKEN": "${env:GITHUB_TOKEN}"
      }
    }
  }
}
```

## Title Generation

Cosmic LLM can automatically generate titles for conversations in server mode. The title generation system:

1. **Runs in background**: A background thread periodically checks for conversations without generated titles
2. **Uses LLM**: Generates titles using the configured LLM profile and system prompt
3. **Processes messages**: Takes the first 5 messages (excluding tool messages) and formats them as a transcript
4. **Updates database**: Automatically updates conversation titles and marks them as generated

### Configuration Options

- **title_generation_profile**: (Required) Which LLM profile to use for title generation. **The feature is disabled if this is not specified.**
- **summary_chars**: Maximum number of characters to include in the conversation transcript sent to the LLM (default: 1000)
- **summary_loop_sleep_seconds**: How often the background thread checks for untitled conversations in seconds (default: 15)
- **title_generation_system_prompt**: The system prompt used to instruct the LLM on how to generate titles

### How It Works

1. When running in server mode, a background thread starts automatically
2. Every `summary_loop_sleep_seconds`, it queries the database for conversations where `title_generated = false`
3. For each conversation:
   - Loads the first 5 messages (excluding "tool" role messages)
   - Formats them as "User: CONTENT\nAssistant: CONTENT\n..." up to `summary_chars` characters
   - Sends the formatted transcript to the LLM with the configured system prompt
   - Truncates the response to 100 characters
   - Updates the conversation title and sets `title_generated = true`

### Notes

- Title generation only works in server mode (`--server` flag)
- The system prompt can be customized to change how titles are generated
- If a conversation has no messages or only tool messages, it will be titled "Untitled Conversation"
- Errors during title generation are logged but don't stop the background thread

## Configuration Management

### Creating Profiles via UI

You can also create and manage LLM profiles through the Cosmic LLM interface:

1. Open Cosmic LLM
2. Navigate to Settings
3. Click "Add New Profile"
4. Fill in the profile details:
   - Profile Name
   - Backend (OpenAI, Anthropic, Ollama, Gemini)
   - Model
   - Endpoint
   - API Key

### Switching Between Profiles

- Use the profile dropdown in the chat interface
- Or change the `default` value in `config.toml`

### Saving Configuration Changes

Configuration changes made through the UI are automatically saved. Manual edits to configuration files require restarting the application.

## Troubleshooting

### Common Issues

1. **Configuration not loading**:
   - Ensure the config directory exists: `~/.local/share/cosmic_llm/`
   - Check file permissions
   - Verify TOML syntax is correct

2. **API keys not working**:
   - Verify API keys are correct and active
   - Check for typos in the configuration
   - Ensure you have proper API access

3. **MCP servers not connecting**:
   - Verify the MCP server packages are installed
   - Check environment variables are set correctly
   - Ensure the server commands are available in PATH

### Debug Mode

Run Cosmic LLM with verbose logging to debug configuration issues:

```bash
RUST_LOG=debug cargo run
```

## Sample Files

Sample configuration files are available in the `docs/` directory:

- `docs/sample_config.toml` - Main configuration template
- `docs/sample_mcp_config.json` - MCP configuration template
- `docs/sample_system_prompt.md` - System prompt template
- `docs/sample_user_prompt.md` - User prompt template

Copy these files to the configuration directory and customize them for your needs.