# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

ValeChat is a cross-platform desktop chat application built with Rust and Tauri that provides access to multiple AI models with MCP (Model Context Protocol) server support. The application uses the valechat.ai domain for all identifiers and configurations.

## Current Status

**Pre-Development Phase**: The project has extensive planning documentation but no implementation yet. All source code needs to be created following the detailed specifications in `plan.md`.

## Architecture

This is a **Tauri-based desktop application** with:

- **Backend**: Rust with async/await using Tokio
- **Frontend**: React + TypeScript in Tauri's webview
- **Database**: SQLite with sqlx for chat history and usage tracking
- **Protocols**: HTTP/WebSocket for model APIs, JSON-RPC for MCP servers
- **Platform**: Native macOS with potential for cross-platform support

### Core Components (Planned)

```
src/
├── app/          # Global state and configuration management
├── models/       # Model provider abstraction (OpenAI, Anthropic, local)
├── mcp/          # MCP client and server management
├── ui/           # Tauri commands and UI logic
├── storage/      # SQLite database and migrations
└── billing/      # Usage tracking and spending limits
```

### Key Design Patterns

- **Provider Pattern**: Abstract `ModelProvider` trait for different AI services
- **Transport Abstraction**: Support for stdio and WebSocket MCP connections
- **Async Architecture**: Non-blocking operations throughout the stack
- **Configuration-Driven**: TOML-based config with runtime model switching

## Development Setup

Since the project is in pre-development:

1. **Initialize Rust Project**: `cargo init` with Tauri dependencies
2. **Add Tauri**: Follow Tauri 2.0 setup for macOS development
3. **Database Setup**: Implement SQLite with sqlx migrations
4. **Frontend**: Set up React/TypeScript within Tauri's src-tauri structure

## Key Technical Requirements

### MCP Integration
- JSON-RPC protocol implementation for tool calling
- Server lifecycle management (stdio processes, WebSocket connections)
- Dynamic tool discovery and execution within chat context

### Multi-Model Support
- Provider abstraction supporting OpenAI, Anthropic, and local models (Ollama)
- Real-time streaming responses with backpressure handling
- Model-specific configuration and capability detection

### Billing System
- Token-level usage tracking with cost calculation
- Configurable spending limits (daily/monthly/per-model)
- SQLite storage for historical data and reporting

## Security Considerations

- API keys stored in macOS Keychain via Tauri secure storage
- Input sanitization for MCP tool parameters
- Sandboxed MCP server execution
- No sensitive data in configuration files or logs

## Implementation Timeline

The project follows a 10-week phased development plan:
- **Weeks 1-2**: Foundation (Tauri setup, config system, first model provider)
- **Weeks 3-4**: Multi-model support and UI
- **Weeks 5-6**: MCP protocol integration
- **Weeks 7-8**: Billing system and database
- **Weeks 9-10**: Advanced features and polish

Reference `plan.md` for detailed specifications, database schemas, and implementation requirements.