#!/usr/bin/env bash
# Install the pinned binaryen release (wasm-opt) on a Linux x86_64 CI runner and
# put it on the GitHub Actions PATH. build.sh optimizes every bound wasm with it.
set -euo pipefail
version=132
sha256=195ddc94f9bc89f45abdabb0b9eea86023d727ba90eac8b35b80f2544fc30572
dir="${RUNNER_TOOL_CACHE:-$HOME/.cache}/binaryen-version_$version"
cache="${RUNNER_TOOL_CACHE:-$HOME/.cache}/binaryen-archives"
archive="$cache/version_$version-$sha256.tar.gz"
mkdir -p "$cache"
staging=$(mktemp -d "${dir}.XXXXXX")
trap 'rm -rf "$staging"' EXIT
verify() { printf '%s  %s\n' "$sha256" "$1" | sha256sum -c -; }
if [[ -f "$archive" ]] && ! verify "$archive"; then
  printf 'Discarding corrupt Binaryen archive: %s\n' "$archive" >&2
  rm -f "$archive"
fi
if [[ ! -f "$archive" ]]; then
  curl --fail --silent --show-error --location \
    --retry 3 --retry-all-errors --retry-delay 2 --retry-max-time 180 \
    --connect-timeout 15 --max-time 60 -o "$staging/download.tar.gz" \
    "https://github.com/WebAssembly/binaryen/releases/download/version_$version/binaryen-version_$version-x86_64-linux.tar.gz"
  verify "$staging/download.tar.gz"
  mv "$staging/download.tar.gz" "$archive"
fi
# Only the checksum-verified archive is cached; never trust a restored executable.
tar -xzf "$archive" -C "$staging" --strip-components=1
"$staging/bin/wasm-opt" --version | grep -E "^wasm-opt version $version([[:space:]]|$)"
rm -rf "$dir"
mv "$staging" "$dir"
"$dir/bin/wasm-opt" --version
echo "$dir/bin" >> "${GITHUB_PATH:?run in GitHub Actions}"
