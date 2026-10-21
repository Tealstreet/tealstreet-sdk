#!/usr/bin/env bash
# Install the pinned binaryen release (wasm-opt) on a Linux x86_64 CI runner and
# put it on the GitHub Actions PATH. build.sh optimizes every bound wasm with it.
set -euo pipefail
version=132
sha256=195ddc94f9bc89f45abdabb0b9eea86023d727ba90eac8b35b80f2544fc30572
dir="${RUNNER_TOOL_CACHE:-$HOME/.cache}/binaryen-version_$version"
if [[ ! -x "$dir/bin/wasm-opt" ]]; then
  archive=$(mktemp)
  curl -fsSL -o "$archive" \
    "https://github.com/WebAssembly/binaryen/releases/download/version_$version/binaryen-version_$version-x86_64-linux.tar.gz"
  echo "$sha256  $archive" | sha256sum -c -
  mkdir -p "$dir"
  tar -xzf "$archive" -C "$dir" --strip-components=1
  rm -f "$archive"
fi
"$dir/bin/wasm-opt" --version
echo "$dir/bin" >> "${GITHUB_PATH:?run in GitHub Actions}"
