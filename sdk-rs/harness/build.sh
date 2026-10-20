#!/usr/bin/env bash
# Build one browser/Workers and Node package per venue family; --venue selects one.
set -euo pipefail
cd "$(dirname "$0")/.."
# macOS strips DYLD_* from inherited environments; rust-lld needs the toolchain's libLLVM.
if [[ $(uname) == Darwin ]]; then
  export DYLD_LIBRARY_PATH="$(rustc --print sysroot)/lib"
fi
# Feature names are the build manifest: registering another venue adds its build.
families=()
while IFS= read -r family; do families+=("$family"); done < <(sed -n 's/^venue-\([a-z0-9-]*\) = .*/\1/p' hosts/wasm/Cargo.toml)
if [[ $# -gt 0 ]]; then
  if [[ $# -ne 2 || $1 != --venue ]]; then
    echo "Usage: $0 [--venue FAMILY]" >&2
    exit 2
  fi
  case "$2" in
    lighter-robinhood) set -- --venue lighter ;;
    hyperliquidspot) set -- --venue hyperliquid ;;
    krakenspot) set -- --venue kraken ;;
    coinbasespot) set -- --venue coinbase ;;
    tealstreet-dex|kodiakfi|mode|salsadex|what|woofi) set -- --venue orderly ;;
  esac
  selected=""
  for family in "${families[@]}"; do
    if [[ "$family" == "$2" ]]; then selected="$family"; break; fi
  done
  if [[ -z "$selected" ]]; then
    echo "Unsupported venue family: $2" >&2
    exit 2
  fi
  families=("$selected")
fi
for family in "${families[@]}"; do
  build_args=(--locked --release -p safe-cex-wasm --target wasm32-unknown-unknown
    --no-default-features --features "venue-$family")
  if [[ $family == paradex ]]; then
    # Standalone signing builds trade host/engine optimization for the venue size gate.
    build_args+=(--config 'profile.release.package.safe-cex-wasm.opt-level=2'
      --config 'profile.release.package.safe-cex-engine.opt-level=2')
  elif [[ $family == bitrue ]]; then
    # Keep the landed standalone decoder throughput profile; aggregate defaults differ.
    build_args+=(--config 'profile.release.package.safe-cex-venue-bitrue.opt-level=3')
  fi
  cargo build "${build_args[@]}"
  for target in web nodejs; do
    out="hosts/wasm/pkg/${target/nodejs/node}/$family"
    wasm-bindgen --remove-name-section --remove-producers-section --target "$target" --out-dir "$out" \
      target/wasm32-unknown-unknown/release/safe_cex_wasm.wasm
    if [[ "$target" == nodejs ]]; then
      echo '{"type":"commonjs"}' > "$out/package.json"
    fi
  done
done
