#!/bin/bash
set -e

# Build script for RPM packages (CentOS/RHEL/Fedora/openSUSE)
# Creates a .rpm package with proper metadata and dependencies

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$PROJECT_ROOT"

# Configuration
PACKAGE_NAME="valechat"
VERSION=$(grep '^version' Cargo.toml | sed 's/.*"\(.*\)".*/\1/')
RELEASE="1"
ARCHITECTURE=$(uname -m)
TARGET_DIR="target/rpm"
BUILD_ROOT="$TARGET_DIR/BUILD"
RPMBUILD_DIR="$TARGET_DIR/rpmbuild"

echo "Building $PACKAGE_NAME v$VERSION for RPM-based distributions ($ARCHITECTURE)..."

# Clean previous builds
rm -rf "$TARGET_DIR"
mkdir -p "$TARGET_DIR"
mkdir -p "$RPMBUILD_DIR"/{BUILD,BUILDROOT,RPMS,SOURCES,SPECS,SRPMS}

# Build the binary
echo "Building binary for $ARCHITECTURE..."
if [ "$ARCHITECTURE" = "aarch64" ]; then
    rustup target add aarch64-unknown-linux-gnu
    cargo build --release --target aarch64-unknown-linux-gnu
    BINARY_PATH="target/aarch64-unknown-linux-gnu/release/valechat"
else
    rustup target add x86_64-unknown-linux-gnu
    cargo build --release --target x86_64-unknown-linux-gnu
    BINARY_PATH="target/x86_64-unknown-linux-gnu/release/valechat"
fi

# Create source tarball for RPM build
echo "Creating source tarball..."
mkdir -p "$TARGET_DIR/source/$PACKAGE_NAME-$VERSION"
cp "$BINARY_PATH" "$TARGET_DIR/source/$PACKAGE_NAME-$VERSION/"
cp README.md "$TARGET_DIR/source/$PACKAGE_NAME-$VERSION/" 2>/dev/null || echo "README.md not found"
cp LICENSE "$TARGET_DIR/source/$PACKAGE_NAME-$VERSION/" 2>/dev/null || echo "LICENSE not found"  
cp CHANGELOG.md "$TARGET_DIR/source/$PACKAGE_NAME-$VERSION/" 2>/dev/null || echo "CHANGELOG.md not found"

# Create desktop file
mkdir -p "$TARGET_DIR/source/$PACKAGE_NAME-$VERSION/desktop"
cat > "$TARGET_DIR/source/$PACKAGE_NAME-$VERSION/desktop/valechat.desktop" << EOF
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

# Create example configuration
mkdir -p "$TARGET_DIR/source/$PACKAGE_NAME-$VERSION/config"
cat > "$TARGET_DIR/source/$PACKAGE_NAME-$VERSION/config/config.toml.example" << EOF
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

# Create man page
mkdir -p "$TARGET_DIR/source/$PACKAGE_NAME-$VERSION/man"
cat > "$TARGET_DIR/source/$PACKAGE_NAME-$VERSION/man/valechat.1" << 'EOF'
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

# Create tarball
cd "$TARGET_DIR/source"
tar -czf "$RPMBUILD_DIR/SOURCES/$PACKAGE_NAME-$VERSION.tar.gz" "$PACKAGE_NAME-$VERSION"
cd "$PROJECT_ROOT"

# Generate shell completions
echo "Generating shell completions..."
mkdir -p "$TARGET_DIR/source/$PACKAGE_NAME-$VERSION/completions"
"$BINARY_PATH" completion bash > "$TARGET_DIR/source/$PACKAGE_NAME-$VERSION/completions/valechat.bash" 2>/dev/null || echo "# Bash completions not available" > "$TARGET_DIR/source/$PACKAGE_NAME-$VERSION/completions/valechat.bash"
"$BINARY_PATH" completion zsh > "$TARGET_DIR/source/$PACKAGE_NAME-$VERSION/completions/_valechat" 2>/dev/null || echo "# Zsh completions not available" > "$TARGET_DIR/source/$PACKAGE_NAME-$VERSION/completions/_valechat"
"$BINARY_PATH" completion fish > "$TARGET_DIR/source/$PACKAGE_NAME-$VERSION/completions/valechat.fish" 2>/dev/null || echo "# Fish completions not available" > "$TARGET_DIR/source/$PACKAGE_NAME-$VERSION/completions/valechat.fish"

# Create RPM spec file
cat > "$RPMBUILD_DIR/SPECS/$PACKAGE_NAME.spec" << EOF
Name:           $PACKAGE_NAME
Version:        $VERSION
Release:        $RELEASE%{?dist}
Summary:        Multi-model AI chat application with MCP server support
License:        Apache-2.0
URL:            https://valechat.ai
Source0:        %{name}-%{version}.tar.gz
BuildArch:      $ARCHITECTURE

# Runtime dependencies
Requires:       openssl-libs >= 1.1.1
Requires:       sqlite >= 3.6.0
Requires:       ca-certificates

# Build dependencies (not needed since we're using pre-built binary)
BuildRequires:  systemd-rpm-macros

%description
ValeChat is a powerful terminal-based (TUI) AI chat application that supports
multiple AI providers including OpenAI, Anthropic, and Google Gemini. It features
secure API key management, conversation persistence, usage tracking, and Model 
Context Protocol (MCP) server support.

Key features:
- Multi-provider support (OpenAI, Anthropic, Google)
- Terminal user interface built with Ratatui
- Conversation management (create, delete, rename, restore)
- Usage tracking and billing analysis
- Secure cross-platform API key storage
- Export functionality (JSON, TXT formats)
- MCP server integration
- Cross-platform support

%prep
%autosetup -n %{name}-%{version}

%build
# Using pre-built binary, no compilation needed

%install
# Create directories
install -d %{buildroot}%{_bindir}
install -d %{buildroot}%{_datadir}/applications
install -d %{buildroot}%{_docdir}/%{name}
install -d %{buildroot}%{_mandir}/man1
install -d %{buildroot}%{_datadir}/bash-completion/completions
install -d %{buildroot}%{_datadir}/zsh/site-functions
install -d %{buildroot}%{_datadir}/fish/completions
install -d %{buildroot}%{_sysconfdir}/%{name}

# Install binary
install -m 755 valechat %{buildroot}%{_bindir}/

# Install desktop file
install -m 644 desktop/valechat.desktop %{buildroot}%{_datadir}/applications/

# Install documentation
install -m 644 README.md %{buildroot}%{_docdir}/%{name}/ || true
install -m 644 LICENSE %{buildroot}%{_docdir}/%{name}/ || true
install -m 644 CHANGELOG.md %{buildroot}%{_docdir}/%{name}/changelog || true

# Install man page
install -m 644 man/valechat.1 %{buildroot}%{_mandir}/man1/
gzip %{buildroot}%{_mandir}/man1/valechat.1

# Install shell completions
install -m 644 completions/valechat.bash %{buildroot}%{_datadir}/bash-completion/completions/valechat
install -m 644 completions/_valechat %{buildroot}%{_datadir}/zsh/site-functions/
install -m 644 completions/valechat.fish %{buildroot}%{_datadir}/fish/completions/

# Install example configuration
install -m 644 config/config.toml.example %{buildroot}%{_sysconfdir}/%{name}/

%files
%{_bindir}/valechat
%{_datadir}/applications/valechat.desktop
%{_docdir}/%{name}/
%{_mandir}/man1/valechat.1.gz
%{_datadir}/bash-completion/completions/valechat
%{_datadir}/zsh/site-functions/_valechat
%{_datadir}/fish/completions/valechat.fish
%config(noreplace) %{_sysconfdir}/%{name}/config.toml.example

%post
# Update desktop database if available
if [ -x %{_bindir}/update-desktop-database ]; then
    %{_bindir}/update-desktop-database %{_datadir}/applications &> /dev/null || :
fi

# Update man database if available
if [ -x %{_bindir}/mandb ]; then
    %{_bindir}/mandb -q &> /dev/null || :
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

%postun
# Update desktop database if available
if [ -x %{_bindir}/update-desktop-database ]; then
    %{_bindir}/update-desktop-database %{_datadir}/applications &> /dev/null || :
fi

# Update man database if available
if [ -x %{_bindir}/mandb ]; then
    %{_bindir}/mandb -q &> /dev/null || :
fi

%changelog
* $(date +"%a %b %d %Y") ValeChat Team <team@valechat.ai> - $VERSION-$RELEASE
- Initial RPM package
- Multi-provider AI chat application
- Terminal user interface with Ratatui
- Secure API key management
- Conversation persistence and export
- Usage tracking and billing analysis
EOF

# Build the RPM
echo "Building RPM package..."
if command -v rpmbuild &> /dev/null; then
    rpmbuild --define "_topdir $RPMBUILD_DIR" -ba "$RPMBUILD_DIR/SPECS/$PACKAGE_NAME.spec"
    
    # Copy the built RPM to target directory
    find "$RPMBUILD_DIR/RPMS" -name "*.rpm" -exec cp {} "$TARGET_DIR/" \;
    find "$RPMBUILD_DIR/SRPMS" -name "*.rpm" -exec cp {} "$TARGET_DIR/" \;
else
    echo "rpmbuild not found. Installing rpm-build..."
    # Try to install rpmbuild
    if command -v dnf &> /dev/null; then
        sudo dnf install -y rpm-build
    elif command -v yum &> /dev/null; then
        sudo yum install -y rpm-build
    elif command -v zypper &> /dev/null; then
        sudo zypper install -y rpm-build
    else
        echo "Error: Cannot install rpmbuild. Please install rpm-build package manually."
        exit 1
    fi
    
    # Try building again
    rpmbuild --define "_topdir $RPMBUILD_DIR" -ba "$RPMBUILD_DIR/SPECS/$PACKAGE_NAME.spec"
    find "$RPMBUILD_DIR/RPMS" -name "*.rpm" -exec cp {} "$TARGET_DIR/" \;
    find "$RPMBUILD_DIR/SRPMS" -name "*.rpm" -exec cp {} "$TARGET_DIR/" \;
fi

# Validate the package
echo "Validating RPM package..."
RPM_FILE=$(find "$TARGET_DIR" -name "*.rpm" | grep -v ".src.rpm" | head -1)
if [ -n "$RPM_FILE" ]; then
    rpm -qip "$RPM_FILE" > "$TARGET_DIR/package-info.txt"
    rpm -qlp "$RPM_FILE" >> "$TARGET_DIR/package-info.txt"
    
    # Check dependencies
    echo "Dependencies:" >> "$TARGET_DIR/package-info.txt"
    rpm -qRp "$RPM_FILE" >> "$TARGET_DIR/package-info.txt"
    
    if command -v rpmlint &> /dev/null; then
        rpmlint "$RPM_FILE" || echo "Warning: rpmlint found issues (non-fatal)"
    fi
fi

# Create installation instructions
cat > "$TARGET_DIR/INSTALL.txt" << EOF
ValeChat v$VERSION - RPM Package
=================================

Installation:

Fedora/CentOS 8+/RHEL 8+:
  sudo dnf install ${PACKAGE_NAME}-${VERSION}-${RELEASE}.*.rpm

CentOS 7/RHEL 7:
  sudo yum install ${PACKAGE_NAME}-${VERSION}-${RELEASE}.*.rpm

openSUSE:
  sudo zypper install ${PACKAGE_NAME}-${VERSION}-${RELEASE}.*.rpm

Generic RPM:
  sudo rpm -ivh ${PACKAGE_NAME}-${VERSION}-${RELEASE}.*.rpm

Removal:
  sudo rpm -e $PACKAGE_NAME                    # Remove package only
  
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

# Clean up
rm -rf "$RPMBUILD_DIR" "$TARGET_DIR/source"

echo ""
echo "✅ RPM package build completed successfully!"
echo ""
echo "Generated files:"
find "$TARGET_DIR" -name "*.rpm" | while read rpm; do
    echo "  📦 $(basename "$rpm")"
done
echo "  📋 $TARGET_DIR/package-info.txt"
echo "  📖 $TARGET_DIR/INSTALL.txt"
echo ""
if [ -n "$RPM_FILE" ]; then
    echo "Package details:"
    rpm -qip "$RPM_FILE" | head -15
fi
echo ""
echo "Next steps:"
echo "  1. Test installation: sudo rpm -ivh $(basename "$RPM_FILE")"
echo "  2. Upload to package repository"  
echo "  3. Test on clean CentOS/RHEL/Fedora systems"