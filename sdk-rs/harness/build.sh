#!/usr/bin/env bash
# Build the release wasm engine with bindings for browsers and Workers (web)
# and for Node (the harness and tests). --venue builds one venue's family in
# place of the combined engine, for gates; --families builds every family into
# pkg/{web,node}/<family>, which browser and Worker loaders fetch per venue.
set -euo pipefail
venues="breakoutprop, asterdex, binance, bitunix, kucoin, kucoinspot, bybit, hyperliquid, hyperliquidspot, txflow, grvt, okx, cryptocom, weex, bingx, mexc, blofin, backpack, bitget, bitgetusdc, bitrue, phemex, woo, lighter, lighter-robinhood, apexomni, kraken, krakenspot, paradex, coinbase, orderly or polymarket"
mutation_barriers=false
if [[ $# -eq 3 && $1 == --venue && $3 == --mutation-barriers ]]; then
  mutation_barriers=true
  set -- "$1" "$2"
fi
if [[ $# -gt 0 && ! ( $# -eq 2 && $1 == --venue ) && ! ( $# -eq 1 && $1 == --families ) ]]; then
  echo "Usage: $0 [--venue VENUE [--mutation-barriers] | --families]" >&2
  exit 2
fi
# WASM-only size profiles. Native release profiles in Cargo.toml stay unchanged.
# Keep the venue throughput exceptions; derivatives KuCoin and Paradex retain
# their existing standalone overrides below. Explicit package profiles still win.
wasm_profile_args=(--config 'profile.release.opt-level="z"'
  --config 'profile.release.package.safe-cex-wasm.opt-level="z"'
  --config 'profile.release.package.safe-cex-engine.opt-level="z"'
  --config 'profile.release.package.safe-cex-venue-kucoin.opt-level=3')
# One JSON throughput policy for every WASM family and the combined engine,
# except standalone Bybit, whose size budget wins; CI gates its public decode CPU.
# Native Cargo profiles are unchanged; CI gates family CPU and size budgets.
wasm_throughput_args=(--config 'profile.release.package.serde_json.opt-level=3')
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
  # Every family links the Rust algos (chaser, TWAP, exec) for the LOCAL worker.
  build_args=(--locked --release -p safe-cex-wasm --target wasm32-unknown-unknown --no-default-features --features "$family,algos-exec")
  if [[ $mutation_barriers == true ]]; then
    build_args+=(--features http-mutation-barriers)
  fi
  # Use size defaults unless measured CPU margin requires speed defaults.
  # KuCoin Spot and WOO retain speed defaults after measured margin misses. Aster
  # takes size defaults since dispatch signing, with its venue crate at "s" below.
  if [[ $family != kucoinspot && $family != woo ]]; then
    build_args+=("${wasm_profile_args[@]}")
  fi
  if [[ $family == asterdex ]]; then
    # Venue crate at "s": the family's size budget, within its CPU gate.
    build_args+=(--config 'profile.release.package.safe-cex-venue-asterdex.opt-level="s"')
  elif [[ $family == woo ]]; then
    # Keep WOO ticker decoding throughput within the fixed CPU and size gates.
    build_args+=(--config 'profile.release.package.safe-cex-venue-woo.opt-level=3')
  elif [[ $family == bitget ]]; then
    # Engine at s: steady CPU clears the 1.10x gate with margin; z measured 1.07-1.21x.
    build_args+=(--config 'profile.release.package.safe-cex-engine.opt-level="s"')
  elif [[ $family == kucoin ]]; then
    # Derivatives pass z; Spot and combined builds keep the crate's throughput profile.
    build_args+=(--config 'profile.release.package.safe-cex-venue-kucoin.opt-level="z"')
  fi
  if [[ $family != bybit ]]; then
    build_args+=("${wasm_throughput_args[@]}")
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
# The broad shared size profile misses combined Bitget/KuCoin Spot CPU gates.
# Measure a narrower host-only profile without changing their engine/dependencies.
build_args=(--locked --release -p safe-cex-wasm --target wasm32-unknown-unknown
  --config 'profile.release.package.safe-cex-wasm.opt-level="z"'
  "${wasm_throughput_args[@]}")
if [[ ${1:-} == --venue ]]; then family_args "$2"; fi
cargo build "${build_args[@]}"
bind hosts/wasm/pkg/web hosts/wasm/pkg/node
