#!/usr/bin/env bash
# Optional probes live apart from production packages and size/performance gates.
set -euo pipefail
cd "$(dirname "$0")/.."
if [[ $(uname) == Darwin ]]; then
  export DYLD_LIBRARY_PATH="$(rustc --print sysroot)/lib"
fi
if [[ $# == 0 ]]; then set -- okx bybit; fi
for venue in "$@"; do
  cargo build --locked --release -p safe-cex-wasm --target wasm32-unknown-unknown \
    --no-default-features --features "venue-$venue,profiling"
  out="hosts/wasm/pkg/profile/$venue"
  wasm-bindgen --remove-name-section --remove-producers-section --target nodejs --out-dir "$out" \
    target/wasm32-unknown-unknown/release/safe_cex_wasm.wasm
  echo '{"type":"commonjs"}' > "$out/package.json"
done
