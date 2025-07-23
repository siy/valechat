#!/bin/bash

# Generate Tauri Updater Signing Keys
# This script generates the public/private key pair for Tauri's updater system

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
KEYS_DIR="$PROJECT_ROOT/.keys"

echo "🔐 Generating Tauri updater signing keys..."

# Create keys directory if it doesn't exist
mkdir -p "$KEYS_DIR"

# Check if tauri CLI is installed
if ! command -v tauri &> /dev/null; then
    echo "❌ Tauri CLI not found. Installing..."
    cargo install tauri-cli --version "^2.0" --locked
fi

# Generate the key pair
echo "Generating key pair..."
cd "$PROJECT_ROOT"
tauri signer generate -w "$KEYS_DIR/updater.key"

# Check if keys were generated successfully
if [ -f "$KEYS_DIR/updater.key" ] && [ -f "$KEYS_DIR/updater.key.pub" ]; then
    echo "✅ Keys generated successfully!"
    echo ""
    echo "📁 Private key: $KEYS_DIR/updater.key"
    echo "📁 Public key: $KEYS_DIR/updater.key.pub"
    echo ""
    echo "🔒 SECURITY NOTICE:"
    echo "   - The private key should be kept SECRET and secure"
    echo "   - Add the private key to your CI/CD secrets as TAURI_SIGNING_PRIVATE_KEY"
    echo "   - The public key will be embedded in your application"
    echo ""
    
    # Display the public key content for easy copying
    echo "📋 Public key content (copy this to tauri.conf.json):"
    echo "----------------------------------------"
    cat "$KEYS_DIR/updater.key.pub"
    echo "----------------------------------------"
    echo ""
    
    # Display the private key content for CI/CD secrets
    echo "🔑 Private key content (add to CI/CD secrets):"
    echo "----------------------------------------"
    cat "$KEYS_DIR/updater.key"
    echo "----------------------------------------"
    echo ""
    
    # Add to .gitignore if not already present
    if ! grep -q "\.keys/" "$PROJECT_ROOT/.gitignore" 2>/dev/null; then
        echo "" >> "$PROJECT_ROOT/.gitignore"
        echo "# Signing keys (keep private)" >> "$PROJECT_ROOT/.gitignore"
        echo ".keys/" >> "$PROJECT_ROOT/.gitignore"
        echo "✅ Added .keys/ to .gitignore"
    fi
    
    echo ""
    echo "📋 Next steps:"
    echo "1. Copy the private key content to your GitHub repository secrets as:"
    echo "   - Secret name: TAURI_SIGNING_PRIVATE_KEY"
    echo "   - Secret value: (the private key content above)"
    echo ""
    echo "2. Update tauri.conf.json with the public key:"
    echo "   - Replace the 'pubkey' value in the 'updater' section"
    echo "   - Use the public key content shown above"
    echo ""
    echo "3. Test the updater functionality in your application"
    echo ""
    
else
    echo "❌ Failed to generate keys!"
    exit 1
fi

echo "🎉 Updater signing keys setup complete!"