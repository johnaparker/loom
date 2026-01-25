#!/bin/bash
set -euo pipefail

# Grove installer script
# Usage: curl -fsSL https://github.com/johnaparker/grove/releases/latest/download/install.sh | bash

VERSION="0.1.0"  # Updated by release workflow
INSTALL_DIR="${GROVE_INSTALL_DIR:-$HOME/.local/bin}"
REPO="johnaparker/grove"

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
    target_file="grove-${VERSION}-${platform}.tar.gz"
    url="https://github.com/${REPO}/releases/download/v${VERSION}/${target_file}"

    echo "Installing grove v${VERSION} for ${platform}..."

    # Create install directory
    mkdir -p "$INSTALL_DIR"

    # Download and extract
    echo "Downloading from ${url}..."
    curl -fsSL "$url" | tar -xz -C "$INSTALL_DIR"

    # Make executable
    chmod +x "${INSTALL_DIR}/grove"

    echo ""
    echo "grove installed to ${INSTALL_DIR}/grove"

    # Check if install dir is in PATH
    if [[ ":$PATH:" != *":${INSTALL_DIR}:"* ]]; then
        echo ""
        echo "Add grove to your PATH by adding this to your shell config:"
        echo ""
        echo "  export PATH=\"\$HOME/.local/bin:\$PATH\""
        echo ""
    else
        echo ""
        echo "Run 'grove --help' to get started."
    fi
}

main
