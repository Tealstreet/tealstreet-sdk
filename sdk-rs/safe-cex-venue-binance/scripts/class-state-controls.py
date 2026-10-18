#!/usr/bin/env python3
"""Four independently compiled semantic faults; exact restoration after each.

The canonical measurement retains all discovery guards. An infrastructure or
compiler error is never accepted as a named failing native observation.
"""
import hashlib
import json
import os
from pathlib import Path
import subprocess

if not __debug__:
    raise SystemExit("controls require Python assertions")
ROOT = Path(__file__).resolve().parents[2]
TARGET = ROOT / "target/accepted-inventory"
SOURCE = ROOT / "safe-cex-venue-binance/src/exchange_classes.rs"
NAMES = ["BinanceExchange", "BinancePMExchange", "BinanceSpotExchange", "ExchangeState"]
COMMAND = ["node", "safe-cex-venue-binance/scripts/class-state-contracts.mjs"]
ENV = dict(os.environ, CARGO_INCREMENTAL="0", RUSTC_BOOTSTRAP="1", CARGO_BUILD_JOBS="1", CARGO_TARGET_DIR=str(TARGET))


def measure(label):
    log = Path("/tmp/port-txflow-class-state-control-" + label + ".log")
    results_file = TARGET / "accepted-set-results.json"
    results_file.unlink(missing_ok=True)
    # Runner validates success; for faults use the canonical driver directly so
    # all four rows can be checked, including the three unchanged siblings.
    command = ["node", "safe-cex-parity/scripts/accepted-set.mjs", "--package=safe-cex-venue-binance", "--features=native", "--jobs=1", "--measure"]
    command += ["--owner=exchange_classes::" + name for name in NAMES]
    with log.open("w") as output:
        run = subprocess.run(command, cwd=ROOT, env=ENV, stdout=output, stderr=subprocess.STDOUT)
    assert run.returncode == 0, (label, log.read_text()[-6000:])
    rows = json.loads(results_file.read_text())
    assert len(rows) == 4 and {r["name"].split("::")[-1] for r in rows} == set(NAMES), rows
    assert all(not r.get("classificationOnly") and "native" in r["compilerFeatures"] and r["configuration"]["target"] == "native" for r in rows), rows
    log.with_suffix(".json").write_text(json.dumps(rows, indent=2) + "\n")
    return rows


original = SOURCE.read_bytes()
baseline = measure("baseline")
assert all(not r["failures"] for r in baseline), baseline
receipts = []
for owner, before, after in [
    ("BinanceExchange", "ExchangeOwner::new(Venue::Futures, name, options, store, deps)", "ExchangeOwner::new(Venue::Spot, name, options, store, deps)"),
    ("BinancePMExchange", "ExchangeOwner::new(Venue::PM, name, options, store, deps)", "ExchangeOwner::new(Venue::Futures, name, options, store, deps)"),
    ("BinanceSpotExchange", "ExchangeOwner::new(Venue::Spot, name, options, store, deps)", "ExchangeOwner::new(Venue::Futures, name, options, store, deps)"),
    ("ExchangeState", "crate::spot_valuation::get_asset_usd_price(asset, |symbol|", 'crate::spot_valuation::get_asset_usd_price("USD", |symbol|'),
]:
    text = original.decode()
    start = 0 if owner == "ExchangeState" else text.index("impl " + owner + " {")
    end = len(text) if owner == "ExchangeState" else text.index("impl std::ops::Deref for " + owner, start)
    body = text[start:end]
    assert body.count(before) == 1, owner
    try:
        SOURCE.write_text(text[:start] + body.replace(before, after) + text[end:])
        red = measure(owner + "-red")
        changed = next(r for r in red if r["name"].endswith("::" + owner))
        assert any("function output mismatch" in f for f in changed["failures"]), changed
        assert all(not r["failures"] for r in red if r is not changed), red
    finally:
        SOURCE.write_bytes(original)
    assert SOURCE.read_bytes() == original
    green = measure(owner + "-restored")
    assert all(not r["failures"] for r in green), green
    receipts.append({"owner": owner, "compiledNamedRed": True, "exactRestoredGreen": True, "sourceSha256": hashlib.sha256(original).hexdigest()})
    print(owner + " compiled named RED -> exact-restored four-owner GREEN", flush=True)
Path("/tmp/port-txflow-class-state-controls.json").write_text(json.dumps(receipts, indent=2) + "\n")
