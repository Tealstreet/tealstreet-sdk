#!/bin/bash
set -e

# Find bun - check PATH first, then common locations
BUN=$(which bun 2>/dev/null || echo "")
if [ -z "$BUN" ]; then
  if [ -x "$HOME/.bun/bin/bun" ]; then
    BUN="$HOME/.bun/bin/bun"
  else
    echo "Error: Bun is not installed."
    echo "Install it with: curl -fsSL https://bun.sh/install | bash"
    exit 1
  fi
fi

# Usage: build.sh [brand] [version]
#   brand   tealstreet (default) | disrupt — see src/brand.ts. Selects the
#           binary name, the web app the CLI logs in through, and which
#           release repo the updater polls. Relay URLs are shared.
#   version defaults to package.json's version.
BRAND="${1:-tealstreet}"
case "$BRAND" in
  tealstreet) BIN_NAME="tealstreet"; PROD_WEB_URL="https://preview.tealstreet.io" ;;
  disrupt)    BIN_NAME="disrupt";    PROD_WEB_URL="https://disrupt.trade" ;;
  *) echo "Unknown brand: $BRAND (expected tealstreet|disrupt)"; exit 1 ;;
esac
VERSION=${2:-$(node -p "require('./package.json').version")}
DIST_DIR="dist"

cleanup_bun_build_artifacts() {
  local artifacts=(./.*.bun-build)
  if [ -e "${artifacts[0]}" ]; then
    rm -f "${artifacts[@]}"
  fi
}

trap cleanup_bun_build_artifacts EXIT

echo "Building ${BRAND} CLI v${VERSION}..."
echo "Using bun at: $BUN"

# Clean
rm -rf "$DIST_DIR"
cleanup_bun_build_artifacts
mkdir -p "$DIST_DIR"

# Build for each platform
PLATFORMS=(
  "darwin-arm64"
  "darwin-x64"
  "linux-x64"
  "linux-arm64"
  "windows-x64"
)

for PLATFORM in "${PLATFORMS[@]}"; do
  echo "Building for ${PLATFORM}..."

  TARGET=""
  # `bun build --compile` appends .exe itself on a Windows target, so EXT is
  # what the produced file is actually called — not what we pass to --outfile.
  EXT=""
  case $PLATFORM in
    darwin-arm64) TARGET="bun-darwin-arm64" ;;
    darwin-x64) TARGET="bun-darwin-x64" ;;
    linux-x64) TARGET="bun-linux-x64" ;;
    linux-arm64) TARGET="bun-linux-arm64" ;;
    windows-x64) TARGET="bun-windows-x64"; EXT=".exe" ;;
  esac

  OUTPUT="${DIST_DIR}/${BIN_NAME}-${PLATFORM}"

  # Production URLs - bundled into the release
  # These can be overridden at runtime via env vars for local development.
  # PROD_WEB_URL is per brand (above); the relay is shared infrastructure.
  PROD_RELAY_URL="https://connect.tealstreet.io"
  PROD_RELAY_WS="wss://connect.tealstreet.io/account"

  "$BUN" build src/index.ts \
    --compile \
    --minify \
    --sourcemap=none \
    --target="$TARGET" \
    --outfile="$OUTPUT" \
    --define "process.env.TEALSTREET_CLI_VERSION='${VERSION}'" \
    --define "process.env.TEALSTREET_BRAND='${BRAND}'" \
    --define "process.env.TEALSTREET_API='${PROD_WEB_URL}'" \
    --define "process.env.TEALSTREET_WEB_URL='${PROD_WEB_URL}'" \
    --define "process.env.TEALSTREET_EXCHANGE_RELAY_URL='${PROD_RELAY_URL}'" \
    --define "process.env.TEALSTREET_EXCHANGE_RELAY_WS='${PROD_RELAY_WS}'"

  # Make executable
  chmod +x "${OUTPUT}${EXT}"
done

# Bun compile can leave intermediate JS output next to the compiled binaries.
# Those files are not release artifacts and should never be uploaded.
rm -f "$DIST_DIR"/index.js "$DIST_DIR"/index.js.map

bash ./scripts/check-release-artifacts.sh

echo ""
echo "Build complete!"
ls -lh "$DIST_DIR"/
