# cosmic_llm

A modern desktop LLM chat application built with the COSMIC desktop framework.

## Overview

cosmic_llm is a desktop application that provides a modern, native interface for interacting with Large Language Models (LLMs). It features a clean, professional UI with real-time streaming responses, conversation management, and MCP (Model Context Protocol) server integration.

## Features

- 🎨 **Modern UI**: Built with libcosmic for native COSMIC desktop integration
- 💬 **Real-time Chat**: Streaming responses with smooth UI updates
- 📚 **Conversation Management**: Save, load, and organize chat conversations
- 🔧 **MCP Integration**: Support for Model Context Protocol servers
- ⚙️ **Configurable**: Multiple LLM providers and settings
- ⌨️ **Keyboard Shortcuts**: Efficient navigation and control
- 🔄 **Async Operations**: Non-blocking UI during processing

## Architecture

### UI Structure

```
┌─────────────────────────────────────────────────────────────┐
│                    cosmic_llm UI Architecture               │
├─────────────────────────────────────────────────────────────┤
│  ┌─────────────────┐  ┌─────────────────────────────────┐   │
│  │   Side Panel    │  │        Main Content Area        │   │
│  │   (Navigation)  │  │                                 │   │
│  │                 │  │  ┌─────────────────────────────┐ │   │
│  │  ┌─────────────┐ │  │  │        Chat Page            │ │   │
│  │  │ New Chat    │ │  │  │  - Message List              │ │   │
│  │  └─────────────┘ │  │  │  - Streaming Responses       │ │   │
│  │  ─────────────── │  │  │  - Input Field               │ │   │
│  │  ┌─────────────┐ │  │  │  - Status Bar                │ │   │
│  │  │ History     │ │  │  └─────────────────────────────┘ │   │
│  │  └─────────────┘ │  │                                 │   │
│  │  ┌─────────────┐ │  │  ┌─────────────────────────────┐ │   │
│  │  │ MCP Config  │ │  │  │     History Page             │ │   │
│  │  └─────────────┘ │  │  │  - Conversation List         │ │   │
│  │  ┌─────────────┐ │  │  │  - Search & Filter           │ │   │
│  │  │ Settings    │ │  │  │  - Conversation Details      │ │   │
│  │  └─────────────┘ │  │  └─────────────────────────────┘ │   │
│  └─────────────────┘  └─────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────┘
```

### Key Components

- **Side Panel**: Navigation controls (New Chat, History, MCP Config, Settings)
- **Chat Page**: Main conversation interface with streaming messages
- **History Page**: Conversation management and search
- **Settings Dialog**: Configuration and preferences
- **MCP Integration**: Tool calling and server management

## Development Status

See [implementation_progress.md](implementation_progress.md) for detailed development status.

## Building

```bash
cargo build
```

## Running

```bash
cargo run
```

## Dependencies

- libcosmic: COSMIC desktop framework
- tokio: Async runtime
- serde: Serialization
- uuid: Unique identifiers
- chrono: Date/time handling

## License

MIT License
