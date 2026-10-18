#!/usr/bin/env python3
"""Run canonical scoped measurement, not a private probe/filter implementation.

Each temporary fault must compile and produce its named failing runtime receipt.
Restore exact production bytes and require all four native context owners to pass afterwards.
"""
import json
import os
from pathlib import Path
import subprocess
import sys

if not __debug__:
    raise SystemExit("Binance receipt controls require Python assertions; do not use -O")

ROOT = Path(__file__).resolve().parents[2]
TARGET = ROOT / "target/accepted-inventory"
OWNERS = ["exchange_classes::" + name for name in ["ExchangeOwner", "Context", "RestContext", "emit_json"]]
COMMAND = ["node", "safe-cex-parity/scripts/accepted-set.mjs",
           "--package=safe-cex-venue-binance", "--features=native", "--jobs=1", "--measure"]
COMMAND += ["--owner=" + name for name in OWNERS]
FAULTS = [
    (OWNERS[0], "impl ExchangeOwner {", "\npub struct ExchangeState", "ExchangeState::construct(venue, name, options, store, d)", "ExchangeState::construct(if venue == Venue::PM { Venue::Futures } else { venue }, name, options, store, d)"),
    (OWNERS[1], "impl crate::private_rest::SnapshotBindings for Context {", "    fn capture(", "HttpMethod::Get", "HttpMethod::Post"),
    (OWNERS[2], "impl crate::public_rest::RestBindings for RestContext {", "    fn is_public_data_only(", "string(p.symbol)", 'string(format!("wrong{}",p.symbol))'),
    (OWNERS[3], "fn emit_json(", "\nimpl ExchangeState {", "crate::private_session::native(value.clone())", 'crate::private_session::native(serde_json::Value::String(safe_cex_num::js_string(Some(value)).expect("JSON profile coercion")))'),
]


def measure(label):
    log = Path("/tmp/port-txflow-native-context-" + label + ".log")
    results_file = TARGET / "accepted-set-results.json"
    results_file.unlink(missing_ok=True)
    # Keep rustdoc and its subsequent compiler/probe checks in one isolated
    # bootstrap environment; toggling it invalidates proc-macro dependency
    # fingerprints on every package. Normal test builds use another target.
    env = dict(os.environ, RUSTC_BOOTSTRAP="1", CARGO_BUILD_JOBS="1",
               CARGO_TARGET_DIR=str(TARGET))
    with log.open("w") as output:
        status = subprocess.run(COMMAND, cwd=ROOT, env=env,
                                stdout=output, stderr=subprocess.STDOUT)
    assert status.returncode == 0, (label, log.read_text()[-6000:])
    results = json.loads(results_file.read_text())
    selected = [row for row in results if row["name"] in
                ["safe_cex_venue_binance::" + owner for owner in OWNERS]]
    assert len(selected) == 4, (label, results)
    assert all(not row.get("classificationOnly") and "native" in row["compilerFeatures"] and row["configuration"]["target"] == "native" for row in selected), (label, selected)
    log.with_suffix(".json").write_text(json.dumps(selected, indent=2) + "\n")
    return selected


assert len(sys.argv) <= 2 and (len(sys.argv) == 1 or sys.argv[1].startswith("--only="))
chosen = OWNERS if len(sys.argv) == 1 else ["exchange_classes::" + name for name in sys.argv[1].removeprefix("--only=").split(",")]
assert len(chosen) == len(set(chosen)) and all(name in OWNERS for name in chosen), chosen
baseline = measure("baseline-green")
assert all(not row["failures"] for row in baseline), baseline
receipts = []
for owner, scope_start, scope_end, before, after in FAULTS:
    if owner not in chosen:
        continue
    path = ROOT / "safe-cex-venue-binance/src/exchange_classes.rs"
    original = path.read_bytes()
    text = original.decode()
    assert text.count(scope_start) == 1, owner
    start = text.index(scope_start)
    end = text.index(scope_end, start + len(scope_start))
    body = text[start:end]
    assert body.count(before) == 1, owner
    try:
        path.write_text(text[:start] + body.replace(before, after) + text[end:])
        rows = measure(owner.split("::")[-1] + "-red")
        fault = next(row for row in rows if row["name"] == "safe_cex_venue_binance::" + owner)
        assert fault["failures"], (owner, rows)
        assert all(not row["failures"] for row in rows if row is not fault), rows
    finally:
        path.write_bytes(original)
    assert path.read_bytes() == original
    restored = measure(owner.split("::")[-1] + "-restored")
    assert all(not row["failures"] for row in restored), restored
    receipts.append({"owner": owner, "compiledNamedRed": True, "exactRestoredGreen": True})
    print(owner + " compiled RED -> exact restored GREEN", flush=True)
Path("/tmp/port-txflow-native-context-controls.json").write_text(json.dumps(receipts, indent=2) + "\n")
