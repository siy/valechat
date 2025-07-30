# ValeChat - Multi-Model AI Chat Application with MCP Support

[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)
[![Rust](https://img.shields.io/badge/rust-1.70+-brightgreen.svg)](https://www.rust-lang.org)

ValeChat is a powerful terminal-based (TUI) AI chat application built in Rust that supports multiple AI providers including OpenAI, Anthropic, and Google Gemini. It features secure API key management, conversation persistence, usage tracking, and Model Context Protocol (MCP) server support.

> **Note**: ValeChat is currently in active development. Some features may be experimental or subject to change.

## Features

- **Multi-Provider Support**: OpenAI, Anthropic Claude, Google Gemini
- **Terminal User Interface**: Built with Ratatui for a modern TUI experience
- **Conversation Management**: Create, delete, rename, and restore conversations
- **Usage Tracking**: Comprehensive billing tracking with cost analysis
- **Secure Storage**: Cross-platform secure API key storage using system keychains
- **Export Functionality**: Export conversations in JSON or TXT formats
- **MCP Support**: Model Context Protocol server integration
- **Cross-Platform**: Windows, macOS, and Linux support

## Installation

### Download Pre-built Binaries

Download the latest release for your platform:

- **macOS**: Download `valechat-VERSION-macos.tar.gz` or `ValeChat-VERSION.dmg`
- **Linux (Ubuntu/Debian)**: Download `valechat_VERSION_amd64.deb`
- **Linux (CentOS/RHEL/Fedora)**: Download `valechat-VERSION-1.x86_64.rpm`
- **Windows**: Download `valechat-VERSION-windows.zip` or `ValeChat-VERSION-Setup.exe`

### Install from Package Managers

#### macOS (Homebrew)
```bash
# Add the tap (replace with actual repository)
brew tap valechat/valechat
brew install valechat
```

#### Linux (Debian/Ubuntu)
```bash
# Download and install the .deb package
wget https://github.com/valechat/valechat/releases/latest/download/valechat_VERSION_amd64.deb
sudo dpkg -i valechat_VERSION_amd64.deb
sudo apt-get install -f  # Fix any dependency issues
```

#### Linux (CentOS/RHEL/Fedora)
```bash
# Download and install the .rpm package
wget https://github.com/valechat/valechat/releases/latest/download/valechat-VERSION-1.x86_64.rpm
sudo rpm -i valechat-VERSION-1.x86_64.rpm
# or using dnf/yum
sudo dnf install valechat-VERSION-1.x86_64.rpm
```

#### Windows (Chocolatey)
```powershell
# Install via Chocolatey (after package approval)
choco install valechat
```

#### Windows (Direct Download)
```powershell
# Download and extract ZIP
Invoke-WebRequest -Uri "https://github.com/valechat/valechat/releases/latest/download/valechat-VERSION-windows.zip" -OutFile "valechat.zip"
Expand-Archive -Path "valechat.zip" -DestinationPath "C:\Program Files\ValeChat"

# Or run the installer
.\ValeChat-VERSION-Setup.exe
```

### Build from Source

```bash
# Clone the repository
git clone https://github.com/valechat/valechat.git
cd valechat

# Build the application
cargo build --release

# The binary will be available at target/release/valechat
```

## Quick Start

1. **Install ValeChat** using one of the methods above
2. **Configure API keys** for your preferred providers
3. **Start chatting** with the interactive interface

```bash
# Set up your API keys
valechat api-key openai --set your-openai-api-key
valechat api-key anthropic --set your-anthropic-api-key
valechat api-key google --set your-google-api-key

# Start the chat interface
valechat

# Or start with specific options
valechat chat --provider openai --model gpt-4
```

## Configuration

### API Key Management

ValeChat securely stores API keys using your system's keychain/credential manager.

```bash
# Set API key for a provider
valechat api-key <provider> --set <api-key>

# Check API key status
valechat api-key <provider> --status

# Remove API key
valechat api-key <provider> --remove
```

**Supported Providers:**
- `openai` - OpenAI GPT models (GPT-4, GPT-3.5-turbo, etc.)
- `anthropic` - Anthropic Claude models (Claude-3-Opus, Claude-3-Sonnet, etc.)
- `google` - Google Gemini models (Gemini-Pro, Gemini-1.5-Pro, etc.)

### Configuration File

ValeChat can be configured using a TOML configuration file. By default, it looks for configuration in:

- **Linux**: `~/.config/valechat/config.toml`
- **macOS**: `~/Library/Application Support/ai.valechat.ValeChat/config.toml`
- **Windows**: `%APPDATA%\ai.valechat.ValeChat\config.toml`

You can specify a custom configuration file with the `-c` flag:

```bash
valechat -c /path/to/custom/config.toml
```

#### Example Configuration

```toml
# ValeChat Configuration File

[app]
# Default provider to use
default_provider = "openai"
# Default model to use
default_model = "gpt-4"
# Enable debug logging
debug = false

[models.openai]
enabled = true
api_base_url = "https://api.openai.com/v1"

[models.anthropic]
enabled = true
api_base_url = "https://api.anthropic.com"

[models.google]
enabled = true
api_base_url = "https://generativelanguage.googleapis.com/v1beta"

[ui]
# Color theme (dark or light)
theme = "dark"
# Enable mouse support
mouse_support = true

[storage]
# Database file location (optional, uses default if not specified)
# database_path = "/custom/path/to/valechat.db"

[logging]
# Log level (error, warn, info, debug, trace)
level = "info"
# Log file location (optional - defaults to platform-specific location)
# file = "/path/to/custom/valechat.log"
```

### Environment Variables

ValeChat also supports configuration via environment variables:

```bash
# API Keys (alternative to keychain storage)
export VALECHAT_OPENAI_API_KEY="your-api-key"
export VALECHAT_ANTHROPIC_API_KEY="your-api-key"
export VALECHAT_GOOGLE_API_KEY="your-api-key"

# Application settings
export VALECHAT_DEBUG=true
export VALECHAT_CONFIG_PATH="/path/to/config.toml"
export VALECHAT_DATA_DIR="/path/to/data"
```

## Usage

### Interactive Chat Interface

Start the main chat interface:

```bash
# Start with default settings
valechat

# Start with specific conversation
valechat chat --conversation <conversation-id>

# Start with specific provider and model
valechat chat --provider openai --model gpt-4
```

#### Keyboard Shortcuts

- **Tab** / **Shift+Tab**: Switch between panels
- **F1**: Show/hide help
- **Ctrl+C** / **Ctrl+Q**: Quit application
- **Enter**: Send message (in input panel)
- **Shift+Enter**: New line (in input panel)
- **n**: Create new conversation (in conversation list)
- **d** / **Delete**: Delete conversation (in conversation list)
- **r**: Rename conversation (in conversation list)
- **↑/↓**: Navigate conversations or messages
- **Esc**: Close help popup

### Command Line Interface

#### Models Command

List available models and providers:

```bash
# List all models
valechat models

# List only enabled models
valechat models --enabled
```

#### Usage Statistics

View usage and billing information:

```bash
# Show overall usage statistics
valechat usage

# Show usage for specific period
valechat usage --period month

# Show usage for specific provider
valechat usage --provider openai
```

#### Export Conversations

Export conversation data:

```bash
# Export all conversations to JSON
valechat export --format json --output conversations.json

# Export specific conversation to text file
valechat export --conversation <id> --format txt --output conversation.txt

# Export to stdout (default)
valechat export --format json
```

**Supported Export Formats:**
- `json`: Structured JSON format with full metadata
- `txt`: Plain text format for easy reading

### Advanced Configuration

#### Database Location

By default, ValeChat stores data in platform-specific directories:

- **Linux**: `~/.local/share/valechat/`
- **macOS**: `~/Library/Application Support/ai.valechat.ValeChat/`
- **Windows**: `%APPDATA%\ai.valechat.ValeChat\`

#### Logging

ValeChat automatically logs application activity to platform-specific log files. This ensures the TUI interface remains clean while providing detailed debugging information.

**Log File Locations:**

- **Linux**: `~/.local/share/valechat/logs/valechat.log`
- **macOS**: `~/Library/Application Support/ai.valechat.ValeChat/logs/valechat.log`
- **Windows**: `%APPDATA%\ai.valechat.ValeChat\logs\valechat.log`

**Enable Debug Logging:**

```bash
# Enable debug logging (writes to log file)
valechat --debug

# Or set environment variable
export VALECHAT_DEBUG=true
valechat
```

**Viewing Logs:**

```bash
# View recent logs (Linux/macOS)
tail -f ~/.local/share/valechat/logs/valechat.log       # Linux
tail -f ~/Library/Application\ Support/ai.valechat.ValeChat/logs/valechat.log  # macOS

# View logs on Windows (PowerShell)
Get-Content "$env:APPDATA\ai.valechat.ValeChat\logs\valechat.log" -Tail 20 -Wait
```

#### Custom Data Directory

You can specify a custom data directory:

```bash
export VALECHAT_DATA_DIR="/path/to/custom/data"
valechat
```

## Distribution Packages

### Building Distribution Packages

The project includes scripts to build distribution packages for all platforms:

#### macOS Package

```bash
# Build macOS application bundle and DMG
./scripts/build-macos.sh

# Output: target/macos/ValeChat.app and target/macos/ValeChat-VERSION.dmg
```

The macOS package includes:
- Native app bundle with proper metadata
- Signed binary (if certificates are available)
- DMG installer with background image
- Homebrew formula for easy installation

#### Linux Packages

```bash
# Build Debian package
./scripts/build-deb.sh

# Build RPM package  
./scripts/build-rpm.sh

# Output: target/debian/valechat_VERSION_amd64.deb and target/rpm/valechat-VERSION-1.x86_64.rpm
```

Linux packages include:
- Desktop entry files for application launchers
- Man pages with comprehensive documentation
- Shell completion placeholders (bash, zsh, fish)
- Automatic dependency resolution
- Proper file permissions and ownership

#### Windows Package

```bash
# Build Windows installer
./scripts/build-windows.sh

# Output: target/windows/ValeChat-VERSION-Setup.exe and target/windows/valechat-VERSION-windows.zip
```

The Windows package includes:
- NSIS installer with uninstaller
- Registry entries for file associations
- Start menu shortcuts
- Chocolatey package for easy installation
- PowerShell installation/uninstallation scripts

#### Build All Platforms

```bash
# Build packages for all platforms at once
./scripts/build-all.sh

# Output: Comprehensive build report and all distribution packages
```

The master build script:
- Builds all platform packages in sequence
- Generates detailed build reports
- Tracks success/failure for each platform
- Creates package verification instructions

### Package Contents

All distribution packages include:

1. **Main binary**: The valechat executable
2. **Documentation**: README, LICENSE, CHANGELOG
3. **Configuration**: Example configuration files
4. **Shell completions**: Placeholder files for bash, zsh, and fish
5. **Man pages**: Comprehensive manual pages
6. **Desktop integration**: Application shortcuts and file associations
7. **Installation scripts**: Platform-specific install/uninstall helpers

## Development

### Prerequisites

- Rust 1.70 or later
- SQLite development libraries
- Platform-specific development tools

### Building

```bash
# Clone repository
git clone https://github.com/valechat/valechat.git
cd valechat

# Install dependencies (Ubuntu/Debian)
sudo apt-get update
sudo apt-get install libsqlite3-dev pkg-config libssl-dev

# Install dependencies (macOS)
brew install sqlite openssl pkg-config

# Build
cargo build --release

# Run tests
cargo test

# Run with debug logging
RUST_LOG=debug cargo run -- --debug
```

### Project Structure

```
valechat/
├── src/
│   ├── app/           # Application state and configuration
│   ├── chat/          # Chat providers and message handling
│   ├── models/        # AI provider implementations
│   ├── storage/       # Database and data persistence
│   ├── platform/      # Platform-specific integrations
│   ├── tui/           # Terminal user interface components
│   ├── mcp/           # Model Context Protocol implementation
│   ├── billing/       # Usage tracking and billing
│   ├── cli.rs         # Command-line interface
│   ├── lib.rs         # Library exports
│   └── main.rs        # Application entry point
├── migrations/        # Database migrations (SQLx)
├── scripts/          # Build and packaging scripts
└── target/           # Build outputs and distribution packages
```

### Testing

```bash
# Run all tests
cargo test

# Run tests with output
cargo test -- --nocapture

# Run specific test module
cargo test storage::tests

# Run tests for specific component
cargo test tui::tests
cargo test models::tests
```

## Troubleshooting

### Common Issues

#### API Key Issues

```bash
# Check if API key is configured
valechat api-key openai --status

# Test API connection
valechat chat --provider openai --model gpt-3.5-turbo
```

#### Database Issues

```bash
# Reset database (WARNING: This will delete all conversations)
rm -rf ~/.local/share/valechat/  # Linux
rm -rf ~/Library/Application\ Support/ai.valechat.ValeChat/  # macOS
Remove-Item -Recurse -Force "$env:APPDATA\ai.valechat.ValeChat"  # Windows (PowerShell)
```

#### Permission Issues

```bash
# Fix permissions on Linux/macOS
chmod +x valechat
```

### Debug Mode

ValeChat writes all logs to platform-specific log files to avoid interfering with the TUI. Enable debug mode for detailed logging:

```bash
valechat --debug
```

Or set the environment variable:

```bash
export RUST_LOG=debug
valechat
```

**Check Log Files:**

After running ValeChat, check the log files for detailed debugging information:

- **Linux**: `~/.local/share/valechat/logs/valechat.log`
- **macOS**: `~/Library/Application Support/ai.valechat.ValeChat/logs/valechat.log`
- **Windows**: `%APPDATA%\ai.valechat.ValeChat\logs\valechat.log`

**Monitor Logs in Real-time:**

```bash
# Linux/macOS
tail -f ~/.local/share/valechat/logs/valechat.log

# Windows (PowerShell)
Get-Content "$env:APPDATA\ai.valechat.ValeChat\logs\valechat.log" -Tail 10 -Wait
```

### Getting Help

- **GitHub Issues**: Report bugs and request features
- **Discussions**: Ask questions and share tips
- **Documentation**: Check the wiki for detailed guides

## Security

ValeChat takes security seriously:

- **API Keys**: Stored securely using system keychains
- **Data Encryption**: SQLite database with proper permissions
- **Network Security**: TLS/SSL for all API communications
- **Input Validation**: All user inputs are validated and sanitized
- **Process Isolation**: MCP servers run in isolated processes

### Security Best Practices

1. **Rotate API Keys**: Regularly rotate your API keys
2. **Limit Permissions**: Use API keys with minimal required permissions
3. **Keep Updated**: Always use the latest version
4. **Secure Storage**: Ensure your system keychain is properly secured
5. **Network Security**: Use ValeChat only on trusted networks

## Contributing

We welcome contributions! Please see our [Contributing Guide](CONTRIBUTING.md) for details.

### Development Setup

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Add tests for new functionality
5. Ensure all tests pass
6. Submit a pull request

### Code Style

- Follow Rust standard formatting (`cargo fmt`)
- Pass all lints (`cargo clippy`)
- Include tests for new features
- Update documentation as needed

## License

ValeChat is licensed under the Apache License 2.0. See [LICENSE](LICENSE) for details.

## Changelog

See [CHANGELOG.md](CHANGELOG.md) for version history and changes.

## Support

- **Documentation**: [GitHub Wiki](https://github.com/valechat/valechat/wiki)
- **Issues**: [GitHub Issues](https://github.com/valechat/valechat/issues) - Report bugs and request features
- **Discussions**: [GitHub Discussions](https://github.com/valechat/valechat/discussions) - Ask questions and share tips
- **Email**: support@valechat.ai

## Version Information

To check your ValeChat version:
```bash
valechat --version
```

For the latest release information, see the [GitHub Releases](https://github.com/valechat/valechat/releases) page.

---

*ValeChat - Your terminal-based gateway to AI conversations*