<div align="center">
  <br>
  <h1>🌙 Luna AI</h1>

  <p><strong>Your brilliant AI companion for desktop and mobile!</strong></p>
  
  <p>A modern, native application that brings powerful AI conversations to your desktop, phone, and tablet with seamless MCP integration.</p>
</div>

## ✨ What is Luna AI?

Luna AI is your intelligent companion that combines the power of modern AI with the beauty of native integration. Available on desktop and mobile, Luna brings you:

- 🤖 **Smart Conversations** - Real-time streaming responses that feel natural and engaging
- 📱 **Mobile & Desktop** - Use Luna on your phone, tablet, or desktop - all in sync
- 🔧 **MCP Superpowers** - Connect to tools and services through Model Context Protocol
- 🎨 **Beautiful Interface** - Native design that feels right at home
- 💾 **Memory Management** - Save, organize, and revisit your conversations
- 🔌 **Flexible Backends** - Support for multiple AI providers and local models

## 🚀 What Can Luna Do?

### 🎯 Core Features
- **Real-time Chat**: Watch responses stream in with smooth, non-blocking UI
- **Cross-Platform**: Desktop and mobile apps that sync seamlessly
- **Voice Mode**: Speech recognition and text-to-speech for hands-free conversations
- **Conversation History**: Never lose a brilliant idea - save and search all your chats
- **MCP Integration**: Connect to external tools, APIs, and services
- **File Attachments**: Share documents and images directly in conversations
- **Keyboard Shortcuts**: Navigate like a pro with efficient keyboard controls (desktop)
- **Thin desktop client** (`luna-thin`): COSMIC UI with selectable/copyable chat text — see [luna_thin_ui/README.md](luna_thin_ui/README.md)

### 🔧 MCP Magic
Luna's MCP integration opens up a world of possibilities:
- **File Operations**: Read, write, and manage files directly from conversations
- **Web Search**: Fetch real-time information from the internet
- **Email Management**: Send, receive, and organize emails
- **Task Management**: Integrate with your todo lists and productivity tools
- **Custom Tools**: Extend Luna's capabilities with your own MCP servers

### 💡 Examples of What You Can Do

```bash
# Ask Luna to help with programming
"Write a Rust function that sorts a vector of integers"

# Get real-time information
"What's the current weather in Warsaw?"

# File management
"Read the contents of my project's README file"

# Email tasks
"Send an email to my team about the project update"

# Web research
"Find the latest news about AI developments"

# Task management
"Add 'fix the bug in login module' to my todo list"
```

## 🔌 Supported Backends

Luna AI supports multiple AI providers, giving you flexibility and choice:

### 🌐 Cloud Providers
- **OpenAI** - GPT-4, GPT-3.5, and other OpenAI models
- **Anthropic** - Claude models with advanced reasoning
- **Google** - Gemini models and Google AI services
- **Azure OpenAI** - Enterprise-grade OpenAI deployments

### 💻 Local Models
- **Ollama** - Run local models like Llama, Mistral, and more
- **Custom endpoints** - Connect to any OpenAI-compatible API

### 🔧 Configuration
Easily switch between backends in the settings or configure multiple providers for different use cases. See the [Configuration Guide](docs/configuration.md) for detailed setup instructions.

## 🛠️ Installation

### Desktop Application

#### Building from Source
```bash
git clone https://github.com/digit1024/LunaAI.git
cd LunaAI
cargo build
```

#### Running
```bash
cargo run
```

#### Running as Server
Run Luna in server mode to access it from mobile devices and other clients:
```bash
cargo run -- --server
```

### Mobile App

The mobile app is available in the `mobile_app/` directory. Built with Flutter, it connects to the desktop server via WebSocket.

See the [Mobile App README](mobile_app/README.md) for installation instructions.

### Telegram bridge
Talk to Luna from Telegram: set `TELEGRAM_BOT_TOKEN`, `LUNA_ADDRESS`, and `LUNA_API_KEY`, then run `python telegram_bridge.py`. Optional: `ALLOWED_TELEGRAM_IDS=123,456` restricts the bot to those Telegram user IDs (comma-separated); if unset, anyone can use the bot. Commands in chat: `/new`, `/new {profile}`, `/profile {profile}`.

## 🏗️ Architecture

Luna AI is built with modern technologies:

- **Desktop**: Rust with COSMIC desktop framework for native integration
- **Mobile**: Flutter for cross-platform mobile support (Android, iOS)
- **Server Mode**: HTTP/WebSocket server for multi-device access
- **tokio**: Async runtime for smooth performance
- **MCP Protocol**: Tool calling and external service integration
- **Real-time Streaming**: Live response updates across all platforms

## 📸 Screenshots

### 🖥️ Desktop

<div align="center">
  <img src="res/screenshots/desktop_conversation.png" alt="Desktop Conversation" width="800"/>
  <p><em>Beautiful conversations on the COSMIC desktop</em></p>

  <img src="res/screenshots/desktop_history.png" alt="Desktop Conversation History" width="800"/>
  <p><em>Manage and revisit your conversation history</em></p>
  
  <img src="res/screenshots/desktop_settings.png" alt="Desktop Settings" width="800"/>
  <p><em>Configure your AI backends and preferences</em></p>

  <img src="res/screenshots/desktop_mcp.png" alt="Desktop MCP Integration" width="800"/>
  <p><em>Powerful MCP tool integration and configuration</em></p>
</div>

### 📱 Mobile

<div align="center">
  <img src="res/screenshots/mobile_conversation.png" alt="Mobile Conversation" width="400"/>
  <p><em>Chat with Luna on the go</em></p>

  <img src="res/screenshots/mobile_conversation_mode.png" alt="Mobile Voice Mode" width="400"/>
  <p><em>Voice mode for hands-free conversations</em></p>
  
  <img src="res/screenshots/mobile_settings.png" alt="Mobile Settings" width="400"/>
  <p><em>Mobile settings and configuration</em></p>
</div>

## 🤝 Contributing

We welcome contributions! Whether it's bug reports, feature requests, or code contributions, feel free to:
- Open an issue
- Submit a pull request
- Join the discussion

## 📄 License

MIT License - Feel free to use, modify, and distribute!

---

<div align="center">
  <p><strong>Ready to experience intelligent desktop computing?</strong></p>
  <p>Clone the repo and start chatting with Luna today! 🚀</p>
</div>
