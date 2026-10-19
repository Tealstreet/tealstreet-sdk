#!/usr/bin/env bash
# Build the release wasm engine and its Node bindings for the harness.
set -euo pipefail
cd "$(dirname "$0")/.."
# macOS strips DYLD_* from inherited environments; rust-lld needs the toolchain's libLLVM.
if [[ $(uname) == Darwin ]]; then
  export DYLD_LIBRARY_PATH="$(rustc --print sysroot)/lib"
fi
cargo build --locked --release -p safe-cex-wasm --target wasm32-unknown-unknown
wasm-bindgen --target nodejs --out-dir safe-cex-wasm/pkg/node \
  target/wasm32-unknown-unknown/release/safe_cex_wasm.wasm
