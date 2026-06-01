#!/usr/bin/env bash
set -euo pipefail

DIST_DIR="${1:-dist}"

if [ ! -d "$DIST_DIR" ]; then
  echo "Release artifact directory does not exist: $DIST_DIR" >&2
  exit 1
fi

map_files="$(find "$DIST_DIR" -type f -name '*.map' -print)"
if [ -n "$map_files" ]; then
  echo "Release artifacts contain source maps:" >&2
  echo "$map_files" >&2
  exit 1
fi

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
SCANNER="$SCRIPT_DIR/check-release-artifact.cjs"

failed=0
while IFS= read -r -d '' artifact; do
  if [ ! -f "$artifact" ]; then
    continue
  fi

  if ! node "$SCANNER" "$artifact"; then
    failed=1
  fi
done < <(find "$DIST_DIR" -type f -print0)

if [ "$failed" -ne 0 ]; then
  exit 1
fi
