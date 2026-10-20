#!/usr/bin/env bash
# Build the release wasm engine with bindings for browsers and Workers (web)
# and for Node (the harness and tests). --venue selects one venue for size gates.
set -euo pipefail
build_args=(--locked --release -p safe-cex-wasm --target wasm32-unknown-unknown)
if [[ $# -gt 0 ]]; then
  if [[ $# -ne 2 || $1 != --venue ]]; then
    echo "Usage: $0 [--venue coinbase|coinbasespot|breakoutprop|polymarket|grvt|binance|binancespot|binanceinverse|binancepm|bitunix|kucoin|kucoinspot|asterdex|kraken|krakenspot|apexomni|paradex|phemex|bybit|hyperliquid|hyperliquidspot|okx|cryptocom|weex|bingx|mexc|blofin|backpack|bitget|bitrue|woo|lighter|lighter-robinhood|txflow|orderly]" >&2
    exit 2
  fi
  case "$2" in
    breakoutprop|polymarket|grvt|binance|bitunix|kucoin|kucoinspot|asterdex|kraken|apexomni|paradex|phemex|bybit|hyperliquid|okx|cryptocom|weex|bingx|mexc|blofin|backpack|bitget|bitrue|woo|lighter|txflow) build_args+=(--no-default-features --features "$2") ;;
    binancespot|binanceinverse|binancepm) build_args+=(--no-default-features --features binance) ;;
    hyperliquidspot) build_args+=(--no-default-features --features hyperliquid) ;;
    krakenspot) build_args+=(--no-default-features --features kraken) ;;
    lighter-robinhood) build_args+=(--no-default-features --features lighter) ;;
    coinbase|coinbasespot) build_args+=(--no-default-features --features coinbase) ;;
    orderly|tealstreet-dex|kodiakfi|mode|salsadex|what|woofi) build_args+=(--no-default-features --features orderly) ;;
    *) echo "Unsupported venue: $2 (expected coinbase, coinbasespot, breakoutprop, polymarket, grvt, binance, binancespot, binanceinverse, binancepm, bitunix, kucoin, kucoinspot, asterdex, kraken, apexomni, paradex, phemex, bybit, hyperliquid, okx, cryptocom, weex, bingx, mexc, blofin, backpack, bitget, bitrue, woo, lighter, lighter-robinhood, txflow or orderly)" >&2; exit 2 ;;
  esac
  if [[ $2 == paradex ]]; then
    # Keep standalone Paradex within the fixed size gate; public replay also gates CPU and memory.
    build_args+=(--config 'profile.release.package.safe-cex-wasm.opt-level="z"'
      --config 'profile.release.package.safe-cex-engine.opt-level=2')
  elif [[ $2 == bitrue ]]; then
    # Spend standalone size headroom on decoder throughput; combined builds keep the size profile.
    build_args+=(--config 'profile.release.package.safe-cex-venue-bitrue.opt-level=3')
  elif [[ $2 == okx ]]; then
    # EU's steady public decoder needs throughput within the same standalone size budget.
    build_args+=(--config 'profile.release.package.safe-cex-venue-okx.opt-level=3')
  fi
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
