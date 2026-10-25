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

crates=$(ls -d engine/ crypto/ hosts/*/ registry/ venues/*/)

# JavaScript value, number, string and runtime emulation.
ban 'JS-shaped value types' '\bJs[A-Z][A-Za-z]*\b|\bjs_runtime\b|\bJsValue\b' $crates
ban 'UTF-16 string emulation' 'encode_utf16|from_utf16|[Uu]tf16' $crates
ban 'JS number formatting emulation' 'ryu[-_]js|js_number_string|to_precision|ECMAScript' $crates
ban 'emulated promises, microtasks or timers' '\bmicrotask|\bMicrotask|\bPromise[A-Z]?\b|setTimeout|setInterval' $crates
ban 'Axios or BaseExchange facades' '\bAxios|\bBaseExchange\b|\bbase_exchange\b' $crates
ban 'JSON map-order or arbitrary-precision emulation' 'preserve_order|arbitrary_precision|IndexMap' $crates
ban 'deleted port crates' 'safe-cex-(exchange|registry|parity|ffi|webworker|types|num|store|transport|candle-cache|lighter-sdk)\b|safe_cex_(exchange|registry|parity|ffi|webworker|types|num|store|transport|candle_cache|lighter_sdk)\b' $crates

# Untyped JSON trees stay out of engine and venue hot paths.
hot=$(ls -d engine/src registry/src venues/*/src 2>/dev/null || true)
if [[ -n $hot ]]; then
  ban 'serde_json::Value in engine or venue source' 'serde_json::Value|\bValue::(Object|Array)' $hot
fi

# Production entropy is supplied only by hosts; formatters and venue signers are pure.
random_sources=$(ls -d engine/src crypto/src venues/*/src)
ban 'ambient randomness outside host adapters' 'getrandom::|rand::|uuid::(uuid\(|Uuid)|new_v4\(|\b(OsRng|ThreadRng|thread_rng)\b' $random_sources

# Bridge behavior comes from capabilities/construction rules, never venue lists.
if ! python3 - <<'PY'
import pathlib, re
path = pathlib.Path('../packages/safe-cex/src/exchanges/rust/rust.exchange.ts')
source = path.read_text()
patterns = [
    r"\b(?:this\.name|exchangeName)\s*(?:===|!==)\s*['\"]",
    r"\.includes\(\s*(?:this\.name|exchangeName)\s*,?\s*\)",
    r"\.has\(\s*this\.name\s*\)",
]
hits = [match for pattern in patterns for match in re.finditer(pattern, source)]
for match in hits:
    print(f'BANNED: venue-name bridge policy: {path}:{source.count(chr(10), 0, match.start()) + 1}')
raise SystemExit(bool(hits))
PY
then
  status=1
fi

# Credential-only setup (docs/plans/RUST_CREDENTIAL_ONLY_SETUP.md, D1/D2): no
# consumer may rebuild an empty signing ref or reach the absent/optional forms.
if ! python3 - <<'PY'
import pathlib, re
rules = [
    ('empty KeyRef outside setup validation metadata',
     r'KeyRef\((?:String::new\(\)|""\.(?:into|to_string|to_owned)\(\)|String::from\(""\))\)|KeyRef::from\(""\)',
     {'crypto/src/signer.rs'}),
    ('Identity::signing_opt or set_signing outside setup and construction',
     r'\.(?:signing_opt|set_signing)\(',
     {'crypto/src/signer.rs', 'engine/src/signing.rs', 'registry/src/construction.rs', 'registry/src/lib.rs'}),
    ('validate_credential outside the declared signing policy',
     r'\bvalidate_credential\(',
     {'engine/src/signing.rs'}),
]
hits = []
for path in sorted(pathlib.Path('.').glob('**/src/**/*.rs')):
    if 'target' in path.parts or path.name.endswith('_tests.rs'):
        continue
    text = path.read_text()
    test_module = re.search(r'^#\[cfg\(test\)\]\s*\n\s*mod \w+', text, re.M)
    source = text[:test_module.start()] if test_module else text
    for description, pattern, allowed in rules:
        if path.as_posix() in allowed:
            continue
        for match in re.finditer(pattern, source):
            hits.append(f'BANNED: {description}: {path}:{source.count(chr(10), 0, match.start()) + 1}')
print('\n'.join(hits)) if hits else None
raise SystemExit(bool(hits))
PY
then
  status=1
fi

if ! python3 scripts/check-signing.test.py || ! python3 scripts/check-signing.py; then
  status=1
fi

if ! python3 scripts/check-request-context.test.py || ! python3 scripts/check-request-context.py; then
  status=1
fi

if ! python3 scripts/check-socket-lifecycle.test.py || ! python3 scripts/check-socket-lifecycle.py; then
  status=1
fi

if ! python3 scripts/check-credential-selection.test.py || ! python3 scripts/check-credential-selection.py; then
  status=1
fi

exit $status
