#!/bin/bash
set -euo pipefail

# Loom installer script
# Usage: curl -fsSL https://github.com/johnaparker/loom-tui/releases/latest/download/install.sh | bash

VERSION="0.1.0"  # Updated by release workflow
INSTALL_DIR="${LOOM_INSTALL_DIR:-$HOME/.local/bin}"
REPO="johnaparker/loom-tui"

# Detect platform
detect_platform() {
    local os arch

    os="$(uname -s)"
    arch="$(uname -m)"

    case "$os" in
        Darwin) os="apple-darwin" ;;
        Linux) os="unknown-linux-gnu" ;;
        *)
            echo "Error: Unsupported operating system: $os"
            exit 1
            ;;
    esac

    case "$arch" in
        x86_64) arch="x86_64" ;;
        aarch64|arm64) arch="aarch64" ;;
        *)
            echo "Error: Unsupported architecture: $arch"
            exit 1
            ;;
    esac

    echo "${arch}-${os}"
}

main() {
    local platform target_file url

    platform="$(detect_platform)"
    target_file="loom-${VERSION}-${platform}.tar.gz"
    url="https://github.com/${REPO}/releases/download/v${VERSION}/${target_file}"

    echo "Installing loom v${VERSION} for ${platform}..."

    # Create install directory
    mkdir -p "$INSTALL_DIR"

    # Download and extract
    echo "Downloading from ${url}..."
    curl -fsSL "$url" | tar -xz -C "$INSTALL_DIR"

    # Make executable
    chmod +x "${INSTALL_DIR}/loom"

    echo ""
    echo "loom installed to ${INSTALL_DIR}/loom"

    # Check if install dir is in PATH
    if [[ ":$PATH:" != *":${INSTALL_DIR}:"* ]]; then
        echo ""
        echo "Add loom to your PATH by adding this to your shell config:"
        echo ""
        echo "  export PATH=\"\$HOME/.local/bin:\$PATH\""
        echo ""
    else
        echo ""
        echo "Run 'loom --help' to get started."
    fi
}

main
