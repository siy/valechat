#!/bin/bash
set -e

# Build script for macOS distribution package
# Creates an application bundle, DMG installer, and Homebrew formula

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$PROJECT_ROOT"

# Configuration
APP_NAME="ValeChat"
BUNDLE_ID="ai.valechat.ValeChat"
VERSION=$(grep '^version' Cargo.toml | sed 's/.*"\(.*\)".*/\1/')
TARGET_DIR="target/macos"
BUNDLE_DIR="$TARGET_DIR/$APP_NAME.app"
DMG_NAME="$APP_NAME-$VERSION.dmg"

echo "Building $APP_NAME v$VERSION for macOS..."

# Clean previous builds
rm -rf "$TARGET_DIR"
mkdir -p "$TARGET_DIR"

# Build the binary for macOS (both Intel and Apple Silicon)
echo "Building universal binary..."
rustup target add x86_64-apple-darwin aarch64-apple-darwin

cargo build --release --target x86_64-apple-darwin
cargo build --release --target aarch64-apple-darwin

# Create universal binary
mkdir -p "$TARGET_DIR/bin"
lipo -create \
    target/x86_64-apple-darwin/release/valechat \
    target/aarch64-apple-darwin/release/valechat \
    -output "$TARGET_DIR/bin/valechat"

# Create application bundle structure
echo "Creating application bundle..."
mkdir -p "$BUNDLE_DIR/Contents/MacOS"
mkdir -p "$BUNDLE_DIR/Contents/Resources"

# Copy binary to bundle
cp "$TARGET_DIR/bin/valechat" "$BUNDLE_DIR/Contents/MacOS/"
chmod +x "$BUNDLE_DIR/Contents/MacOS/valechat"

# Create Info.plist
cat > "$BUNDLE_DIR/Contents/Info.plist" << EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDisplayName</key>
    <string>$APP_NAME</string>
    <key>CFBundleExecutable</key>
    <string>valechat</string>
    <key>CFBundleIdentifier</key>
    <string>$BUNDLE_ID</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>$APP_NAME</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>$VERSION</string>
    <key>CFBundleVersion</key>
    <string>$VERSION</string>
    <key>LSMinimumSystemVersion</key>
    <string>10.15</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>LSApplicationCategoryType</key>
    <string>public.app-category.productivity</string>
    <key>CFBundleDocumentTypes</key>
    <array>
        <dict>
            <key>CFBundleTypeName</key>
            <string>Chat Export</string>
            <key>CFBundleTypeExtensions</key>
            <array>
                <string>valechat</string>
            </array>
            <key>CFBundleTypeRole</key>
            <string>Editor</string>
        </dict>
    </array>
    <key>NSHumanReadableCopyright</key>
    <string>Copyright © 2024 ValeChat Team. All rights reserved.</string>
</dict>
</plist>
EOF

# Create application icon (if available)
if [ -f "assets/icon.icns" ]; then
    cp "assets/icon.icns" "$BUNDLE_DIR/Contents/Resources/"
    echo "    <key>CFBundleIconFile</key>" >> "$BUNDLE_DIR/Contents/Info.plist.tmp"
    echo "    <string>icon</string>" >> "$BUNDLE_DIR/Contents/Info.plist.tmp"
fi

# Copy documentation
mkdir -p "$BUNDLE_DIR/Contents/Resources/docs"
cp README.md "$BUNDLE_DIR/Contents/Resources/docs/" 2>/dev/null || true
cp LICENSE "$BUNDLE_DIR/Contents/Resources/docs/" 2>/dev/null || true
cp CHANGELOG.md "$BUNDLE_DIR/Contents/Resources/docs/" 2>/dev/null || true

# Create example configuration
mkdir -p "$BUNDLE_DIR/Contents/Resources/config"
cat > "$BUNDLE_DIR/Contents/Resources/config/config.toml" << EOF
# ValeChat Example Configuration
# Copy this file to ~/Library/Application Support/ai.valechat.ValeChat/config.toml

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

# Sign the application (if certificates are available)
if command -v codesign &> /dev/null; then
    echo "Signing application..."
    # Try to sign with Developer ID
    if security find-identity -v -p codesigning | grep -q "Developer ID Application"; then
        SIGNING_IDENTITY=$(security find-identity -v -p codesigning | grep "Developer ID Application" | head -1 | grep -o '"[^"]*"' | sed 's/"//g')
        echo "Signing with: $SIGNING_IDENTITY"
        codesign --force --deep --sign "$SIGNING_IDENTITY" "$BUNDLE_DIR" || echo "Warning: Code signing failed"
    else
        echo "No Developer ID certificate found. Signing with ad-hoc signature..."
        codesign --force --deep --sign - "$BUNDLE_DIR" || echo "Warning: Ad-hoc signing failed"
    fi
else
    echo "codesign not available. Skipping code signing."
fi

# Create standalone tarball
echo "Creating standalone tarball..."
cd "$TARGET_DIR"
tar -czf "valechat-$VERSION-macos.tar.gz" "$APP_NAME.app"
cd "$PROJECT_ROOT"

# Create DMG installer
echo "Creating DMG installer..."
if command -v create-dmg &> /dev/null; then
    create-dmg \
        --volname "$APP_NAME $VERSION" \
        --volicon "assets/icon.icns" \
        --window-pos 200 120 \
        --window-size 800 400 \
        --icon-size 100 \
        --icon "$APP_NAME.app" 200 190 \
        --hide-extension "$APP_NAME.app" \
        --app-drop-link 600 185 \
        --background "assets/dmg-background.png" \
        "$TARGET_DIR/$DMG_NAME" \
        "$BUNDLE_DIR" || {
        echo "create-dmg failed, creating simple DMG..."
        hdiutil create -volname "$APP_NAME $VERSION" -srcfolder "$BUNDLE_DIR" -ov -format UDZO "$TARGET_DIR/$DMG_NAME"
    }
else
    echo "create-dmg not found. Creating simple DMG..."
    hdiutil create -volname "$APP_NAME $VERSION" -srcfolder "$BUNDLE_DIR" -ov -format UDZO "$TARGET_DIR/$DMG_NAME"
fi

# Create Homebrew formula
echo "Creating Homebrew formula..."
mkdir -p "$TARGET_DIR/homebrew"
cat > "$TARGET_DIR/homebrew/valechat.rb" << EOF
class Valechat < Formula
  desc "Multi-model AI chat application with MCP server support"
  homepage "https://valechat.ai"
  url "https://github.com/valechat/valechat/releases/download/v$VERSION/valechat-$VERSION-macos.tar.gz"
  version "$VERSION"
  license "Apache-2.0"

  depends_on "sqlite"

  def install
    prefix.install "ValeChat.app"
    bin.install_symlink prefix/"ValeChat.app/Contents/MacOS/valechat"
    
    # Install shell completions
    generate_completions_from_executable(bin/"valechat", "completion")
    
    # Install documentation
    doc.install prefix/"ValeChat.app/Contents/Resources/docs/README.md"
    doc.install prefix/"ValeChat.app/Contents/Resources/docs/LICENSE" if File.exist?(prefix/"ValeChat.app/Contents/Resources/docs/LICENSE")
    
    # Install example configuration
    (etc/"valechat").install prefix/"ValeChat.app/Contents/Resources/config/config.toml" => "config.toml.example"
  end

  def caveats
    <<~EOS
      ValeChat has been installed as both a command-line tool and a macOS application.
      
      To use the command-line interface:
        valechat --help
        
      To configure API keys:
        valechat api-key openai --set YOUR_API_KEY
        
      The application bundle is also available at:
        #{prefix}/ValeChat.app
        
      Example configuration file is available at:
        #{etc}/valechat/config.toml.example
    EOS
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/valechat --version")
  end
end
EOF

# Create installation script
cat > "$TARGET_DIR/install.sh" << 'EOF'
#!/bin/bash
# ValeChat macOS Installation Script

set -e

APP_NAME="ValeChat"
INSTALL_DIR="/Applications"
BINARY_DIR="/usr/local/bin"

echo "Installing ValeChat..."

# Check if running as root
if [[ $EUID -eq 0 ]]; then
    echo "This script should not be run as root. Please run as a regular user."
    exit 1
fi

# Copy application to Applications folder
if [ -d "$APP_NAME.app" ]; then
    echo "Installing $APP_NAME.app to $INSTALL_DIR..."
    sudo cp -R "$APP_NAME.app" "$INSTALL_DIR/"
    
    # Create symlink to binary
    echo "Creating command-line symlink..."
    sudo ln -sf "$INSTALL_DIR/$APP_NAME.app/Contents/MacOS/valechat" "$BINARY_DIR/valechat"
    
    echo "Installation completed successfully!"
    echo ""
    echo "You can now:"
    echo "  - Use the command line: valechat --help"
    echo "  - Launch the app from Applications folder"
    echo "  - Configure API keys: valechat api-key openai --set YOUR_KEY"
else
    echo "Error: $APP_NAME.app not found in current directory"
    exit 1
fi
EOF

chmod +x "$TARGET_DIR/install.sh"

# Generate shell completions
echo "Generating shell completions..."
mkdir -p "$TARGET_DIR/completions"
"$TARGET_DIR/bin/valechat" completion bash > "$TARGET_DIR/completions/valechat.bash" 2>/dev/null || echo "# Bash completions not available" > "$TARGET_DIR/completions/valechat.bash"
"$TARGET_DIR/bin/valechat" completion zsh > "$TARGET_DIR/completions/_valechat" 2>/dev/null || echo "# Zsh completions not available" > "$TARGET_DIR/completions/_valechat"
"$TARGET_DIR/bin/valechat" completion fish > "$TARGET_DIR/completions/valechat.fish" 2>/dev/null || echo "# Fish completions not available" > "$TARGET_DIR/completions/valechat.fish"

# Create uninstaller
cat > "$TARGET_DIR/uninstall.sh" << 'EOF'
#!/bin/bash
# ValeChat macOS Uninstaller

APP_NAME="ValeChat"
INSTALL_DIR="/Applications"
BINARY_DIR="/usr/local/bin"
CONFIG_DIR="$HOME/Library/Application Support/ai.valechat.ValeChat"
CACHE_DIR="$HOME/Library/Caches/ai.valechat.ValeChat"

echo "Uninstalling ValeChat..."

# Remove application
if [ -d "$INSTALL_DIR/$APP_NAME.app" ]; then
    echo "Removing $APP_NAME.app from $INSTALL_DIR..."
    sudo rm -rf "$INSTALL_DIR/$APP_NAME.app"
fi

# Remove symlink
if [ -L "$BINARY_DIR/valechat" ]; then
    echo "Removing command-line symlink..."
    sudo rm -f "$BINARY_DIR/valechat"
fi

# Ask about user data
echo ""
read -p "Do you want to remove user data and configuration? [y/N]: " -n 1 -r
echo
if [[ $REPLY =~ ^[Yy]$ ]]; then
    echo "Removing user data..."
    rm -rf "$CONFIG_DIR"
    rm -rf "$CACHE_DIR"
    
    # Remove keychain entries
    echo "Removing keychain entries..."
    security delete-generic-password -s "valechat-openai" -a "api-key" 2>/dev/null || true
    security delete-generic-password -s "valechat-anthropic" -a "api-key" 2>/dev/null || true
    security delete-generic-password -s "valechat-google" -a "api-key" 2>/dev/null || true
fi

echo "Uninstallation completed."
EOF

chmod +x "$TARGET_DIR/uninstall.sh"

# Create package info
cat > "$TARGET_DIR/package-info.txt" << EOF
ValeChat v$VERSION - macOS Distribution Package
===============================================

Contents:
- ValeChat.app                    # Main application bundle
- valechat-$VERSION-macos.tar.gz  # Standalone archive
- $DMG_NAME                      # DMG installer
- install.sh                     # Installation script
- uninstall.sh                   # Uninstaller script
- homebrew/valechat.rb           # Homebrew formula
- completions/                   # Shell completion files

Installation Options:

1. Drag & Drop (DMG):
   Open $DMG_NAME and drag ValeChat.app to Applications

2. Script Installation:
   ./install.sh

3. Homebrew (after publishing):
   brew tap valechat/valechat
   brew install valechat

4. Manual Installation:
   tar -xzf valechat-$VERSION-macos.tar.gz
   cp -R ValeChat.app /Applications/
   ln -s /Applications/ValeChat.app/Contents/MacOS/valechat /usr/local/bin/valechat

Usage:
  valechat --help                    # Show help
  valechat api-key openai --set KEY  # Configure API key
  valechat                           # Start chat interface

For more information, see README.md in the documentation folder.
EOF

echo ""
echo "✅ macOS package build completed successfully!"
echo ""
echo "Generated files:"
echo "  📱 $BUNDLE_DIR"
echo "  📦 $TARGET_DIR/valechat-$VERSION-macos.tar.gz"
echo "  💿 $TARGET_DIR/$DMG_NAME"
echo "  🍺 $TARGET_DIR/homebrew/valechat.rb"
echo "  📋 $TARGET_DIR/install.sh"
echo "  🗑️  $TARGET_DIR/uninstall.sh"
echo ""
echo "Next steps:"
echo "  1. Test the application bundle"
echo "  2. Upload release artifacts to GitHub"
echo "  3. Update Homebrew tap repository"
echo "  4. Test DMG installer on clean system"