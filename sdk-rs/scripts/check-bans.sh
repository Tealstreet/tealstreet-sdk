#!/usr/bin/env bash
# Enforces the bans in docs/plans/RUST_REWRITE.md. Run from sdk-rs/.
set -euo pipefail
cd "$(dirname "$0")/.."

status=0
# wasm-bindgen's own host types are glue, not emulation.
allowed='Js(Error|Cast)'

ban() {
  local description=$1 pattern=$2
  shift 2
  local hits
  hits=$(grep -rnE --include='*.rs' --include='Cargo.toml' "$pattern" "$@" 2>/dev/null \
    | sed -E "s/$allowed//g" | grep -E "$pattern" || true)
  if [[ -n $hits ]]; then
    echo "BANNED: $description"
    echo "$hits"
    status=1
  fi
}

crates=$(ls -d engine/ crypto/ hosts/*/ venues/*/)

# JavaScript value, number, string and runtime emulation.
ban 'JS-shaped value types' '\bJs[A-Z][A-Za-z]*\b|\bjs_runtime\b|\bJsValue\b' $crates
ban 'UTF-16 string emulation' 'encode_utf16|from_utf16|[Uu]tf16' $crates
ban 'JS number formatting emulation' 'ryu[-_]js|js_number_string|to_precision|ECMAScript' $crates
ban 'emulated promises, microtasks or timers' '\bmicrotask|\bMicrotask|\bPromise[A-Z]?\b|setTimeout|setInterval' $crates
ban 'Axios or BaseExchange facades' '\bAxios|\bBaseExchange\b|\bbase_exchange\b' $crates
ban 'JSON map-order or arbitrary-precision emulation' 'preserve_order|arbitrary_precision|IndexMap' $crates
ban 'deleted port crates' 'safe-cex-(exchange|registry|parity|ffi|webworker|types|num|store|transport|candle-cache|lighter-sdk)\b|safe_cex_(exchange|registry|parity|ffi|webworker|types|num|store|transport|candle_cache|lighter_sdk)\b' $crates

# Untyped JSON trees stay out of engine and venue hot paths.
hot=$(ls -d engine/src venues/*/src 2>/dev/null || true)
if [[ -n $hot ]]; then
  ban 'serde_json::Value in engine or venue source' 'serde_json::Value|\bValue::(Object|Array)' $hot
fi

exit $status
