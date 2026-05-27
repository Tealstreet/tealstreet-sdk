#!/bin/bash
set -e

# Release script: builds all platforms and creates a GitHub release
# Releases to the PUBLIC cli repo for distribution
#
# To bump version before releasing:
#   yarn version:patch && yarn release
#   yarn version:minor && yarn release
#   yarn version:major && yarn release

cd "$(dirname "$0")/.."

# Configuration
PUBLIC_REPO="Tealstreet/cli"  # Public repo for releases
VERSION=$(node -p "require('./package.json').version")
TAG="v${VERSION}"

echo "=== Tealstreet CLI Release v${VERSION} ==="
echo ""
echo "Publishing to: ${PUBLIC_REPO}"
echo ""

# Check if gh is installed
if ! command -v gh &> /dev/null; then
  echo "Error: GitHub CLI (gh) is not installed."
  echo "Install it with: brew install gh"
  exit 1
fi

# Check if bun is installed
if ! command -v bun &> /dev/null; then
  if [ -x "$HOME/.bun/bin/bun" ]; then
    export PATH="$HOME/.bun/bin:$PATH"
  else
    echo "Error: Bun is not installed."
    echo "Install it with: curl -fsSL https://bun.sh/install | bash"
    exit 1
  fi
fi

# Check if logged in to gh (either via gh auth or GH_TOKEN env var)
if [ -z "$GH_TOKEN" ] && ! gh auth status &> /dev/null; then
  echo "Error: Not logged in to GitHub CLI."
  echo "Run: gh auth login"
  echo "Or set GH_TOKEN environment variable"
  exit 1
fi

# Build all platforms
echo ""
echo "Building binaries..."
./scripts/build.sh "$VERSION"

# Calculate SHA256 checksums for Homebrew
echo ""
echo "Calculating checksums..."
SHA_DARWIN_ARM64=$(shasum -a 256 dist/tealstreet-darwin-arm64 | cut -d ' ' -f 1)
SHA_DARWIN_X64=$(shasum -a 256 dist/tealstreet-darwin-x64 | cut -d ' ' -f 1)
SHA_LINUX_X64=$(shasum -a 256 dist/tealstreet-linux-x64 | cut -d ' ' -f 1)
SHA_LINUX_ARM64=$(shasum -a 256 dist/tealstreet-linux-arm64 | cut -d ' ' -f 1)

echo "  darwin-arm64: $SHA_DARWIN_ARM64"
echo "  darwin-x64:   $SHA_DARWIN_X64"
echo "  linux-x64:    $SHA_LINUX_X64"
echo "  linux-arm64:  $SHA_LINUX_ARM64"

# Read release notes from RELEASE_NOTES.md if it exists.
#
# Format: each release lives under its own "## v<VERSION> — <DATE>" heading.
# We extract everything from the matching version heading down to (but not
# including) the next "## v..." heading (or EOF). If no matching section
# exists, the release body falls back to the install/checksum boilerplate
# below.
RELEASE_NOTES_FILE="RELEASE_NOTES.md"
WHATS_NEW=""
if [ -f "$RELEASE_NOTES_FILE" ]; then
  WHATS_NEW=$(awk -v ver="v${VERSION}" '
    $0 ~ "^## " ver "( |\\b)" { capture = 1; next }
    capture && /^## v/ { exit }
    capture { print }
  ' "$RELEASE_NOTES_FILE" | sed -e '/./,$!d' | sed -e :a -e '/^$/N;/\n$/ba')
  if [ -n "$WHATS_NEW" ]; then
    echo "Release notes for v${VERSION} found:"
    echo "$WHATS_NEW"
    echo ""
  else
    echo "Warning: no '## v${VERSION}' section found in $RELEASE_NOTES_FILE."
    echo "Release body will be install/checksum boilerplate only."
    echo ""
  fi
fi

# Check if tag already exists on PUBLIC repo
if gh release view "$TAG" --repo "$PUBLIC_REPO" &> /dev/null; then
  echo ""
  echo "Warning: Release $TAG already exists on ${PUBLIC_REPO}."
  read -p "Delete and recreate? (y/N) " -n 1 -r
  echo
  if [[ $REPLY =~ ^[Yy]$ ]]; then
    gh release delete "$TAG" --repo "$PUBLIC_REPO" --yes 2>/dev/null || true
  else
    echo "Aborting."
    exit 1
  fi
fi

# Build release notes body
RELEASE_BODY=""
if [ -n "$WHATS_NEW" ]; then
  RELEASE_BODY="## What's New

$WHATS_NEW

"
fi

RELEASE_BODY="${RELEASE_BODY}## Installation

### Quick Install (Recommended)
\`\`\`bash
curl -fsSL https://raw.githubusercontent.com/${PUBLIC_REPO}/main/install.sh | bash
\`\`\`

### Homebrew (macOS/Linux)
\`\`\`bash
brew tap tealstreet/tap
brew install tealstreet
\`\`\`

### Manual Download

#### macOS (Apple Silicon)
\`\`\`bash
curl -fsSL https://github.com/${PUBLIC_REPO}/releases/download/${TAG}/tealstreet-darwin-arm64 -o /usr/local/bin/tealstreet
chmod +x /usr/local/bin/tealstreet
\`\`\`

#### macOS (Intel)
\`\`\`bash
curl -fsSL https://github.com/${PUBLIC_REPO}/releases/download/${TAG}/tealstreet-darwin-x64 -o /usr/local/bin/tealstreet
chmod +x /usr/local/bin/tealstreet
\`\`\`

#### Linux (x64)
\`\`\`bash
curl -fsSL https://github.com/${PUBLIC_REPO}/releases/download/${TAG}/tealstreet-linux-x64 -o /usr/local/bin/tealstreet
chmod +x /usr/local/bin/tealstreet
\`\`\`

#### Linux (ARM64)
\`\`\`bash
curl -fsSL https://github.com/${PUBLIC_REPO}/releases/download/${TAG}/tealstreet-linux-arm64 -o /usr/local/bin/tealstreet
chmod +x /usr/local/bin/tealstreet
\`\`\`

## Usage
\`\`\`bash
tealstreet --help
tealstreet account add -n myaccount -e bybit
tealstreet
\`\`\`

## Checksums (SHA256)
\`\`\`
${SHA_DARWIN_ARM64}  tealstreet-darwin-arm64
${SHA_DARWIN_X64}  tealstreet-darwin-x64
${SHA_LINUX_X64}  tealstreet-linux-x64
${SHA_LINUX_ARM64}  tealstreet-linux-arm64
\`\`\`"

# Create GitHub release with binaries on PUBLIC repo
echo ""
echo "Creating GitHub release on ${PUBLIC_REPO}..."
gh release create "$TAG" \
  --repo "$PUBLIC_REPO" \
  --title "Tealstreet CLI v${VERSION}" \
  --notes "$RELEASE_BODY" \
  dist/tealstreet-darwin-arm64 \
  dist/tealstreet-darwin-x64 \
  dist/tealstreet-linux-x64 \
  dist/tealstreet-linux-arm64

# Update Homebrew formula with new SHA256 values
echo ""
echo "Updating Homebrew formula..."
FORMULA_FILE="homebrew/tealstreet.rb"
if [ -f "$FORMULA_FILE" ]; then
  # Update version
  sed -i '' "s/version \".*\"/version \"${VERSION}\"/" "$FORMULA_FILE"

  # Update SHA256 values by matching the URL context on the previous line
  # darwin-arm64: find line with that URL, go to next line, replace sha256
  sed -i '' "/darwin-arm64/{n;s/sha256 \"[a-f0-9]*\"/sha256 \"${SHA_DARWIN_ARM64}\"/;}" "$FORMULA_FILE"
  # darwin-x64
  sed -i '' "/darwin-x64/{n;s/sha256 \"[a-f0-9]*\"/sha256 \"${SHA_DARWIN_X64}\"/;}" "$FORMULA_FILE"
  # linux-arm64
  sed -i '' "/linux-arm64/{n;s/sha256 \"[a-f0-9]*\"/sha256 \"${SHA_LINUX_ARM64}\"/;}" "$FORMULA_FILE"
  # linux-x64
  sed -i '' "/linux-x64/{n;s/sha256 \"[a-f0-9]*\"/sha256 \"${SHA_LINUX_X64}\"/;}" "$FORMULA_FILE"

  echo "  Updated: $FORMULA_FILE"
fi

# Push to homebrew-tap repo
echo ""
echo "Pushing to homebrew-tap..."
HOMEBREW_TAP_REPO="Tealstreet/homebrew-tap"
TEMP_TAP_DIR=$(mktemp -d)
trap "rm -rf $TEMP_TAP_DIR" EXIT

if gh repo clone "$HOMEBREW_TAP_REPO" "$TEMP_TAP_DIR" 2>/dev/null; then
  cp "$FORMULA_FILE" "$TEMP_TAP_DIR/Formula/tealstreet.rb"
  cd "$TEMP_TAP_DIR"
  git remote set-url origin "git@github.com:${HOMEBREW_TAP_REPO}.git"
  git add -A
  if git diff --staged --quiet; then
    echo "  No changes to homebrew-tap"
  else
    git commit -m "Update tealstreet to v${VERSION}"
    git push
    echo "  Pushed to homebrew-tap"
  fi
  cd - > /dev/null
else
  echo "  Warning: Could not clone homebrew-tap repo"
  echo "  Manually copy: cp $FORMULA_FILE to homebrew-tap/Formula/tealstreet.rb"
fi

echo ""
echo "=== Release complete! ==="
echo ""
echo "View release: https://github.com/${PUBLIC_REPO}/releases/tag/${TAG}"
echo ""
echo "Users can now install with:"
echo ""
echo "  # Quick install"
echo "  curl -fsSL https://raw.githubusercontent.com/${PUBLIC_REPO}/main/install.sh | bash"
echo ""
echo "  # Or with Homebrew (once tap is set up)"
echo "  brew tap tealstreet/tap && brew install tealstreet"
echo ""
