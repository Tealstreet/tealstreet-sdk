#!/usr/bin/env bash
# Vercel Ignored Build Step for docs project
# Exit 1 => continue with build/deploy
# Exit 0 => skip this deployment
#
# Recommended Vercel setting:
#   Ignored Build Step: bash ./scripts/vercel-ignore.sh
#
# Policy defaults:
# - Allow Preview builds only when apps/documentation/** actually changed
#   (the CLI section lives under apps/documentation/docs/cli/ — no longer
#   pulled in from a sibling path)
# - Block Production builds unless explicitly allowed
#
# Optional environment overrides:
# - VERCEL_DOCS_FORCE_BUILD=1  => always build
# - VERCEL_DOCS_ALLOW_PROD=1   => allow production builds
# - VERCEL_DOCS_FREEZE=1       => force skip all builds
# - VERCEL_DOCS_PREVIEW_BRANCH  => preview branch to allow (default: master)

set -euo pipefail

echo "=== Docs ignore-build check ==="
echo "VERCEL_ENV=${VERCEL_ENV:-unset}"
echo "VERCEL_GIT_COMMIT_REF=${VERCEL_GIT_COMMIT_REF:-unset}"
echo "VERCEL_GIT_COMMIT_SHA=${VERCEL_GIT_COMMIT_SHA:-unset}"
echo "VERCEL_GIT_PREVIOUS_SHA=${VERCEL_GIT_PREVIOUS_SHA:-unset}"
echo "VERCEL_GIT_PULL_REQUEST_ID=${VERCEL_GIT_PULL_REQUEST_ID:-unset}"

handle_build() {
  echo "=> Proceeding with build"
  exit 1
}

handle_skip() {
  echo "=> Skipping build"
  exit 0
}

is_allowed_preview_branch() {
  local branch="${VERCEL_GIT_COMMIT_REF:-}"
  local allowed="${VERCEL_DOCS_PREVIEW_BRANCH:-master}"

  case "$branch" in
    "$allowed"|origin/"$allowed"|refs/heads/"$allowed")
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

# Ensure the previous deployment SHA is reachable in Vercel's shallow clone.
# This is best-effort: downstream diff logic has safe fallbacks if unavailable.
ensure_git_history() {
  local from_sha="${VERCEL_GIT_PREVIOUS_SHA:-}"

  if [ -z "$from_sha" ]; then
    return 0
  fi

  if git cat-file -e "$from_sha" 2>/dev/null; then
    return 0
  fi

  echo "Previous SHA not in shallow clone; deepening history..."

  # Try fetching the specific commit first (fastest)
  if git fetch --depth=1 origin "$from_sha" 2>/dev/null; then
    if git cat-file -e "$from_sha" 2>/dev/null; then
      echo "Fetched previous SHA successfully"
      return 0
    fi
  fi

  # Broaden: deepen the current branch history
  git fetch --deepen=50 2>/dev/null || true
  if git cat-file -e "$from_sha" 2>/dev/null; then
    echo "Found previous SHA after deepening by 50"
    return 0
  fi

  git fetch --deepen=150 2>/dev/null || true
  if git cat-file -e "$from_sha" 2>/dev/null; then
    echo "Found previous SHA after deepening by 150"
    return 0
  fi

  echo "WARNING: Could not fetch previous SHA $from_sha"
  return 1
}

get_changed_files() {
  local from_sha="${VERCEL_GIT_PREVIOUS_SHA:-}"
  local to_sha="${VERCEL_GIT_COMMIT_SHA:-HEAD}"

  if [ -z "$to_sha" ]; then
    echo "Unable to determine current commit SHA for diff" >&2
    return 1
  fi

  # First deployment on this project/environment may not have a previous SHA.
  if [ -z "$from_sha" ]; then
    echo "No previous deployment SHA; using current commit file list" >&2
    git show --name-only --pretty="" "$to_sha"
    return 0
  fi

  if ! git cat-file -e "$from_sha" 2>/dev/null; then
    echo "Previous deployment SHA is unavailable locally: $from_sha" >&2
    echo "Falling back to current commit file list" >&2
    git show --name-only --pretty="" "$to_sha"
    return 0
  fi

  # Avoid invalid diff ranges when previous SHA is not in current commit ancestry.
  if ! git merge-base --is-ancestor "$from_sha" "$to_sha" 2>/dev/null; then
    echo "Previous SHA is not an ancestor of current SHA; using current commit file list" >&2
    git show --name-only --pretty="" "$to_sha"
    return 0
  fi

  git diff --name-only "$from_sha" "$to_sha"
}

has_docs_relevant_changes() {
  local changed
  changed="$(get_changed_files || true)"

  if [ -z "$changed" ]; then
    echo "Unable to determine changed files; skipping to avoid wasted build"
    return 1
  fi

  echo "Changed files in range:"
  echo "$changed"
  echo "---"

  while IFS= read -r file; do
    [ -z "$file" ] && continue

    case "$file" in
      apps/documentation/*)
        echo "Docs-relevant change: $file"
        return 0
        ;;
    esac
  done <<< "$changed"

  return 1
}

# ── Override switches ──

if [ "${VERCEL_DOCS_FREEZE:-0}" = "1" ]; then
  echo "Freeze enabled via VERCEL_DOCS_FREEZE"
  handle_skip
fi

if [ "${VERCEL_DOCS_FORCE_BUILD:-0}" = "1" ]; then
  echo "Force build enabled via VERCEL_DOCS_FORCE_BUILD"
  handle_build
fi

# ── Production guard ──

if [ "${VERCEL_ENV:-}" = "production" ] && [ "${VERCEL_DOCS_ALLOW_PROD:-0}" != "1" ]; then
  echo "Production builds are disabled by default"
  handle_skip
fi

# ── Preview: build only when docs changed ──

if [ "${VERCEL_ENV:-}" = "preview" ]; then
  echo "Preview deployment detected"
  echo "Allowed preview branch: ${VERCEL_DOCS_PREVIEW_BRANCH:-master}"

  if ! is_allowed_preview_branch; then
    echo "Branch ${VERCEL_GIT_COMMIT_REF:-unset} is not allowed for docs preview deploys"
    handle_skip
  fi

  ensure_git_history || true

  if has_docs_relevant_changes; then
    handle_build
  fi

  echo "No docs-relevant changes detected in commit range"
  handle_skip
fi

# ── Development (CLI/local) ──

if [ "${VERCEL_ENV:-}" = "development" ]; then
  echo "Development deployment detected"
  handle_build
fi

echo "Unknown or unsupported VERCEL_ENV; skipping by default"
handle_skip
