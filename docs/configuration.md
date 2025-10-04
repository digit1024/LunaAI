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