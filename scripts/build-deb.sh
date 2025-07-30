#!/bin/bash
set -e

# Build script for Debian/Ubuntu package
# Creates a .deb package with proper metadata and dependencies

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$PROJECT_ROOT"

# Configuration
PACKAGE_NAME="valechat"
VERSION=$(grep '^version' Cargo.toml | sed 's/.*"\(.*\)".*/\1/')
ARCHITECTURE=$(dpkg --print-architecture 2>/dev/null || echo "amd64")
TARGET_DIR="target/debian"
PACKAGE_DIR="$TARGET_DIR/$PACKAGE_NAME-$VERSION"

echo "Building $PACKAGE_NAME v$VERSION for Debian/Ubuntu ($ARCHITECTURE)..."

# Clean previous builds
rm -rf "$TARGET_DIR"
mkdir -p "$TARGET_DIR"

# Build the binary
echo "Building binary for $ARCHITECTURE..."
if [ "$ARCHITECTURE" = "arm64" ] || [ "$ARCHITECTURE" = "aarch64" ]; then
    rustup target add aarch64-unknown-linux-gnu
    cargo build --release --target aarch64-unknown-linux-gnu
    BINARY_PATH="target/aarch64-unknown-linux-gnu/release/valechat"
else
    rustup target add x86_64-unknown-linux-gnu
    cargo build --release --target x86_64-unknown-linux-gnu
    BINARY_PATH="target/x86_64-unknown-linux-gnu/release/valechat"
fi

# Create package directory structure
echo "Creating package structure..."
mkdir -p "$PACKAGE_DIR/DEBIAN"
mkdir -p "$PACKAGE_DIR/usr/bin"
mkdir -p "$PACKAGE_DIR/usr/share/applications"
mkdir -p "$PACKAGE_DIR/usr/share/doc/$PACKAGE_NAME"
mkdir -p "$PACKAGE_DIR/usr/share/man/man1"
mkdir -p "$PACKAGE_DIR/usr/share/bash-completion/completions"
mkdir -p "$PACKAGE_DIR/usr/share/zsh/site-functions"
mkdir -p "$PACKAGE_DIR/usr/share/fish/completions"
mkdir -p "$PACKAGE_DIR/etc/$PACKAGE_NAME"

# Copy binary
cp "$BINARY_PATH" "$PACKAGE_DIR/usr/bin/"
chmod 755 "$PACKAGE_DIR/usr/bin/valechat"

# Create control file
cat > "$PACKAGE_DIR/DEBIAN/control" << EOF
Package: $PACKAGE_NAME
Version: $VERSION
Section: utils
Priority: optional
Architecture: $ARCHITECTURE
Depends: libssl3 (>= 3.0.0) | libssl1.1, libsqlite3-0 (>= 3.6.0), ca-certificates
Suggests: bash-completion
Maintainer: ValeChat Team <team@valechat.ai>
Description: Multi-model AI chat application with MCP server support
 ValeChat is a powerful terminal-based (TUI) AI chat application that supports
 multiple AI providers including OpenAI, Anthropic, and Google Gemini. It features
 secure API key management, conversation persistence, usage tracking, and Model
 Context Protocol (MCP) server support.
 .
 Key features:
  * Multi-provider support (OpenAI, Anthropic, Google)
  * Terminal user interface built with Ratatui
  * Conversation management (create, delete, rename, restore)
  * Usage tracking and billing analysis
  * Secure cross-platform API key storage
  * Export functionality (JSON, TXT formats)
  * MCP server integration
  * Cross-platform support
Homepage: https://valechat.ai
EOF

# Calculate installed size
INSTALLED_SIZE=$(du -sk "$PACKAGE_DIR" | cut -f1)
echo "Installed-Size: $INSTALLED_SIZE" >> "$PACKAGE_DIR/DEBIAN/control"

# Create postinst script
cat > "$PACKAGE_DIR/DEBIAN/postinst" << 'EOF'
#!/bin/bash
set -e

# Update desktop database
if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database -q
fi

# Update man database
if command -v mandb >/dev/null 2>&1; then
    mandb -q
fi

echo "ValeChat has been installed successfully!"
echo ""
echo "To get started:"
echo "  1. Configure an API key: valechat api-key openai --set YOUR_KEY"
echo "  2. Start the chat interface: valechat"
echo "  3. Get help: valechat --help"
echo ""
echo "Configuration directory: ~/.config/valechat/"
echo "Data directory: ~/.local/share/valechat/"
EOF

chmod 755 "$PACKAGE_DIR/DEBIAN/postinst"

# Create prerm script
cat > "$PACKAGE_DIR/DEBIAN/prerm" << 'EOF'
#!/bin/bash
set -e

# Nothing special needed for removal
exit 0
EOF

chmod 755 "$PACKAGE_DIR/DEBIAN/prerm"

# Create postrm script
cat > "$PACKAGE_DIR/DEBIAN/postrm" << 'EOF'
#!/bin/bash
set -e

# Update desktop database
if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database -q
fi

# Update man database  
if command -v mandb >/dev/null 2>&1; then
    mandb -q
fi

# Note: We don't remove user data directories automatically
# Users can manually remove ~/.config/valechat and ~/.local/share/valechat
EOF

chmod 755 "$PACKAGE_DIR/DEBIAN/postrm"

# Create desktop entry
cat > "$PACKAGE_DIR/usr/share/applications/valechat.desktop" << EOF
[Desktop Entry]
Version=1.0
Type=Application
Name=ValeChat
Comment=Multi-model AI chat application
GenericName=AI Chat Client
Exec=valechat
Icon=valechat
Terminal=true
Categories=Office;Utility;ConsoleOnly;
Keywords=AI;chat;OpenAI;Anthropic;GPT;Claude;terminal;TUI;
StartupNotify=false
EOF

# Copy documentation
cp README.md "$PACKAGE_DIR/usr/share/doc/$PACKAGE_NAME/" 2>/dev/null || echo "README.md not found"
cp LICENSE "$PACKAGE_DIR/usr/share/doc/$PACKAGE_NAME/" 2>/dev/null || echo "LICENSE not found"
cp CHANGELOG.md "$PACKAGE_DIR/usr/share/doc/$PACKAGE_NAME/changelog" 2>/dev/null || echo "CHANGELOG.md not found"

# Create copyright file
cat > "$PACKAGE_DIR/usr/share/doc/$PACKAGE_NAME/copyright" << EOF
Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/
Upstream-Name: valechat
Upstream-Contact: ValeChat Team <team@valechat.ai>
Source: https://github.com/valechat/valechat

Files: *
Copyright: 2024 ValeChat Team
License: Apache-2.0

License: Apache-2.0
 Licensed under the Apache License, Version 2.0 (the "License");
 you may not use this file except in compliance with the License.
 You may obtain a copy of the License at
 .
 http://www.apache.org/licenses/LICENSE-2.0
 .
 Unless required by applicable law or agreed to in writing, software
 distributed under the License is distributed on an "AS IS" BASIS,
 WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 See the License for the specific language governing permissions and
 limitations under the License.
 .
 On Debian systems, the complete text of the Apache version 2.0 license
 can be found in "/usr/share/common-licenses/Apache-2.0".
EOF

# Create man page
cat > "$PACKAGE_DIR/usr/share/man/man1/valechat.1" << 'EOF'
.TH VALECHAT 1 "2024" "valechat" "User Commands"
.SH NAME
valechat \- Multi-model AI chat application with MCP server support
.SH SYNOPSIS
.B valechat
[\fIOPTION\fR]... [\fICOMMAND\fR]
.SH DESCRIPTION
ValeChat is a powerful terminal-based (TUI) AI chat application that supports multiple AI providers including OpenAI, Anthropic, and Google Gemini. It features secure API key management, conversation persistence, usage tracking, and Model Context Protocol (MCP) server support.
.SH OPTIONS
.TP
\fB\-c\fR, \fB\-\-config\fR \fIFILE\fR
Configuration file path
.TP
\fB\-d\fR, \fB\-\-debug\fR
Enable debug logging
.TP
\fB\-\-no\-color\fR
Disable colors in output
.TP
\fB\-h\fR, \fB\-\-help\fR
Print help information
.TP
\fB\-V\fR, \fB\-\-version\fR
Print version information
.SH COMMANDS
.TP
\fBchat\fR
Start the interactive chat interface (default)
.TP
\fBapi\-key\fR \fIPROVIDER\fR
Manage API keys for providers
.TP
\fBmodels\fR
List available models and providers
.TP
\fBusage\fR
Show usage and billing information
.TP
\fBexport\fR
Export conversation data
.SH EXAMPLES
.TP
Start the chat interface:
.B valechat
.TP
Configure OpenAI API key:
.B valechat api-key openai --set sk-...
.TP
List available models:
.B valechat models --enabled
.TP
Export conversations to JSON:
.B valechat export --format json --output backup.json
.SH FILES
.TP
\fI~/.config/valechat/config.toml\fR
User configuration file
.TP
\fI~/.local/share/valechat/\fR
Data directory containing database and logs
.SH ENVIRONMENT
.TP
\fBVALECHAT_DEBUG\fR
Enable debug logging if set to 'true'
.TP
\fBVALECHAT_CONFIG_PATH\fR
Override configuration file path
.TP
\fBVALECHAT_DATA_DIR\fR
Override data directory path
.SH SEE ALSO
.BR sqlite3 (1)
.SH BUGS
Report bugs at: https://github.com/valechat/valechat/issues
.SH AUTHOR
ValeChat Team <team@valechat.ai>
EOF

# Generate shell completions
echo "Generating shell completions..."
"$PACKAGE_DIR/usr/bin/valechat" completion bash > "$PACKAGE_DIR/usr/share/bash-completion/completions/valechat" 2>/dev/null || echo "# Bash completions not available" > "$PACKAGE_DIR/usr/share/bash-completion/completions/valechat"
"$PACKAGE_DIR/usr/bin/valechat" completion zsh > "$PACKAGE_DIR/usr/share/zsh/site-functions/_valechat" 2>/dev/null || echo "# Zsh completions not available" > "$PACKAGE_DIR/usr/share/zsh/site-functions/_valechat"
"$PACKAGE_DIR/usr/bin/valechat" completion fish > "$PACKAGE_DIR/usr/share/fish/completions/valechat.fish" 2>/dev/null || echo "# Fish completions not available" > "$PACKAGE_DIR/usr/share/fish/completions/valechat.fish"

# Create example configuration
cat > "$PACKAGE_DIR/etc/$PACKAGE_NAME/config.toml.example" << EOF
# ValeChat Example Configuration
# Copy this file to ~/.config/valechat/config.toml and customize

[app]
default_provider = "openai"
default_model = "gpt-4"
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
theme = "dark"
mouse_support = true

[logging]
level = "info"
EOF

# Compress man page
gzip -9 "$PACKAGE_DIR/usr/share/man/man1/valechat.1"

# Set correct permissions
find "$PACKAGE_DIR" -type d -exec chmod 755 {} \;
find "$PACKAGE_DIR" -type f -exec chmod 644 {} \;
chmod 755 "$PACKAGE_DIR/usr/bin/valechat"
chmod 755 "$PACKAGE_DIR/DEBIAN/postinst"
chmod 755 "$PACKAGE_DIR/DEBIAN/prerm"
chmod 755 "$PACKAGE_DIR/DEBIAN/postrm"

# Build the package
echo "Building .deb package..."
fakeroot dpkg-deb --build "$PACKAGE_DIR" "$TARGET_DIR/${PACKAGE_NAME}_${VERSION}_${ARCHITECTURE}.deb"

# Validate the package
echo "Validating package..."
if command -v lintian &> /dev/null; then
    lintian "$TARGET_DIR/${PACKAGE_NAME}_${VERSION}_${ARCHITECTURE}.deb" || echo "Warning: lintian found issues (non-fatal)"
else
    echo "lintian not available, skipping package validation"
fi

# Create package info
dpkg-deb --info "$TARGET_DIR/${PACKAGE_NAME}_${VERSION}_${ARCHITECTURE}.deb" > "$TARGET_DIR/package-info.txt"
dpkg-deb --contents "$TARGET_DIR/${PACKAGE_NAME}_${VERSION}_${ARCHITECTURE}.deb" >> "$TARGET_DIR/package-info.txt"

# Create installation instructions
cat > "$TARGET_DIR/INSTALL.txt" << EOF
ValeChat v$VERSION - Debian/Ubuntu Package
==========================================

Installation:
  sudo dpkg -i ${PACKAGE_NAME}_${VERSION}_${ARCHITECTURE}.deb
  sudo apt-get install -f  # Fix any dependency issues

Removal:
  sudo apt-get remove $PACKAGE_NAME          # Remove package only
  sudo apt-get purge $PACKAGE_NAME           # Remove package and config files

User data (not removed automatically):
  ~/.config/valechat/       # Configuration files
  ~/.local/share/valechat/  # Data and database

First-time setup:
  1. valechat api-key openai --set YOUR_API_KEY
  2. valechat

For more information, see:
  man valechat
  /usr/share/doc/$PACKAGE_NAME/README.md
EOF

# Clean up build directory
rm -rf "$PACKAGE_DIR"

echo ""
echo "✅ Debian package build completed successfully!"
echo ""
echo "Generated files:"
echo "  📦 $TARGET_DIR/${PACKAGE_NAME}_${VERSION}_${ARCHITECTURE}.deb"
echo "  📋 $TARGET_DIR/package-info.txt"
echo "  📖 $TARGET_DIR/INSTALL.txt"
echo ""
echo "Package details:"
dpkg-deb --info "$TARGET_DIR/${PACKAGE_NAME}_${VERSION}_${ARCHITECTURE}.deb" | head -20
echo ""
echo "Next steps:"
echo "  1. Test installation: sudo dpkg -i ${PACKAGE_NAME}_${VERSION}_${ARCHITECTURE}.deb"
echo "  2. Upload to package repository"
echo "  3. Test on clean Debian/Ubuntu systems"