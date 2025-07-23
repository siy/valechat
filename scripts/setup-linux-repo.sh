#!/bin/bash

# Linux Package Repository Setup Script
# This script sets up APT and YUM repositories for ValeChat distribution

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
REPO_DIR="$PROJECT_ROOT/dist/repo"

echo "📦 Setting up Linux package repositories..."

# Create repository directories
mkdir -p "$REPO_DIR"/{apt,yum}/{stable,beta,alpha}
mkdir -p "$REPO_DIR"/apt/{stable,beta,alpha}/{dists,pool}
mkdir -p "$REPO_DIR"/yum/{stable,beta,alpha}/{x86_64,aarch64}

# Generate GPG key for package signing
generate_gpg_key() {
    local keyname="ValeChat Package Signing"
    local email="packages@valechat.ai"
    
    echo "🔑 Generating GPG key for package signing..."
    
    # Check if key already exists
    if gpg --list-secret-keys | grep -q "$email"; then
        echo "✅ GPG key already exists"
        return 0
    fi
    
    # Create GPG batch file
    cat > "$REPO_DIR/gpg-batch" << EOF
%echo Generating package signing key
Key-Type: RSA
Key-Length: 4096
Subkey-Type: RSA
Subkey-Length: 4096
Name-Real: $keyname
Name-Email: $email
Expire-Date: 2y
Passphrase: 
%commit
%echo Done
EOF
    
    # Generate key
    gpg --batch --generate-key "$REPO_DIR/gpg-batch"
    rm "$REPO_DIR/gpg-batch"
    
    # Export public key
    gpg --armor --export "$email" > "$REPO_DIR/valechat-archive-keyring.gpg"
    
    echo "✅ GPG key generated and exported"
}

# Setup APT repository structure
setup_apt_repo() {
    local channel=$1
    local dist_dir="$REPO_DIR/apt/$channel/dists/stable"
    
    echo "📋 Setting up APT repository for $channel channel..."
    
    mkdir -p "$dist_dir"/{main,contrib,non-free}/binary-{amd64,arm64}
    mkdir -p "$dist_dir"/{main,contrib,non-free}/source
    
    # Create Release file template
    cat > "$dist_dir/Release.template" << EOF
Origin: ValeChat
Label: ValeChat $channel
Suite: stable
Codename: stable
Version: 1.0
Architectures: amd64 arm64
Components: main
Description: ValeChat multi-model AI chat application - $channel channel
Date: DATE_PLACEHOLDER
SHA256:
HASH_PLACEHOLDER
EOF
    
    # Create component directories
    for component in main contrib non-free; do
        for arch in amd64 arm64; do
            touch "$dist_dir/$component/binary-$arch/Packages"
            gzip -k "$dist_dir/$component/binary-$arch/Packages"
        done
        touch "$dist_dir/$component/source/Sources"
        gzip -k "$dist_dir/$component/source/Sources"
    done
    
    echo "✅ APT repository structure created for $channel"
}

# Setup YUM repository structure  
setup_yum_repo() {
    local channel=$1
    local repo_dir="$REPO_DIR/yum/$channel"
    
    echo "📦 Setting up YUM repository for $channel channel..."
    
    # Create repodata directories
    for arch in x86_64 aarch64; do
        mkdir -p "$repo_dir/$arch/repodata"
        
        # Create repo file template
        cat > "$repo_dir/$arch/valechat-$channel.repo" << EOF
[valechat-$channel]
name=ValeChat $channel Repository
baseurl=https://packages.valechat.ai/yum/$channel/$arch/
enabled=1
gpgcheck=1
gpgkey=https://packages.valechat.ai/valechat-archive-keyring.gpg
EOF
    done
    
    echo "✅ YUM repository structure created for $channel"
}

# Generate repository metadata
generate_apt_metadata() {
    local channel=$1
    local dist_dir="$REPO_DIR/apt/$channel/dists/stable"
    
    echo "📊 Generating APT metadata for $channel..."
    
    cd "$REPO_DIR/apt/$channel"
    
    # Generate Packages files
    for component in main contrib non-free; do
        for arch in amd64 arm64; do
            dpkg-scanpackages "pool/$component" /dev/null > "dists/stable/$component/binary-$arch/Packages"
            gzip -k "dists/stable/$component/binary-$arch/Packages"
        done
    done
    
    # Generate Release file
    cd "$dist_dir"
    
    # Calculate checksums
    {
        echo "SHA256:"
        find . -name "Packages*" -o -name "Sources*" | while read file; do
            if [ -f "$file" ]; then
                echo " $(sha256sum "$file" | cut -d' ' -f1) $(stat -c%s "$file") ${file#./}"
            fi
        done
    } > Release.hashes
    
    # Create Release file
    sed "s/DATE_PLACEHOLDER/$(date -u '+%a, %d %b %Y %H:%M:%S UTC')/" Release.template > Release.tmp
    sed "/HASH_PLACEHOLDER/r Release.hashes" Release.tmp | sed "/HASH_PLACEHOLDER/d" > Release
    rm Release.tmp Release.hashes
    
    # Sign Release file
    gpg --armor --detach-sign --output Release.gpg Release
    gpg --clearsign --output InRelease Release
    
    echo "✅ APT metadata generated for $channel"
}

generate_yum_metadata() {
    local channel=$1
    local repo_dir="$REPO_DIR/yum/$channel"
    
    echo "📊 Generating YUM metadata for $channel..."
    
    for arch in x86_64 aarch64; do
        cd "$repo_dir/$arch"
        
        # Generate metadata
        createrepo_c .
        
        # Sign metadata
        gpg --detach-sign --armor repodata/repomd.xml
        
        echo "✅ YUM metadata generated for $channel/$arch"
    done
}

# Create package upload script
create_upload_script() {
    cat > "$REPO_DIR/upload-package.sh" << 'EOF'
#!/bin/bash

# Package Upload Script
# Usage: ./upload-package.sh <package-file> <channel> [architecture]

set -e

if [ $# -lt 2 ]; then
    echo "Usage: $0 <package-file> <channel> [architecture]"
    echo "Channels: stable, beta, alpha"
    exit 1
fi

PACKAGE_FILE="$1"
CHANNEL="$2"
ARCH="${3:-$(dpkg --print-architecture)}"
REPO_DIR="$(dirname "$0")"

if [ ! -f "$PACKAGE_FILE" ]; then
    echo "❌ Package file not found: $PACKAGE_FILE"
    exit 1
fi

# Detect package type
case "$PACKAGE_FILE" in
    *.deb)
        echo "📦 Uploading DEB package to $CHANNEL..."
        
        # Copy to pool
        COMPONENT="main"
        POOL_DIR="$REPO_DIR/apt/$CHANNEL/pool/$COMPONENT"
        mkdir -p "$POOL_DIR"
        cp "$PACKAGE_FILE" "$POOL_DIR/"
        
        # Regenerate metadata
        cd "$REPO_DIR"
        ./generate-apt-metadata.sh "$CHANNEL"
        ;;
        
    *.rpm)
        echo "📦 Uploading RPM package to $CHANNEL..."
        
        # Copy to repository
        RPM_DIR="$REPO_DIR/yum/$CHANNEL/$ARCH"
        mkdir -p "$RPM_DIR"
        cp "$PACKAGE_FILE" "$RPM_DIR/"
        
        # Regenerate metadata
        cd "$REPO_DIR" 
        ./generate-yum-metadata.sh "$CHANNEL"
        ;;
        
    *)
        echo "❌ Unsupported package type: $PACKAGE_FILE"
        exit 1
        ;;
esac

echo "✅ Package uploaded successfully!"
EOF
    
    chmod +x "$REPO_DIR/upload-package.sh"
}

# Create metadata generation scripts
create_metadata_scripts() {
    # APT metadata script
    cat > "$REPO_DIR/generate-apt-metadata.sh" << 'EOF'
#!/bin/bash
CHANNEL=${1:-stable}
REPO_DIR="$(dirname "$0")"
source "$REPO_DIR/../setup-linux-repo.sh"
generate_apt_metadata "$CHANNEL"
EOF
    
    # YUM metadata script  
    cat > "$REPO_DIR/generate-yum-metadata.sh" << 'EOF'
#!/bin/bash
CHANNEL=${1:-stable}
REPO_DIR="$(dirname "$0")"
source "$REPO_DIR/../setup-linux-repo.sh"  
generate_yum_metadata "$CHANNEL"
EOF
    
    chmod +x "$REPO_DIR/generate-apt-metadata.sh"
    chmod +x "$REPO_DIR/generate-yum-metadata.sh"
}

# Create installation instructions
create_install_instructions() {
    cat > "$REPO_DIR/INSTALL.md" << 'EOF'
# ValeChat Installation Instructions

## Ubuntu/Debian (APT)

### Add Repository
```bash
# Add GPG key
curl -fsSL https://packages.valechat.ai/valechat-archive-keyring.gpg | sudo gpg --dearmor -o /usr/share/keyrings/valechat-archive-keyring.gpg

# Add repository
echo "deb [signed-by=/usr/share/keyrings/valechat-archive-keyring.gpg] https://packages.valechat.ai/apt/stable stable main" | sudo tee /etc/apt/sources.list.d/valechat.list

# Update package list
sudo apt update
```

### Install ValeChat
```bash
sudo apt install valechat
```

## Fedora/RHEL/CentOS (YUM/DNF)

### Add Repository
```bash
# Add repository
sudo tee /etc/yum.repos.d/valechat.repo << 'EOL'
[valechat-stable]
name=ValeChat Stable Repository
baseurl=https://packages.valechat.ai/yum/stable/$basearch/
enabled=1
gpgcheck=1
gpgkey=https://packages.valechat.ai/valechat-archive-keyring.gpg
EOL

# Import GPG key
sudo rpm --import https://packages.valechat.ai/valechat-archive-keyring.gpg
```

### Install ValeChat
```bash
# Fedora/RHEL 8+
sudo dnf install valechat

# CentOS 7/RHEL 7
sudo yum install valechat
```

## Alternative Channels

### Beta Channel
Replace `stable` with `beta` in repository URLs for beta releases.

### Alpha Channel  
Replace `stable` with `alpha` in repository URLs for alpha releases.

## Manual Installation

### DEB Package
```bash
wget https://github.com/valechat/valechat/releases/latest/download/valechat_latest_amd64.deb
sudo dpkg -i valechat_latest_amd64.deb
sudo apt-get install -f  # Install dependencies if needed
```

### RPM Package
```bash
wget https://github.com/valechat/valechat/releases/latest/download/valechat-latest.x86_64.rpm
sudo rpm -i valechat-latest.x86_64.rpm
```

### AppImage
```bash
wget https://github.com/valechat/valechat/releases/latest/download/ValeChat-latest.AppImage
chmod +x ValeChat-latest.AppImage
./ValeChat-latest.AppImage
```
EOF
}

# Main execution
main() {
    echo "🚀 Starting Linux repository setup..."
    
    # Check dependencies
    if ! command -v gpg &> /dev/null; then
        echo "❌ GPG not found. Please install gnupg."
        exit 1
    fi
    
    if ! command -v createrepo_c &> /dev/null; then
        echo "⚠️  createrepo_c not found. YUM repositories will not be functional."
        echo "   Install with: sudo apt install createrepo-c (Ubuntu) or sudo dnf install createrepo_c (Fedora)"
    fi
    
    # Generate GPG key
    generate_gpg_key
    
    # Setup repositories for all channels
    for channel in stable beta alpha; do
        setup_apt_repo "$channel"
        setup_yum_repo "$channel"
    done
    
    # Create helper scripts
    create_upload_script
    create_metadata_scripts
    create_install_instructions
    
    echo ""
    echo "✅ Linux package repository setup complete!"
    echo ""
    echo "📁 Repository structure created in: $REPO_DIR"
    echo "🔑 GPG public key: $REPO_DIR/valechat-archive-keyring.gpg"
    echo "📋 Installation instructions: $REPO_DIR/INSTALL.md"
    echo ""
    echo "📋 Next steps:"
    echo "1. Upload the repository to your web server"
    echo "2. Configure your web server to serve the repository"
    echo "3. Update DNS to point packages.valechat.ai to your server"
    echo "4. Test installation with provided instructions"
    echo ""
    echo "🔧 Upload a package:"
    echo "   $REPO_DIR/upload-package.sh package.deb stable"
    echo ""
}

# Run main function if script is executed directly
if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
    main "$@"
fi