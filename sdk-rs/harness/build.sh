#!/usr/bin/env bash
# Build the release wasm engine with bindings for browsers and Workers (web)
# and for Node (the harness and tests). --venue builds one venue's family in
# place of the combined engine, for gates; --families builds every family into
# pkg/{web,node}/<family>, which browser and Worker loaders fetch per venue.
set -euo pipefail
venues="asterdex, binance, bitunix, kucoin, kucoinspot, bybit, hyperliquid, hyperliquidspot, txflow, grvt, okx, cryptocom, weex, bingx, mexc, blofin, backpack, bitget, bitgetusdc, bitrue, phemex, woo, lighter, lighter-robinhood, apexomni, kraken, krakenspot, paradex, coinbase, orderly or polymarket"
mutation_barriers=false
bingx_spot_replay=false
if [[ ${!#:-} == --bingx-spot-replay ]]; then
  bingx_spot_replay=true
  set -- "${@:1:$(($# - 1))}"
  if [[ $# -gt 0 && ! ( $# -eq 2 && $1 == --venue && $2 == bingx ) ]]; then
    echo "Spot replay fixture supports only the BingX family or combined build" >&2
    exit 2
  fi
fi
if [[ $# -eq 3 && $1 == --venue && $3 == --mutation-barriers ]]; then
  mutation_barriers=true
  set -- "$1" "$2"
fi
if [[ $# -gt 0 && ! ( $# -eq 2 && $1 == --venue ) && ! ( $# -eq 1 && $1 == --families ) ]]; then
  echo "Usage: $0 [--venue VENUE [--mutation-barriers] | --families] [--bingx-spot-replay]" >&2
  exit 2
fi
# WASM-only size profiles. Native release profiles in Cargo.toml stay unchanged.
# Keep the venue throughput exceptions; derivatives KuCoin and Paradex retain
# their existing standalone overrides below. Explicit package profiles still win.
wasm_profile_args=(--config 'profile.release.opt-level="z"'
  --config 'profile.release.package.safe-cex-wasm.opt-level="z"'
  --config 'profile.release.package.safe-cex-engine.opt-level="z"'
  --config 'profile.release.package.safe-cex-venue-kucoin.opt-level=3')
# One JSON throughput policy for every WASM family and the combined engine.
# Native Cargo profiles are unchanged; CI gates family CPU and size budgets.
wasm_throughput_args=(--config 'profile.release.package.serde_json.opt-level=3')
# The Cargo arguments building the family that contains a venue.
family_args() {
  case "$1" in
    asterdex|binance|bitunix|kucoin|kucoinspot|bybit|hyperliquid|txflow|grvt|okx|cryptocom|weex|bingx|mexc|blofin|backpack|bitget|bitrue|phemex|woo|lighter|apexomni|kraken|paradex|polymarket|coinbase|orderly) family=$1 ;;
    bitgetusdc) family=bitget ;;
    lighter-robinhood) family=lighter ;;
    hyperliquidspot) family=hyperliquid ;;
    krakenspot) family=kraken ;;
    coinbasespot) family=coinbase ;;
    kodiakfi|mode|what|woofi) family=orderly ;;
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
  elif [[ $family == txflow ]]; then
    # Only the public-stream decode crate takes throughput defaults: the TxFlow
    # CPU gate measures that path; the venue crate keeps its size profile.
    build_args+=(--config 'profile.release.package.safe-cex-txflow-public-wire.opt-level=3')
  elif [[ $family == bitget ]]; then
    # Bitget's public steady workload needs full engine throughput under the
    # strict Rust-not-slower-than-TS bound, including its hot host frame path.
    # Measure the family's size independently.
    build_args+=(--config 'profile.release.package.safe-cex-engine.opt-level=3'
      --config 'profile.release.package.safe-cex-wasm.opt-level=3')
  elif [[ $family == bybit ]]; then
    # Public frame processing needs the engine's throughput profile; retain the
    # family's separate measured size budget and strict public CPU gate.
    build_args+=(--config 'profile.release.package.safe-cex-engine.opt-level="s"')
  elif [[ $family == kucoin ]]; then
    # Derivatives pass z; Spot and combined builds keep the crate's throughput profile.
    build_args+=(--config 'profile.release.package.safe-cex-venue-kucoin.opt-level="z"')
  fi
  build_args+=("${wasm_throughput_args[@]}")
}
# Optimize a bound module with binaryen (behaviour-neutral; shrinks every
# family). CI installs the pinned release (install-binaryen.sh) and sets
# SAFE_CEX_WASM_OPT=required; a local build without wasm-opt only warns.
# WASM_OPT_LEVEL defaults to -Oz, run twice: the second identical pass saved
# 7,367 B on the combined CI artifact, exports and imports unchanged.
optimize() {
  if command -v wasm-opt >/dev/null; then
    for _ in 1 2; do
      wasm-opt "${WASM_OPT_LEVEL:--Oz}" --enable-bulk-memory --enable-bulk-memory-opt \
        --enable-mutable-globals --enable-sign-ext --enable-nontrapping-float-to-int \
        --enable-reference-types --enable-multivalue "$1" -o "$1"
    done
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
    "$target_dir/wasm32-unknown-unknown/release/safe_cex_wasm.wasm"
  wasm-bindgen --remove-name-section --remove-producers-section --target nodejs --out-dir "$2" \
    "$target_dir/wasm32-unknown-unknown/release/safe_cex_wasm.wasm"
  optimize "$1/safe_cex_wasm_bg.wasm"
  optimize "$2/safe_cex_wasm_bg.wasm"
  echo '{"type":"commonjs"}' > "$2/package.json"
}
cd "$(dirname "$0")/.."
target_dir="${CARGO_TARGET_DIR:-target}"
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
# safe-cex-wasm performs the fat-LTO link, so its "z" sets the whole module's
# post-link pipeline (rustc back/lto.rs run_pass_manager): Oz inlining and no
# unrolling for every crate. Package opt-levels only shape each crate's pre-link.
build_args=(--locked --release -p safe-cex-wasm --target wasm32-unknown-unknown
  --config 'profile.release.package.safe-cex-wasm.opt-level="z"'
  "${wasm_throughput_args[@]}")
if [[ ${1:-} == --venue ]]; then family_args "$2"; fi
if [[ $bingx_spot_replay == true ]]; then build_args+=(--features bingx-spot-replay); fi
cargo build "${build_args[@]}"
bind hosts/wasm/pkg/web hosts/wasm/pkg/node
