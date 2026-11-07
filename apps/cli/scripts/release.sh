#!/bin/bash
set -euo pipefail

# Tag-driven release entrypoint.
#
# The GitHub Action publishes the public Tealstreet/cli release and updates
# Tealstreet/homebrew-tap after this script pushes a cli-vX.Y.Z tag.
#
# BRAND=disrupt pushes disrupt-cli-vX.Y.Z instead, which the same workflow
# builds with `scripts/build.sh disrupt` and publishes to DisruptTrade/cli
# only (no npm, no Homebrew). Both brands share package.json's version.

cd "$(dirname "$0")/.."

BRAND="${BRAND:-tealstreet}"
case "$BRAND" in
  tealstreet) TAG_PREFIX="cli-v" ;;
  disrupt) TAG_PREFIX="disrupt-cli-v" ;;
  *)
    echo "Unknown BRAND: $BRAND (expected tealstreet|disrupt)"
    exit 1
    ;;
esac

BUMP="${1:-}"
case "$BUMP" in
  "" | patch | minor | major) ;;
  *)
    echo "Usage: yarn release[:patch|:minor|:major]"
    exit 1
    ;;
esac

if [ -n "$BUMP" ]; then
  yarn version "--${BUMP}" --no-git-tag-version
fi

VERSION=$(node -p "require('./package.json').version")
TAG="${TAG_PREFIX}${VERSION}"
ROOT_DIR=$(git rev-parse --show-toplevel)

cd "$ROOT_DIR"

if [ -n "$(git status --porcelain --untracked-files=no | grep -v '^ M apps/cli/package.json$' || true)" ]; then
  echo "Refusing to release with tracked changes outside apps/cli/package.json:"
  git status --short --untracked-files=no
  exit 1
fi

if git rev-parse "$TAG" >/dev/null 2>&1; then
  echo "Tag already exists locally: $TAG"
  exit 1
fi

if git ls-remote --exit-code --tags origin "refs/tags/${TAG}" >/dev/null 2>&1; then
  echo "Tag already exists on origin: $TAG"
  exit 1
fi

if ! git diff --quiet -- apps/cli/package.json; then
  git add apps/cli/package.json
  git commit -m "chore(cli): release v${VERSION}"
fi

git tag "$TAG"
git push origin HEAD
git push origin "$TAG"

echo "Release tag pushed: $TAG"
if [ "$BRAND" = disrupt ]; then
  echo "GitHub Actions will publish DisruptTrade/cli v${VERSION} (binaries only)."
else
  echo "GitHub Actions will publish Tealstreet/cli v${VERSION} and update Homebrew."
  echo "  brew tap tealstreet/tap && brew install tealstreet"
fi
echo ""
