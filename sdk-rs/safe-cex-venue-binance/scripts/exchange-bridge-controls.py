#!/usr/bin/env python3
"""Run canonical scoped measurement, not a private probe/filter implementation.

Each temporary fault must compile and produce its named failing runtime receipt.
Restore exact production bytes and require all three bridges to pass afterwards.
"""
import json
import os
from pathlib import Path
import re
import subprocess

ROOT = Path(__file__).resolve().parents[2]
TARGET = ROOT / "target/accepted-inventory"
OWNERS = ["exchange_classes::failure", "exchange_classes::json_value", "exchange_classes::typed"]
COMMAND = ["node", "safe-cex-parity/scripts/accepted-set.mjs",
           "--package=safe-cex-venue-binance", "--jobs=1", "--measure"]
COMMAND += ["--owner=" + name for name in OWNERS]
FAULTS = [
    (OWNERS[0], "exchange_classes.rs", "Rc::new(ExchangeError::new(message))", 'Rc::new(ExchangeError::new({ let message = message.into(); if message == "venue refusal" { "wrong reason".to_owned() } else { message } }))'),
    (OWNERS[1], "exchange_classes.rs", "serde_json::from_str(&text)", 'serde_json::from_str("null")'),
    (OWNERS[2], "exchange_classes.rs", "crate::class_decode::decode(v.clone())", "crate::class_decode::decode(EventValue::Null)"),
]


def measure(label):
    log = Path("/tmp/port-txflow-exchange-bridge-" + label + ".log")
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
    assert len(selected) == 3, (label, results)
    assert all(not row.get("classificationOnly") for row in selected), (label, selected)
    log.with_suffix(".json").write_text(json.dumps(selected, indent=2) + "\n")
    return selected


baseline = measure("baseline-green")
assert all(not row["failures"] for row in baseline), baseline
receipts = []
for owner, filename, before, after in FAULTS:
    path = ROOT / "safe-cex-venue-binance/src" / filename
    original = path.read_bytes()
    text = original.decode()
    # Locate only the compiler-selected function; preserve all sibling bodies.
    start = re.search(r"\bfn " + re.escape(owner.split("::")[-1]) + r"[<(]", text)
    assert start, owner
    following = re.search(r"\n(?:pub )?(?:async )?fn ", text[start.end():])
    end = start.end() + following.start() if following else len(text)
    body = text[start.start():end]
    assert body.count(before) == 1, owner
    try:
        path.write_text(text[:start.start()] + body.replace(before, after) + text[end:])
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
Path("/tmp/port-txflow-exchange-bridge-controls.json").write_text(json.dumps(receipts, indent=2) + "\n")
