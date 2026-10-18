#!/usr/bin/env python3
"""Run canonical scoped measurement, not a private probe/filter implementation.

Each temporary fault must compile and produce its named failing runtime receipt.
Restore exact production bytes and require all four owners to pass afterwards.
"""
import json
import os
from pathlib import Path
import re
import subprocess

ROOT = Path(__file__).resolve().parents[2]
TARGET = ROOT / "target/accepted-inventory"
OWNERS = [
    "account_leverage_overlay::build_overlay",
    "closed_position_reader::derive_binance_closed_positions",
    "lifecycle::loaded",
    "private_helpers::detach_binance_private_socket",
]
COMMAND = ["node", "safe-cex-parity/scripts/accepted-set.mjs",
           "--package=safe-cex-venue-binance", "--jobs=1", "--measure"]
COMMAND += ["--owner=" + name for name in OWNERS]
FAULTS = [
    (OWNERS[0], "account_leverage_overlay.rs", "maxima.insert(symbol.to_owned(), max);",
     "maxima.insert(symbol.to_owned(), max + 1.0);"),
    (OWNERS[1], "closed_position_reader.rs", "b.closed_time\n                .partial_cmp(&a.closed_time)",
     "a.closed_time\n                .partial_cmp(&b.closed_time)"),
    (OWNERS[2], "lifecycle.rs", "out.write(key, EventValue::Bool(true))",
     "out.write(key, EventValue::Bool(false))"),
    (OWNERS[3], "private_helpers.rs", 'remove_listener(&previous, "message")?;',
     'remove_listener(&previous, "wrong-event")?;'),
]


def measure(label):
    log = Path("/tmp/port-txflow-residual-" + label + ".log")
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
    # The PM duplicate may still exist until #938 lands. Locate the assigned
    # function, never mutate its separately owned sibling by a global replace.
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
Path("/tmp/port-txflow-residual-controls.json").write_text(json.dumps(receipts, indent=2) + "\n")
