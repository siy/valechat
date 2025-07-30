#!/bin/bash
set -e

# Master build script that builds all distribution packages
# Runs all platform-specific build scripts in sequence

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$PROJECT_ROOT"

VERSION=$(grep '^version' Cargo.toml | sed 's/.*"\(.*\)".*/\1/')
TIMESTAMP=$(date +"%Y%m%d_%H%M%S")
BUILD_LOG="target/build_${TIMESTAMP}.log"

echo "🚀 Building ValeChat v$VERSION for all platforms..."
echo "📝 Build log: $BUILD_LOG"

# Create target directory and log file
mkdir -p target
touch "$BUILD_LOG"

# Function to run build script and log output
run_build() {
    local script_name="$1"
    local platform="$2"
    
    echo "Building $platform package..." | tee -a "$BUILD_LOG"
    echo "----------------------------------------" | tee -a "$BUILD_LOG"
    
    if [ -f "scripts/$script_name" ]; then
        if bash "scripts/$script_name" 2>&1 | tee -a "$BUILD_LOG"; then
            echo "✅ $platform build completed successfully" | tee -a "$BUILD_LOG"
            return 0
        else
            echo "❌ $platform build failed" | tee -a "$BUILD_LOG"
            return 1
        fi
    else
        echo "❌ Build script scripts/$script_name not found" | tee -a "$BUILD_LOG"
        return 1
    fi
    
    echo "" | tee -a "$BUILD_LOG"
}

# Track build results
BUILDS_ATTEMPTED=0
BUILDS_SUCCESSFUL=0
FAILED_BUILDS=""

# Build macOS package
echo "🍎 Building macOS distribution package..."
if run_build "build-macos.sh" "macOS"; then
    BUILDS_SUCCESSFUL=$((BUILDS_SUCCESSFUL + 1))
else
    FAILED_BUILDS="$FAILED_BUILDS macOS"
fi
BUILDS_ATTEMPTED=$((BUILDS_ATTEMPTED + 1))

# Build Debian package
echo "🐧 Building Debian/Ubuntu package..."
if run_build "build-deb.sh" "Debian/Ubuntu"; then
    BUILDS_SUCCESSFUL=$((BUILDS_SUCCESSFUL + 1))
else
    FAILED_BUILDS="$FAILED_BUILDS Debian/Ubuntu"
fi
BUILDS_ATTEMPTED=$((BUILDS_ATTEMPTED + 1))

# Build RPM package
echo "🎩 Building RPM package..."
if run_build "build-rpm.sh" "RPM (CentOS/RHEL/Fedora)"; then
    BUILDS_SUCCESSFUL=$((BUILDS_SUCCESSFUL + 1))
else
    FAILED_BUILDS="$FAILED_BUILDS RPM"
fi
BUILDS_ATTEMPTED=$((BUILDS_ATTEMPTED + 1))

# Build Windows package
echo "🪟 Building Windows package..."
if run_build "build-windows.sh" "Windows"; then
    BUILDS_SUCCESSFUL=$((BUILDS_SUCCESSFUL + 1))
else
    FAILED_BUILDS="$FAILED_BUILDS Windows"
fi
BUILDS_ATTEMPTED=$((BUILDS_ATTEMPTED + 1))

# Generate comprehensive build report
echo "📊 Generating build report..." | tee -a "$BUILD_LOG"
echo "========================================" | tee -a "$BUILD_LOG"

REPORT_FILE="target/build_report_${TIMESTAMP}.md"
cat > "$REPORT_FILE" << EOF
# ValeChat v$VERSION - Build Report

**Build Date:** $(date)
**Build ID:** ${TIMESTAMP}
**Repository:** $(git remote get-url origin 2>/dev/null || echo "Unknown")
**Commit:** $(git rev-parse HEAD 2>/dev/null || echo "Unknown")

## Summary

- **Total Builds:** $BUILDS_ATTEMPTED
- **Successful:** $BUILDS_SUCCESSFUL
- **Failed:** $((BUILDS_ATTEMPTED - BUILDS_SUCCESSFUL))
- **Success Rate:** $(echo "scale=1; $BUILDS_SUCCESSFUL * 100 / $BUILDS_ATTEMPTED" | bc -l 2>/dev/null || echo "N/A")%

## Build Results

EOF

# Add individual build results
if [ -d "target/macos" ]; then
    echo "### ✅ macOS Package" >> "$REPORT_FILE"
    echo "" >> "$REPORT_FILE"
    echo "**Generated Files:**" >> "$REPORT_FILE"
    find target/macos -type f -name "*.app" -o -name "*.dmg" -o -name "*.tar.gz" | while read file; do
        echo "- \`$(basename "$file")\` ($(du -h "$file" | cut -f1))" >> "$REPORT_FILE"
    done
    echo "" >> "$REPORT_FILE"
else
    echo "### ❌ macOS Package - Build Failed" >> "$REPORT_FILE"
    echo "" >> "$REPORT_FILE"
fi

if [ -d "target/debian" ]; then
    echo "### ✅ Debian/Ubuntu Package" >> "$REPORT_FILE"
    echo "" >> "$REPORT_FILE"
    echo "**Generated Files:**" >> "$REPORT_FILE"
    find target/debian -name "*.deb" | while read file; do
        echo "- \`$(basename "$file")\` ($(du -h "$file" | cut -f1))" >> "$REPORT_FILE"
    done
    echo "" >> "$REPORT_FILE"
else
    echo "### ❌ Debian/Ubuntu Package - Build Failed" >> "$REPORT_FILE"
    echo "" >> "$REPORT_FILE"
fi

if [ -d "target/rpm" ]; then
    echo "### ✅ RPM Package" >> "$REPORT_FILE"
    echo "" >> "$REPORT_FILE"
    echo "**Generated Files:**" >> "$REPORT_FILE"
    find target/rpm -name "*.rpm" | while read file; do
        echo "- \`$(basename "$file")\` ($(du -h "$file" | cut -f1))" >> "$REPORT_FILE"
    done
    echo "" >> "$REPORT_FILE"
else
    echo "### ❌ RPM Package - Build Failed" >> "$REPORT_FILE"
    echo "" >> "$REPORT_FILE"
fi

if [ -d "target/windows" ]; then
    echo "### ✅ Windows Package" >> "$REPORT_FILE"
    echo "" >> "$REPORT_FILE"
    echo "**Generated Files:**" >> "$REPORT_FILE"
    find target/windows -name "*.exe" -o -name "*.zip" -o -name "*.tar.gz" | while read file; do
        echo "- \`$(basename "$file")\` ($(du -h "$file" | cut -f1))" >> "$REPORT_FILE"
    done
    echo "" >> "$REPORT_FILE"
else
    echo "### ❌ Windows Package - Build Failed" >> "$REPORT_FILE"
    echo "" >> "$REPORT_FILE"
fi

# Add system information
cat >> "$REPORT_FILE" << EOF
## Build Environment

- **OS:** $(uname -s) $(uname -r)
- **Architecture:** $(uname -m)
- **Rust Version:** $(rustc --version 2>/dev/null || echo "Not available")
- **Cargo Version:** $(cargo --version 2>/dev/null || echo "Not available")
- **Build User:** $(whoami)
- **Build Host:** $(hostname)

## Next Steps

1. **Test Packages:** Test each package on target platforms
2. **Upload Artifacts:** Upload to GitHub releases or package repositories
3. **Update Documentation:** Update installation instructions if needed
4. **Announce Release:** Notify users of the new version

## Package Verification

Before distributing, verify each package:

### macOS
\`\`\`bash
# Test the application bundle
open target/macos/ValeChat.app
# Test the DMG
hdiutil verify target/macos/ValeChat-$VERSION.dmg
\`\`\`

### Linux (Debian)
\`\`\`bash
# Test package installation
sudo dpkg -i target/debian/valechat_*_amd64.deb
valechat --version
sudo apt-get remove valechat
\`\`\`

### Linux (RPM)
\`\`\`bash
# Test package installation
sudo rpm -ivh target/rpm/valechat-*.rpm
valechat --version
sudo rpm -e valechat
\`\`\`

### Windows
\`\`\`powershell
# Test the executable
target\\windows\\valechat.exe --version
# Test the installer (if available)
target\\windows\\ValeChat-$VERSION-Setup.exe /S
\`\`\`

---

*Build completed at $(date)*
EOF

# Display final summary
echo ""
echo "🎉 Build Summary:"
echo "=================="
echo "Builds attempted: $BUILDS_ATTEMPTED"
echo "Builds successful: $BUILDS_SUCCESSFUL"
echo "Builds failed: $((BUILDS_ATTEMPTED - BUILDS_SUCCESSFUL))"

if [ $BUILDS_SUCCESSFUL -eq $BUILDS_ATTEMPTED ]; then
    echo ""
    echo "✅ All builds completed successfully!"
    echo ""
    echo "📦 Generated packages:"
    find target -name "*.app" -o -name "*.dmg" -o -name "*.deb" -o -name "*.rpm" -o -name "*.exe" -o -name "*.zip" -o -name "*.tar.gz" | while read file; do
        echo "  $(basename "$file") ($(du -h "$file" | cut -f1))"
    done
else
    echo ""
    echo "⚠️  Some builds failed: $FAILED_BUILDS"
    echo "Check the build log for details: $BUILD_LOG"
fi

echo ""
echo "📋 Build report: $REPORT_FILE"
echo "📝 Build log: $BUILD_LOG"
echo ""
echo "Next steps:"
echo "1. Review build report and test packages"
echo "2. Upload successful packages to release artifacts"
echo "3. Update package repositories and distribution channels"
echo "4. Test installations on clean target systems"

# Exit with error if any builds failed
if [ $BUILDS_SUCCESSFUL -ne $BUILDS_ATTEMPTED ]; then
    exit 1
fi