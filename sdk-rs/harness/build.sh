#!/usr/bin/env bash
# Build the release wasm engine and its Node bindings for the harness.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build --locked --release -p safe-cex-wasm --target wasm32-unknown-unknown
wasm-bindgen --target nodejs --out-dir safe-cex-wasm/pkg/node \
  target/wasm32-unknown-unknown/release/safe_cex_wasm.wasm
