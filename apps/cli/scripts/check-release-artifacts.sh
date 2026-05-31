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
