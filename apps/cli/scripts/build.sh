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

# Get version from package.json or use argument
VERSION=${1:-$(node -p "require('./package.json').version")}
DIST_DIR="dist"

echo "Building Tealstreet CLI v${VERSION}..."
echo "Using bun at: $BUN"

# Clean
rm -rf $DIST_DIR
mkdir -p $DIST_DIR

# Build for each platform
PLATFORMS=(
  "darwin-arm64"
  "darwin-x64"
  "linux-x64"
  "linux-arm64"
)

for PLATFORM in "${PLATFORMS[@]}"; do
  echo "Building for ${PLATFORM}..."

  TARGET=""
  case $PLATFORM in
    darwin-arm64) TARGET="bun-darwin-arm64" ;;
    darwin-x64) TARGET="bun-darwin-x64" ;;
    linux-x64) TARGET="bun-linux-x64" ;;
    linux-arm64) TARGET="bun-linux-arm64" ;;
  esac

  OUTPUT="${DIST_DIR}/tealstreet-${PLATFORM}"

  # Production URLs - bundled into the release
  # These can be overridden at runtime via env vars for local development
  PROD_WEB_URL="https://preview.tealstreet.io"
  PROD_RELAY_URL="https://exchange.tealstreet.io"
  PROD_RELAY_WS="wss://exchange.tealstreet.io/account"

  $BUN build src/index.ts \
    --compile \
    --minify \
    --target=$TARGET \
    --outfile=$OUTPUT \
    --define "process.env.TEALSTREET_CLI_VERSION='${VERSION}'" \
    --define "process.env.TEALSTREET_API='${PROD_WEB_URL}'" \
    --define "process.env.TEALSTREET_WEB_URL='${PROD_WEB_URL}'" \
    --define "process.env.TEALSTREET_EXCHANGE_RELAY_URL='${PROD_RELAY_URL}'" \
    --define "process.env.TEALSTREET_EXCHANGE_RELAY_WS='${PROD_RELAY_WS}'"

  # Make executable
  chmod +x $OUTPUT
done

echo ""
echo "Build complete!"
ls -lh $DIST_DIR/
