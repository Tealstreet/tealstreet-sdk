#!/usr/bin/env bash
# Build the release wasm engine with bindings for browsers and Workers (web)
# and for Node (the harness and tests). --venue selects one venue for size gates.
set -euo pipefail
build_args=(--locked --release -p safe-cex-wasm --target wasm32-unknown-unknown)
if [[ $# -gt 0 ]]; then
  if [[ $# -ne 2 || $1 != --venue ]]; then
    echo "Usage: $0 [--venue asterdex|binance|kucoin|bybit|hyperliquid|okx|cryptocom|weex|bingx|mexc|blofin|backpack|bitget|bitrue|woo|lighter|apexomni|kraken|orderly|coinbase]" >&2
    exit 2
  fi
  case "$2" in
    asterdex|binance|kucoin|bybit|hyperliquid|okx|cryptocom|weex|bingx|mexc|blofin|backpack|bitget|bitrue|woo|lighter|apexomni|kraken) build_args+=(--no-default-features --features "$2") ;;
    coinbase|coinbasespot) build_args+=(--no-default-features --features coinbase) ;;
    orderly|tealstreet-dex|kodiakfi|mode|salsadex|what|woofi) build_args+=(--no-default-features --features orderly) ;;
    *) echo "Unsupported venue: $2 (expected asterdex, binance, kucoin, bybit, hyperliquid, okx, cryptocom, weex, bingx, mexc, blofin, backpack, bitget, bitrue, woo, lighter, apexomni, kraken, coinbase or orderly)" >&2; exit 2 ;;
  esac
fi
cd "$(dirname "$0")/.."
# macOS strips DYLD_* from inherited environments; rust-lld needs the toolchain's libLLVM.
if [[ $(uname) == Darwin ]]; then
  export DYLD_LIBRARY_PATH="$(rustc --print sysroot)/lib"
fi
cargo build "${build_args[@]}"
for target in web nodejs; do
  wasm-bindgen --remove-name-section --remove-producers-section --target "$target" --out-dir "hosts/wasm/pkg/${target/nodejs/node}" \
    target/wasm32-unknown-unknown/release/safe_cex_wasm.wasm
done
# The Node bindings are CommonJS inside an ESM package.
echo '{"type":"commonjs"}' > hosts/wasm/pkg/node/package.json
