#!/usr/bin/env bash
# install.sh - Install quickctx on Linux, macOS, or Android (Termux)
# Usage: curl -fsSL https://raw.githubusercontent.com/CaddyGlow/quickctx/main/install.sh | bash
# Usage: wget -qO- https://raw.githubusercontent.com/CaddyGlow/quickctx/main/install.sh | bash

set -e

# Configuration
REPO="CaddyGlow/quickctx"
APP_NAME="quickctx"
INSTALL_DIR="${QUICKCTX_INSTALL_DIR:-$HOME/.local/bin}"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
GRAY='\033[0;90m'
NC='\033[0m' # No Color

# Helper functions
info() {
    echo -e "${CYAN}$1${NC}"
}

success() {
    echo -e "${GREEN}✓ $1${NC}"
}

warning() {
    echo -e "${YELLOW}⚠ $1${NC}"
}

error() {
    echo -e "${RED}✗ Error: $1${NC}" >&2
}

# Detect OS and architecture
detect_platform() {
    local os arch target

    # Detect OS
    case "$(uname -s)" in
        Linux*)
            if [ -n "$ANDROID_ROOT" ] || [ -n "$TERMUX_VERSION" ]; then
                os="android"
            else
                os="linux"
            fi
            ;;
        Darwin*)
            os="darwin"
            ;;
        *)
            error "Unsupported operating system: $(uname -s)"
            exit 1
            ;;
    esac

    # Detect architecture
    case "$(uname -m)" in
        x86_64|amd64)
            arch="x86_64"
            ;;
        aarch64|arm64)
            arch="aarch64"
            ;;
        armv7l|armv7)
            arch="armv7"
            ;;
        i686|i386)
            arch="i686"
            ;;
        *)
            error "Unsupported architecture: $(uname -m)"
            exit 1
            ;;
    esac

    # Construct target triple
    case "$os" in
        linux)
            target="${arch}-unknown-linux-gnu"
            # Check for musl
            if ldd --version 2>&1 | grep -q musl; then
                target="${arch}-unknown-linux-musl"
            fi
            ;;
        darwin)
            target="${arch}-apple-darwin"
            ;;
        android)
            target="${arch}-linux-android"
            ;;
    esac

    echo "$target"
}

# Download file with fallback
download() {
    local url="$1"
    local output="$2"

    if command -v curl >/dev/null 2>&1; then
        curl -fsSL "$url" -o "$output"
    elif command -v wget >/dev/null 2>&1; then
        wget -q "$url" -O "$output"
    else
        error "Neither curl nor wget found. Please install one of them."
        exit 1
    fi
}

# Extract archive
extract_archive() {
    local archive="$1"
    local dest="$2"

    if [[ "$archive" == *.tar.gz ]] || [[ "$archive" == *.tgz ]]; then
        tar -xzf "$archive" -C "$dest"
    elif [[ "$archive" == *.zip ]]; then
        if command -v unzip >/dev/null 2>&1; then
            unzip -q "$archive" -d "$dest"
        else
            error "unzip not found. Please install unzip to extract .zip archives."
            exit 1
        fi
    else
        error "Unsupported archive format: $archive"
        exit 1
    fi
}

# Main installation
main() {
    info "Installing $APP_NAME..."

    # Detect platform
    local target
    target=$(detect_platform)
    info "Detected platform: $target"

    # Fetch latest release
    info "Fetching latest release from GitHub..."
    local api_url="https://api.github.com/repos/$REPO/releases/latest"
    local release_json
    local tmp_json="/tmp/quickctx-release.json"

    download "$api_url" "$tmp_json"

    # Parse version
    local version
    version=$(grep -o '"tag_name": *"[^"]*"' "$tmp_json" | head -1 | sed 's/.*: *"\(.*\)".*/\1/')

    if [ -z "$version" ]; then
        error "Failed to fetch release information"
        rm -f "$tmp_json"
        exit 1
    fi

    success "Latest version: $version"

    # Find the right asset
    # Try to find target-specific archive first, then fall back to generic patterns
    local download_url archive_name

    # Extract all download URLs and names
    if grep -q "\"name\": *\"$APP_NAME.*$target" "$tmp_json"; then
        # Target-specific binary found
        archive_name=$(grep -o "\"name\": *\"$APP_NAME[^\"]*$target[^\"]*\"" "$tmp_json" | head -1 | sed 's/.*: *"\(.*\)".*/\1/')
        download_url=$(grep -B 3 "\"name\": *\"$archive_name\"" "$tmp_json" | grep browser_download_url | sed 's/.*: *"\(.*\)".*/\1/')
    else
        # Try platform-specific patterns
        case "$target" in
            *linux*)
                archive_name=$(grep -o "\"name\": *\"$APP_NAME[^\"]*linux[^\"]*\"" "$tmp_json" | head -1 | sed 's/.*: *"\(.*\)".*/\1/')
                ;;
            *darwin*)
                archive_name=$(grep -o "\"name\": *\"$APP_NAME[^\"]*darwin[^\"]*\"" "$tmp_json" | head -1 | sed 's/.*: *"\(.*\)".*/\1/')
                ;;
            *android*)
                archive_name=$(grep -o "\"name\": *\"$APP_NAME[^\"]*android[^\"]*\"" "$tmp_json" | head -1 | sed 's/.*: *"\(.*\)".*/\1/')
                ;;
        esac

        if [ -n "$archive_name" ]; then
            download_url=$(grep -B 3 "\"name\": *\"$archive_name\"" "$tmp_json" | grep browser_download_url | sed 's/.*: *"\(.*\)".*/\1/')
        fi
    fi

    rm -f "$tmp_json"

    if [ -z "$download_url" ]; then
        error "Could not find binary for $target in release assets."
        echo ""
        echo -e "${YELLOW}Troubleshooting:${NC}"
        echo -e "${GRAY}  1. Check available releases: https://github.com/$REPO/releases${NC}"
        echo -e "${GRAY}  2. Try manual installation from releases page${NC}"
        echo -e "${GRAY}  3. Your platform: $target${NC}"
        exit 1
    fi

    info "Downloading $archive_name..."
    local tmp_archive="/tmp/$archive_name"
    download "$download_url" "$tmp_archive"

    local archive_size
    archive_size=$(du -h "$tmp_archive" | cut -f1)
    success "Download complete: $archive_size"

    # Create install directory
    info "Installing to $INSTALL_DIR..."
    mkdir -p "$INSTALL_DIR"

    # Extract to temporary directory
    local tmp_dir="/tmp/quickctx-extract"
    rm -rf "$tmp_dir"
    mkdir -p "$tmp_dir"

    extract_archive "$tmp_archive" "$tmp_dir"

    # Find and move the binary
    local binary_path
    binary_path=$(find "$tmp_dir" -type f -name "$APP_NAME" | head -1)

    if [ -z "$binary_path" ]; then
        error "Binary not found in extracted archive"
        rm -rf "$tmp_dir" "$tmp_archive"
        exit 1
    fi

    # Install binary
    local install_path="$INSTALL_DIR/$APP_NAME"

    # Remove old version if exists
    if [ -f "$install_path" ]; then
        rm -f "$install_path"
    fi

    mv "$binary_path" "$install_path"
    chmod +x "$install_path"

    # Clean up
    rm -rf "$tmp_dir" "$tmp_archive"

    success "$APP_NAME $version installed successfully!"
    echo -e "${GRAY}  Installed to: $install_path${NC}"

    # Check if in PATH
    if echo "$PATH" | grep -q "$INSTALL_DIR"; then
        success "Installation directory is in PATH"
    else
        warning "Installation directory not in PATH"
        echo ""
        echo "Add the following to your shell profile (~/.bashrc, ~/.zshrc, etc.):"
        echo ""
        echo -e "${CYAN}  export PATH=\"\$PATH:$INSTALL_DIR\"${NC}"
        echo ""
        echo "Then reload your shell or run: source ~/.bashrc (or ~/.zshrc)"
    fi

    # Test installation
    echo ""
    info "Verifying installation..."
    if "$install_path" --version >/dev/null 2>&1; then
        success "Installation verified"
        echo ""
        echo -e "${CYAN}Get started with:${NC}"
        echo -e "${GRAY}  $APP_NAME --help${NC}"
        echo -e "${GRAY}  $APP_NAME update    # Check for updates${NC}"
    else
        warning "Installation completed but verification failed"
        echo "Try running: $install_path --version"
    fi
}

# Trap errors
trap 'error "Installation failed"; exit 1' ERR

# Run main
main "$@"
