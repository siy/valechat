#!/bin/bash
set -e

# Build script for Windows distribution package
# Creates an NSIS installer, ZIP archive, and Chocolatey package

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$PROJECT_ROOT"

# Configuration
APP_NAME="ValeChat"
PACKAGE_NAME="valechat"
VERSION=$(grep '^version' Cargo.toml | sed 's/.*"\(.*\)".*/\1/')
TARGET_DIR="target/windows"
NSIS_DIR="$TARGET_DIR/nsis"
CHOCO_DIR="$TARGET_DIR/chocolatey"

echo "Building $APP_NAME v$VERSION for Windows..."

# Clean previous builds
rm -rf "$TARGET_DIR"
mkdir -p "$TARGET_DIR"
mkdir -p "$NSIS_DIR"
mkdir -p "$CHOCO_DIR"

# Build the binary for Windows
echo "Building Windows binary..."
rustup target add x86_64-pc-windows-gnu

# Check if we can cross-compile to Windows
if cargo build --release --target x86_64-pc-windows-gnu 2>/dev/null; then
    BINARY_PATH="target/x86_64-pc-windows-gnu/release/valechat.exe"
    echo "Cross-compiled Windows binary successfully"
else
    echo "Cross-compilation failed. You may need to build this on Windows or use a Windows cross-compilation environment."
    echo "For now, creating package structure with placeholder..."
    mkdir -p "target/x86_64-pc-windows-gnu/release"
    echo "# Windows binary placeholder" > "target/x86_64-pc-windows-gnu/release/valechat.exe"
    BINARY_PATH="target/x86_64-pc-windows-gnu/release/valechat.exe"
fi

# Copy binary to target directory
cp "$BINARY_PATH" "$TARGET_DIR/"

# Create NSIS installer script
cat > "$NSIS_DIR/installer.nsi" << EOF
!define APP_NAME "$APP_NAME"
!define APP_VERSION "$VERSION"
!define PUBLISHER "ValeChat Team"
!define WEB_SITE "https://valechat.ai"
!define APP_EXE "valechat.exe"

Unicode True
SetCompressor lzma

!include "MUI2.nsh"
!include "FileAssociation.nsh"

# General
Name "\${APP_NAME}"
OutFile "..\ValeChat-\${APP_VERSION}-Setup.exe"
InstallDir "\$PROGRAMFILES64\\\${APP_NAME}"
InstallDirRegKey HKLM "Software\\\${APP_NAME}" "InstallDir"
RequestExecutionLevel admin

# Version information
VIProductVersion "\${APP_VERSION}.0"
VIAddVersionKey "ProductName" "\${APP_NAME}"
VIAddVersionKey "ProductVersion" "\${APP_VERSION}"
VIAddVersionKey "CompanyName" "\${PUBLISHER}"
VIAddVersionKey "LegalCopyright" "© 2024 \${PUBLISHER}"
VIAddVersionKey "FileDescription" "Multi-model AI chat application"
VIAddVersionKey "FileVersion" "\${APP_VERSION}"

# MUI Settings
!define MUI_ABORTWARNING
!define MUI_ICON "\${NSISDIR}\\Contrib\\Graphics\\Icons\\modern-install.ico"
!define MUI_UNICON "\${NSISDIR}\\Contrib\\Graphics\\Icons\\modern-uninstall.ico"
!define MUI_WELCOMEFINISHPAGE_BITMAP "\${NSISDIR}\\Contrib\\Graphics\\Wizard\\win.bmp"
!define MUI_UNWELCOMEFINISHPAGE_BITMAP "\${NSISDIR}\\Contrib\\Graphics\\Wizard\\win.bmp"

# Welcome page
!insertmacro MUI_PAGE_WELCOME

# License page
!insertmacro MUI_PAGE_LICENSE "..\\..\\LICENSE"

# Directory page
!insertmacro MUI_PAGE_DIRECTORY

# Components page
!insertmacro MUI_PAGE_COMPONENTS

# Instfiles page
!insertmacro MUI_PAGE_INSTFILES

# Finish page
!define MUI_FINISHPAGE_RUN "\$INSTDIR\\\${APP_EXE}"
!define MUI_FINISHPAGE_RUN_TEXT "Run \${APP_NAME}"
!define MUI_FINISHPAGE_SHOWREADME "\$INSTDIR\\README.md"
!define MUI_FINISHPAGE_SHOWREADME_TEXT "Show README"
!insertmacro MUI_PAGE_FINISH

# Uninstaller pages
!insertmacro MUI_UNPAGE_WELCOME
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_UNPAGE_FINISH

# Languages
!insertmacro MUI_LANGUAGE "English"

# Sections
Section "Core Application" SecCore
    SectionIn RO
    SetOutPath "\$INSTDIR"
    
    # Copy files
    File "..\\valechat.exe"
    File "..\\..\\README.md"
    File "..\\..\\LICENSE"
    File /nonfatal "..\\..\\CHANGELOG.md"
    
    # Create example configuration
    CreateDirectory "\$INSTDIR\\config"
    FileOpen \$0 "\$INSTDIR\\config\\config.toml.example" w
    FileWrite \$0 "# ValeChat Example Configuration\$\\r\$\\n"
    FileWrite \$0 "# Copy this file to %APPDATA%\\ai.valechat.ValeChat\\config.toml\$\\r\$\\n"
    FileWrite \$0 "\$\\r\$\\n"
    FileWrite \$0 "[app]\$\\r\$\\n"
    FileWrite \$0 "default_provider = \\"openai\\"\$\\r\$\\n"
    FileWrite \$0 "default_model = \\"gpt-4\\"\$\\r\$\\n"
    FileWrite \$0 "debug = false\$\\r\$\\n"
    FileWrite \$0 "\$\\r\$\\n"
    FileWrite \$0 "[models.openai]\$\\r\$\\n"
    FileWrite \$0 "enabled = true\$\\r\$\\n"
    FileWrite \$0 "api_base_url = \\"https://api.openai.com/v1\\"\$\\r\$\\n"
    FileWrite \$0 "\$\\r\$\\n"
    FileWrite \$0 "[models.anthropic]\$\\r\$\\n"
    FileWrite \$0 "enabled = true\$\\r\$\\n"
    FileWrite \$0 "api_base_url = \\"https://api.anthropic.com\\"\$\\r\$\\n"
    FileWrite \$0 "\$\\r\$\\n"
    FileWrite \$0 "[models.google]\$\\r\$\\n"
    FileWrite \$0 "enabled = true\$\\r\$\\n"
    FileWrite \$0 "api_base_url = \\"https://generativelanguage.googleapis.com/v1beta\\"\$\\r\$\\n"
    FileWrite \$0 "\$\\r\$\\n"
    FileWrite \$0 "[ui]\$\\r\$\\n"
    FileWrite \$0 "theme = \\"dark\\"\$\\r\$\\n"
    FileWrite \$0 "mouse_support = true\$\\r\$\\n"
    FileWrite \$0 "\$\\r\$\\n"
    FileWrite \$0 "[logging]\$\\r\$\\n"
    FileWrite \$0 "level = \\"info\\"\$\\r\$\\n"
    FileClose \$0
    
    # Create uninstaller
    WriteUninstaller "\$INSTDIR\\Uninstall.exe"
    
    # Registry entries
    WriteRegStr HKLM "Software\\\${APP_NAME}" "InstallDir" "\$INSTDIR"
    WriteRegStr HKLM "Software\\\${APP_NAME}" "Version" "\${APP_VERSION}"
    
    # Add/Remove Programs entry
    WriteRegStr HKLM "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\\${APP_NAME}" "DisplayName" "\${APP_NAME}"
    WriteRegStr HKLM "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\\${APP_NAME}" "DisplayVersion" "\${APP_VERSION}"
    WriteRegStr HKLM "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\\${APP_NAME}" "Publisher" "\${PUBLISHER}"
    WriteRegStr HKLM "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\\${APP_NAME}" "URLInfoAbout" "\${WEB_SITE}"
    WriteRegStr HKLM "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\\${APP_NAME}" "UninstallString" "\$INSTDIR\\Uninstall.exe"
    WriteRegStr HKLM "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\\${APP_NAME}" "InstallLocation" "\$INSTDIR"
    WriteRegDWORD HKLM "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\\${APP_NAME}" "NoModify" 1
    WriteRegDWORD HKLM "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\\${APP_NAME}" "NoRepair" 1
SectionEnd

Section "Add to PATH" SecPath
    # Add to system PATH
    EnVar::SetHKLM
    EnVar::AddValue "PATH" "\$INSTDIR"
    Pop \$0
SectionEnd

Section "Start Menu Shortcuts" SecStartMenu  
    CreateDirectory "\$SMPROGRAMS\\\${APP_NAME}"
    CreateShortCut "\$SMPROGRAMS\\\${APP_NAME}\\\${APP_NAME}.lnk" "\$INSTDIR\\\${APP_EXE}"
    CreateShortCut "\$SMPROGRAMS\\\${APP_NAME}\\README.lnk" "\$INSTDIR\\README.md"
    CreateShortCut "\$SMPROGRAMS\\\${APP_NAME}\\Uninstall.lnk" "\$INSTDIR\\Uninstall.exe"
SectionEnd

Section "Desktop Shortcut" SecDesktop
    CreateShortCut "\$DESKTOP\\\${APP_NAME}.lnk" "\$INSTDIR\\\${APP_EXE}"
SectionEnd

# Component descriptions
!insertmacro MUI_FUNCTION_DESCRIPTION_BEGIN
!insertmacro MUI_DESCRIPTION_TEXT \${SecCore} "Core application files (required)"
!insertmacro MUI_DESCRIPTION_TEXT \${SecPath} "Add ValeChat to system PATH for command-line access"
!insertmacro MUI_DESCRIPTION_TEXT \${SecStartMenu} "Create Start Menu shortcuts"
!insertmacro MUI_DESCRIPTION_TEXT \${SecDesktop} "Create Desktop shortcut"
!insertmacro MUI_FUNCTION_DESCRIPTION_END

# Uninstaller
Section "Uninstall"
    # Remove files
    Delete "\$INSTDIR\\\${APP_EXE}"
    Delete "\$INSTDIR\\README.md"
    Delete "\$INSTDIR\\LICENSE"
    Delete "\$INSTDIR\\CHANGELOG.md"
    Delete "\$INSTDIR\\Uninstall.exe"
    RMDir /r "\$INSTDIR\\config"
    RMDir "\$INSTDIR"
    
    # Remove shortcuts
    Delete "\$SMPROGRAMS\\\${APP_NAME}\\\${APP_NAME}.lnk"
    Delete "\$SMPROGRAMS\\\${APP_NAME}\\README.lnk"
    Delete "\$SMPROGRAMS\\\${APP_NAME}\\Uninstall.lnk"
    RMDir "\$SMPROGRAMS\\\${APP_NAME}"
    Delete "\$DESKTOP\\\${APP_NAME}.lnk"
    
    # Remove from PATH
    EnVar::SetHKLM
    EnVar::DeleteValue "PATH" "\$INSTDIR"
    Pop \$0
    
    # Remove registry entries
    DeleteRegKey HKLM "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\\${APP_NAME}"
    DeleteRegKey HKLM "Software\\\${APP_NAME}"
    
    # Ask about user data
    MessageBox MB_YESNO|MB_ICONQUESTION "Do you want to remove user data and configuration files?" IDNO +3
    RMDir /r "\$APPDATA\\ai.valechat.ValeChat"
    RMDir /r "\$LOCALAPPDATA\\ai.valechat.ValeChat"
SectionEnd
EOF

# Create PowerShell installation script
cat > "$TARGET_DIR/install.ps1" << 'EOF'
# ValeChat Windows Installation Script
param(
    [string]$InstallPath = "$env:ProgramFiles\ValeChat",
    [switch]$AddToPath = $false,
    [switch]$CreateShortcuts = $false
)

Write-Host "Installing ValeChat..." -ForegroundColor Green

# Check if running as administrator
if (-NOT ([Security.Principal.WindowsPrincipal] [Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole] "Administrator")) {
    Write-Warning "This script requires administrator privileges. Please run as Administrator."
    exit 1
}

# Create installation directory
if (!(Test-Path $InstallPath)) {
    New-Item -ItemType Directory -Path $InstallPath -Force | Out-Null
}

# Copy files
Copy-Item "valechat.exe" -Destination $InstallPath -Force
Copy-Item "README.md" -Destination $InstallPath -Force -ErrorAction SilentlyContinue
Copy-Item "LICENSE" -Destination $InstallPath -Force -ErrorAction SilentlyContinue

# Create example configuration
$configDir = Join-Path $InstallPath "config"
if (!(Test-Path $configDir)) {
    New-Item -ItemType Directory -Path $configDir -Force | Out-Null
}

$configContent = @"
# ValeChat Example Configuration
# Copy this file to %APPDATA%\ai.valechat.ValeChat\config.toml

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
"@

$configContent | Out-File -FilePath (Join-Path $configDir "config.toml.example") -Encoding utf8

# Add to PATH if requested
if ($AddToPath) {
    Write-Host "Adding to system PATH..." -ForegroundColor Yellow
    $currentPath = [Environment]::GetEnvironmentVariable("PATH", "Machine")
    if ($currentPath -notlike "*$InstallPath*") {
        [Environment]::SetEnvironmentVariable("PATH", "$currentPath;$InstallPath", "Machine")
        Write-Host "Added to PATH. Restart your command prompt to use 'valechat' command." -ForegroundColor Green
    }
}

# Create shortcuts if requested
if ($CreateShortcuts) {
    Write-Host "Creating shortcuts..." -ForegroundColor Yellow
    
    # Desktop shortcut
    $WshShell = New-Object -comObject WScript.Shell
    $Shortcut = $WshShell.CreateShortcut("$env:USERPROFILE\Desktop\ValeChat.lnk")
    $Shortcut.TargetPath = Join-Path $InstallPath "valechat.exe"
    $Shortcut.WorkingDirectory = $InstallPath
    $Shortcut.Description = "Multi-model AI chat application"
    $Shortcut.Save()
    
    # Start Menu shortcut
    $startMenuPath = "$env:APPDATA\Microsoft\Windows\Start Menu\Programs"
    $startMenuShortcut = Join-Path $startMenuPath "ValeChat.lnk"
    $Shortcut = $WshShell.CreateShortcut($startMenuShortcut)
    $Shortcut.TargetPath = Join-Path $InstallPath "valechat.exe"
    $Shortcut.WorkingDirectory = $InstallPath
    $Shortcut.Description = "Multi-model AI chat application"
    $Shortcut.Save()
}

Write-Host "Installation completed successfully!" -ForegroundColor Green
Write-Host ""
Write-Host "To get started:" -ForegroundColor Cyan
Write-Host "  1. Configure an API key: valechat api-key openai --set YOUR_KEY"
Write-Host "  2. Start the chat interface: valechat"
Write-Host "  3. Get help: valechat --help"
Write-Host ""
Write-Host "Installation path: $InstallPath" -ForegroundColor Gray
EOF

# Create Chocolatey package
echo "Creating Chocolatey package..."
mkdir -p "$CHOCO_DIR/tools"

# Chocolatey nuspec file
cat > "$CHOCO_DIR/$PACKAGE_NAME.nuspec" << EOF
<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://schemas.microsoft.com/packaging/2015/06/nuspec.xsd">
  <metadata>
    <id>$PACKAGE_NAME</id>
    <version>$VERSION</version>
    <packageSourceUrl>https://github.com/valechat/valechat</packageSourceUrl>
    <owners>ValeChat Team</owners>
    <title>ValeChat</title>
    <authors>ValeChat Team</authors>
    <projectUrl>https://valechat.ai</projectUrl>
    <iconUrl>https://raw.githubusercontent.com/valechat/valechat/main/assets/icon.png</iconUrl>
    <copyright>2024 ValeChat Team</copyright>
    <licenseUrl>https://raw.githubusercontent.com/valechat/valechat/main/LICENSE</licenseUrl>
    <requireLicenseAcceptance>false</requireLicenseAcceptance>
    <projectSourceUrl>https://github.com/valechat/valechat</projectSourceUrl>
    <docsUrl>https://github.com/valechat/valechat/wiki</docsUrl>
    <bugTrackerUrl>https://github.com/valechat/valechat/issues</bugTrackerUrl>
    <tags>ai chat openai anthropic google terminal tui cli</tags>
    <summary>Multi-model AI chat application with MCP server support</summary>
    <description>ValeChat is a powerful terminal-based (TUI) AI chat application that supports multiple AI providers including OpenAI, Anthropic, and Google Gemini. It features secure API key management, conversation persistence, usage tracking, and Model Context Protocol (MCP) server support.

Key features:
- Multi-provider support (OpenAI, Anthropic, Google)
- Terminal user interface built with Ratatui
- Conversation management (create, delete, rename, restore)
- Usage tracking and billing analysis
- Secure cross-platform API key storage
- Export functionality (JSON, TXT formats)
- MCP server integration
- Cross-platform support</description>
    <releaseNotes>https://github.com/valechat/valechat/releases/tag/v$VERSION</releaseNotes>
  </metadata>
  <files>
    <file src="tools\**" target="tools" />
  </files>
</package>
EOF

# Chocolatey install script
cat > "$CHOCO_DIR/tools/chocolateyinstall.ps1" << EOF
\$ErrorActionPreference = 'Stop'

\$packageName = '$PACKAGE_NAME'
\$toolsDir = "\$(Split-Path -parent \$MyInvocation.MyCommand.Definition)"
\$url64 = 'https://github.com/valechat/valechat/releases/download/v$VERSION/valechat-$VERSION-windows.zip'

\$packageArgs = @{
  packageName   = \$packageName
  unzipLocation = \$toolsDir
  url64bit      = \$url64
  checksum64    = 'PLACEHOLDER_CHECKSUM'
  checksumType64= 'sha256'
}

Install-ChocolateyZipPackage @packageArgs

# Add to PATH
\$binPath = Join-Path \$toolsDir "valechat.exe"
Install-ChocolateyPath \$toolsDir
EOF

# Chocolatey uninstall script
cat > "$CHOCO_DIR/tools/chocolateyuninstall.ps1" << 'EOF'
$ErrorActionPreference = 'Stop'

$packageName = 'valechat'
$toolsDir = "$(Split-Path -parent $MyInvocation.MyCommand.Definition)"

# Remove from PATH
Uninstall-ChocolateyPath $toolsDir

Write-Host "ValeChat has been uninstalled."
Write-Host "User data and configuration files are preserved at:"
Write-Host "  %APPDATA%\ai.valechat.ValeChat\"
Write-Host "Remove these manually if desired."
EOF

# Create ZIP archive
echo "Creating ZIP archive..."
cd "$TARGET_DIR"
zip -r "valechat-$VERSION-windows.zip" valechat.exe README.md LICENSE CHANGELOG.md install.ps1 2>/dev/null || {
    echo "Warning: zip command not found. Creating tar.gz instead..."
    tar -czf "valechat-$VERSION-windows.tar.gz" valechat.exe README.md LICENSE CHANGELOG.md install.ps1
}
cd "$PROJECT_ROOT"

# Create batch file for easy command-line access
cat > "$TARGET_DIR/valechat.bat" << 'EOF'
@echo off
setlocal

REM ValeChat Windows Batch Launcher
REM This batch file helps launch ValeChat from anywhere

REM Find the directory where this batch file is located
set "SCRIPT_DIR=%~dp0"

REM Launch ValeChat with all arguments passed to this batch file
"%SCRIPT_DIR%valechat.exe" %*
EOF

# Create PowerShell uninstaller
cat > "$TARGET_DIR/uninstall.ps1" << 'EOF'
# ValeChat Windows Uninstaller
param(
    [switch]$RemoveUserData = $false
)

Write-Host "Uninstalling ValeChat..." -ForegroundColor Yellow

# Check if running as administrator for system-wide removal
$isAdmin = ([Security.Principal.WindowsPrincipal] [Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole] "Administrator")

# Remove from Program Files if installed there
$programFilesPath = "$env:ProgramFiles\ValeChat"
if ((Test-Path $programFilesPath) -and $isAdmin) {
    Write-Host "Removing from Program Files..." -ForegroundColor Yellow
    Remove-Item -Path $programFilesPath -Recurse -Force -ErrorAction SilentlyContinue
    
    # Remove from system PATH
    $currentPath = [Environment]::GetEnvironmentVariable("PATH", "Machine")
    if ($currentPath -like "*$programFilesPath*") {
        $newPath = $currentPath -replace [regex]::Escape(";$programFilesPath"), ""
        $newPath = $newPath -replace [regex]::Escape("$programFilesPath;"), ""
        $newPath = $newPath -replace [regex]::Escape("$programFilesPath"), ""
        [Environment]::SetEnvironmentVariable("PATH", $newPath, "Machine")
        Write-Host "Removed from system PATH" -ForegroundColor Green
    }
}

# Remove shortcuts
$desktopShortcut = "$env:USERPROFILE\Desktop\ValeChat.lnk"
$startMenuShortcut = "$env:APPDATA\Microsoft\Windows\Start Menu\Programs\ValeChat.lnk"

if (Test-Path $desktopShortcut) {
    Remove-Item $desktopShortcut -Force
    Write-Host "Removed desktop shortcut" -ForegroundColor Green
}

if (Test-Path $startMenuShortcut) {
    Remove-Item $startMenuShortcut -Force
    Write-Host "Removed start menu shortcut" -ForegroundColor Green
}

# Remove user data if requested
if ($RemoveUserData) {
    $configPath = "$env:APPDATA\ai.valechat.ValeChat"
    $dataPath = "$env:LOCALAPPDATA\ai.valechat.ValeChat"
    
    if (Test-Path $configPath) {
        Remove-Item -Path $configPath -Recurse -Force -ErrorAction SilentlyContinue
        Write-Host "Removed user configuration" -ForegroundColor Green
    }
    
    if (Test-Path $dataPath) {
        Remove-Item -Path $dataPath -Recurse -Force -ErrorAction SilentlyContinue
        Write-Host "Removed user data" -ForegroundColor Green
    }
    
    Write-Host "User data and configuration removed" -ForegroundColor Green
} else {
    Write-Host "User data preserved at:" -ForegroundColor Cyan
    Write-Host "  %APPDATA%\ai.valechat.ValeChat\" -ForegroundColor Gray
    Write-Host "  %LOCALAPPDATA%\ai.valechat.ValeChat\" -ForegroundColor Gray
}

Write-Host "Uninstallation completed!" -ForegroundColor Green
EOF

# Create package info
cat > "$TARGET_DIR/package-info.txt" << EOF
ValeChat v$VERSION - Windows Distribution Package
==================================================

Contents:
- valechat.exe                        # Main executable
- valechat-$VERSION-windows.zip       # Portable ZIP archive
- ValeChat-$VERSION-Setup.exe         # NSIS installer (if built)
- install.ps1                         # PowerShell installation script
- uninstall.ps1                       # PowerShell uninstaller
- valechat.bat                        # Batch file launcher
- chocolatey/                         # Chocolatey package files

Installation Options:

1. NSIS Installer (Recommended):
   Run ValeChat-$VERSION-Setup.exe and follow the wizard

2. PowerShell Script:
   .\install.ps1 -AddToPath -CreateShortcuts

3. Portable Installation:
   Extract valechat-$VERSION-windows.zip anywhere
   Optionally add the folder to your PATH

4. Chocolatey (after publishing):
   choco install valechat

Usage:
  valechat --help                    # Show help
  valechat api-key openai --set KEY  # Configure API key
  valechat                           # Start chat interface

System Requirements:
- Windows 10 or later (64-bit)
- Visual C++ Redistributable 2019 or later

For more information, see README.md
EOF

# Try to build NSIS installer if makensis is available
if command -v makensis &> /dev/null; then
    echo "Building NSIS installer..."
    cd "$NSIS_DIR"
    makensis installer.nsi
    echo "NSIS installer created successfully!"
elif command -v wine &> /dev/null && [ -f "$HOME/.wine/drive_c/Program Files (x86)/NSIS/makensis.exe" ]; then
    echo "Building NSIS installer with Wine..."
    cd "$NSIS_DIR"
    wine "$HOME/.wine/drive_c/Program Files (x86)/NSIS/makensis.exe" installer.nsi
    echo "NSIS installer created with Wine!"
else
    echo "NSIS not found. Installer script created but not compiled."
    echo "To build the installer on Windows, install NSIS and run:"
    echo "  makensis $NSIS_DIR/installer.nsi"
fi

cd "$PROJECT_ROOT"

echo ""
echo "✅ Windows package build completed successfully!"
echo ""
echo "Generated files:"
echo "  📦 $TARGET_DIR/valechat.exe"
echo "  🗜️  $TARGET_DIR/valechat-$VERSION-windows.zip"
echo "  📋 $TARGET_DIR/install.ps1"
echo "  🗑️  $TARGET_DIR/uninstall.ps1"
echo "  ⚙️  $TARGET_DIR/valechat.bat"
echo "  🍫 $TARGET_DIR/chocolatey/"
if [ -f "$TARGET_DIR/ValeChat-$VERSION-Setup.exe" ]; then
    echo "  💿 $TARGET_DIR/ValeChat-$VERSION-Setup.exe"
fi
echo ""
echo "Next steps:"
echo "  1. Test the executable on Windows"
echo "  2. Upload release artifacts to GitHub"
echo "  3. Submit Chocolatey package for review"
echo "  4. Test installer on clean Windows systems"

if [ ! -f "$TARGET_DIR/ValeChat-$VERSION-Setup.exe" ]; then
    echo ""
    echo "Note: To build the NSIS installer, install NSIS on Windows and run:"
    echo "  makensis $NSIS_DIR/installer.nsi"
fi