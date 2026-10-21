#!/usr/bin/env bash
# Build the release wasm engine with bindings for browsers and Workers (web)
# and for Node (the harness and tests). --venue builds one venue's family in
# place of the combined engine, for gates; --families builds every family into
# pkg/{web,node}/<family>, which browser and Worker loaders fetch per venue.
set -euo pipefail
venues="breakoutprop, asterdex, binance, bitunix, kucoin, kucoinspot, bybit, hyperliquid, hyperliquidspot, txflow, grvt, okx, cryptocom, weex, bingx, mexc, blofin, backpack, bitget, bitgetusdc, bitrue, phemex, woo, lighter, lighter-robinhood, apexomni, kraken, krakenspot, paradex, coinbase, orderly or polymarket"
if [[ $# -gt 0 && ! ( $# -eq 2 && $1 == --venue ) && ! ( $# -eq 1 && $1 == --families ) ]]; then
  echo "Usage: $0 [--venue VENUE | --families]" >&2
  exit 2
fi
# The Cargo arguments building the family that contains a venue.
family_args() {
  case "$1" in
    breakoutprop|asterdex|binance|bitunix|kucoin|kucoinspot|bybit|hyperliquid|txflow|grvt|okx|cryptocom|weex|bingx|mexc|blofin|backpack|bitget|bitrue|phemex|woo|lighter|apexomni|kraken|paradex|polymarket|coinbase|orderly) family=$1 ;;
    bitgetusdc) family=bitget ;;
    lighter-robinhood) family=lighter ;;
    hyperliquidspot) family=hyperliquid ;;
    krakenspot) family=kraken ;;
    coinbasespot) family=coinbase ;;
    tealstreet-dex|kodiakfi|mode|salsadex|what|woofi) family=orderly ;;
    *) echo "Unsupported venue: $1 (expected $venues)" >&2; exit 2 ;;
  esac
  build_args=(--locked --release -p safe-cex-wasm --target wasm32-unknown-unknown --no-default-features --features "$family")
  if [[ $family == paradex ]]; then
    # Keep standalone Paradex within the fixed size gate; public replay also gates CPU and memory.
    build_args+=(--config 'profile.release.package.safe-cex-wasm.opt-level="z"'
      --config 'profile.release.package.safe-cex-engine.opt-level=2')
  fi
}
# Optimize a bound module with binaryen (behaviour-neutral; shrinks every
# family). CI installs the pinned release (install-binaryen.sh) and sets
# SAFE_CEX_WASM_OPT=required; a local build without wasm-opt only warns.
# WASM_OPT_LEVEL defaults to -Oz.
optimize() {
  if command -v wasm-opt >/dev/null; then
    wasm-opt "${WASM_OPT_LEVEL:--Oz}" --enable-bulk-memory --enable-bulk-memory-opt \
      --enable-mutable-globals --enable-sign-ext --enable-nontrapping-float-to-int \
      --enable-reference-types --enable-multivalue "$1" -o "$1"
  elif [[ ${SAFE_CEX_WASM_OPT:-} == required ]]; then
    echo "wasm-opt is required (sdk-rs/harness/install-binaryen.sh)" >&2
    exit 1
  else
    echo "warning: wasm-opt not found; $1 is unoptimized" >&2
  fi
}
# Bind the last build for browsers/Workers and Node; Node bindings are
# CommonJS inside an ESM package.
bind() {
  wasm-bindgen --remove-name-section --remove-producers-section --target web --out-dir "$1" \
    target/wasm32-unknown-unknown/release/safe_cex_wasm.wasm
  wasm-bindgen --remove-name-section --remove-producers-section --target nodejs --out-dir "$2" \
    target/wasm32-unknown-unknown/release/safe_cex_wasm.wasm
  optimize "$1/safe_cex_wasm_bg.wasm"
  optimize "$2/safe_cex_wasm_bg.wasm"
  echo '{"type":"commonjs"}' > "$2/package.json"
}
cd "$(dirname "$0")/.."
# macOS strips DYLD_* from inherited environments; rust-lld needs the toolchain's libLLVM.
if [[ $(uname) == Darwin ]]; then
  export DYLD_LIBRARY_PATH="$(rustc --print sysroot)/lib"
fi
if [[ ${1:-} == --families ]]; then
  # Assigned first so a failing node fails the build instead of looping over nothing.
  families=$(node --input-type=module -e "import('./hosts/wasm/js/venues.mjs').then(({ venueFamilies }) => console.log(venueFamilies.join(' ')))")
  for venue in $families; do
    family_args "$venue"
    cargo build "${build_args[@]}"
    bind "hosts/wasm/pkg/web/$family" "hosts/wasm/pkg/node/$family"
  done
  exit 0
fi
build_args=(--locked --release -p safe-cex-wasm --target wasm32-unknown-unknown)
if [[ ${1:-} == --venue ]]; then family_args "$2"; fi
cargo build "${build_args[@]}"
bind hosts/wasm/pkg/web hosts/wasm/pkg/node
