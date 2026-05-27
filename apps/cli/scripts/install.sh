#!/bin/bash
# Tealstreet CLI Installer
# Usage: curl -fsSL https://get.tealstreet.io/cli | bash
#    or: curl -fsSL https://raw.githubusercontent.com/Tealstreet/cli/main/install.sh | bash

set -e

REPO="Tealstreet/cli"
INSTALL_DIR="/usr/local/bin"
BINARY_NAME="tealstreet"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

info() { echo -e "${GREEN}[INFO]${NC} $1"; }
warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
error() { echo -e "${RED}[ERROR]${NC} $1"; exit 1; }

# Detect OS and architecture
detect_platform() {
  OS=$(uname -s | tr '[:upper:]' '[:lower:]')
  ARCH=$(uname -m)

  case "$OS" in
    darwin) OS="darwin" ;;
    linux) OS="linux" ;;
    *) error "Unsupported OS: $OS" ;;
  esac

  case "$ARCH" in
    x86_64|amd64) ARCH="x64" ;;
    arm64|aarch64) ARCH="arm64" ;;
    *) error "Unsupported architecture: $ARCH" ;;
  esac

  PLATFORM="${OS}-${ARCH}"
  info "Detected platform: $PLATFORM"
}

# Get latest version from GitHub
get_latest_version() {
  info "Fetching latest version..."
  VERSION=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" | grep '"tag_name"' | sed -E 's/.*"v([^"]+)".*/\1/')

  if [ -z "$VERSION" ]; then
    error "Failed to get latest version"
  fi

  info "Latest version: v$VERSION"
}

# Download and install binary
install_binary() {
  DOWNLOAD_URL="https://github.com/${REPO}/releases/download/v${VERSION}/tealstreet-${PLATFORM}"
  TMP_FILE=$(mktemp)

  info "Downloading from: $DOWNLOAD_URL"

  if ! curl -fsSL "$DOWNLOAD_URL" -o "$TMP_FILE"; then
    error "Failed to download binary"
  fi

  # Check if we need sudo
  if [ -w "$INSTALL_DIR" ]; then
    mv "$TMP_FILE" "$INSTALL_DIR/$BINARY_NAME"
    chmod +x "$INSTALL_DIR/$BINARY_NAME"
  else
    info "Requesting sudo access to install to $INSTALL_DIR..."
    sudo mv "$TMP_FILE" "$INSTALL_DIR/$BINARY_NAME"
    sudo chmod +x "$INSTALL_DIR/$BINARY_NAME"
  fi

  info "Installed to: $INSTALL_DIR/$BINARY_NAME"
}

# Verify installation
verify_installation() {
  if command -v tealstreet &> /dev/null; then
    echo ""
    info "Installation successful!"
    echo ""
    tealstreet --version
    echo ""
    echo "Get started:"
    echo "  tealstreet --help"
    echo "  tealstreet account add -n myaccount -e bybit"
    echo "  tealstreet"
    echo ""
  else
    warn "Binary installed but not in PATH"
    echo "Add $INSTALL_DIR to your PATH or run:"
    echo "  $INSTALL_DIR/$BINARY_NAME"
  fi
}

# Main
main() {
  echo ""
  echo "  ╔════════════════════════════════════╗"
  echo "  ║     Tealstreet CLI Installer       ║"
  echo "  ╚════════════════════════════════════╝"
  echo ""

  detect_platform
  get_latest_version
  install_binary
  verify_installation
}

main
