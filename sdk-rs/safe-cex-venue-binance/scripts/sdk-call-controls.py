#!/usr/bin/env python3
"""Require compiled named output failures, then restore the actual SDK helper."""
import hashlib
import json
import os
from pathlib import Path
import subprocess

if not __debug__:
    raise SystemExit("SDK call controls require Python assertions; do not use -O")
ROOT = Path(__file__).resolve().parents[2]
TARGET = ROOT / "target/accepted-inventory"
OWNER = "safe_cex_venue_binance::exchange_classes::exchange_trait::call"
COMMAND = ["node", "safe-cex-parity/scripts/accepted-set.mjs",
           "--package=safe-cex-venue-binance", "--features=native", "--jobs=1", "--measure",
           "--owner=exchange_classes::exchange_trait::call"]


def measure(label):
    log = Path("/tmp/port-txflow-sdk-call-" + label + ".log")
    results = TARGET / "accepted-set-results.json"
    results.unlink(missing_ok=True)
    env = dict(os.environ, RUSTC_BOOTSTRAP="1", CARGO_BUILD_JOBS="1", CARGO_TARGET_DIR=str(TARGET))
    with log.open("w") as output:
        process = subprocess.run(COMMAND, cwd=ROOT, env=env, stdout=output, stderr=subprocess.STDOUT)
    assert process.returncode == 0, (label, log.read_text()[-6000:])
    rows = json.loads(results.read_text())
    assert len(rows) == 1 and rows[0]["name"] == OWNER, (label, rows)
    row = rows[0]
    assert not row.get("classificationOnly") and "native" in row["compilerFeatures"], row
    assert row["configuration"]["target"] == "native", row
    log.with_suffix(".json").write_text(json.dumps(rows, indent=2) + "\n")
    return row


assert not measure("control-baseline")["failures"]
source = ROOT / "safe-cex-venue-binance/src/exchange_trait.rs"
original = source.read_bytes()
text = original.decode()
start = text.index("async fn call<T:")
end = text.index("async fn income_call(", start)
body = text[start:end]
faults = [
    ("typed-forwarding", "typed(&value)", "typed(&{ let _ = &value; EventValue::Null })", 6),
    ("argument-metadata", "let args = args.map_err(|e| (*e).clone())?;",
     "let args = args.map_err(|e| ExchangeError::new(e.message.clone()))?;", 2),
]
receipts = []
for label, before, after, count in faults:
    assert body.count(before) == 1, label
    try:
        source.write_text(text[:start] + body.replace(before, after) + text[end:])
        row = measure(label + "-red")
        assert len(row["failures"]) == count, row
        assert all(failure.startswith("function output mismatch") for failure in row["failures"]), row
    finally:
        source.write_bytes(original)
    assert source.read_bytes() == original
    assert not measure(label + "-restored")["failures"]
    receipts.append({"fault": label, "compiledNamedOutputFailures": count, "exactRestoredGreen": True})
    print(label + " compiled RED -> exact restored GREEN", flush=True)
Path("/tmp/port-txflow-sdk-call-controls.json").write_text(json.dumps({
    "sourceSha256": hashlib.sha256(original).hexdigest(), "receipts": receipts}, indent=2) + "\n")
