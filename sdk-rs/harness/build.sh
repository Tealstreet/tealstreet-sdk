#!/usr/bin/env bash
# Build the release wasm engine with bindings for browsers and Workers (web)
# and for Node (the harness and tests).
set -euo pipefail
cd "$(dirname "$0")/.."
# macOS strips DYLD_* from inherited environments; rust-lld needs the toolchain's libLLVM.
if [[ $(uname) == Darwin ]]; then
  export DYLD_LIBRARY_PATH="$(rustc --print sysroot)/lib"
fi
# Keep the aggregate browser module within budget; native release stays at level 3.
CARGO_PROFILE_RELEASE_OPT_LEVEL=s cargo build --locked --release -p safe-cex-wasm --target wasm32-unknown-unknown
for target in web nodejs; do
  wasm-bindgen --remove-name-section --remove-producers-section --target "$target" --out-dir "hosts/wasm/pkg/${target/nodejs/node}" \
    target/wasm32-unknown-unknown/release/safe_cex_wasm.wasm
done
# The Node bindings are CommonJS inside an ESM package.
echo '{"type":"commonjs"}' > hosts/wasm/pkg/node/package.json
