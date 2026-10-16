#!/usr/bin/env python3
"""Disconnect five real producer/getter/state operations; require native receipt failures."""
from pathlib import Path
import json
import subprocess

ROOT = Path(__file__).resolve().parents[3]
PACKAGE = ROOT / "sdk-rs/safe-cex-transport"
# The explicit runner uses the actual production compiler inventory and stored
# contracts, instrumenting native code without cfg(test). It must write fresh
# receipts even for expected behavioral failures; a compiler failure is refused.
RUNNER = PACKAGE / "scripts/accepted-value-owner-native.mjs"
RESULTS = Path("/tmp/transport-value-owner-control-native-results.json")
INVENTORY = Path("/tmp/transport-value-owner-control-inventory.json")
CASES = [
    ("provider.rs", "provider::NetworkFailure", "Self { message, code }", "Self { message: { let _ = &message; String::new() }, code }"),
    ("provider.rs", "provider::RestFailure", "transport_error: None,", "transport_error: Some(Box::new(TransportError::Timeout)),"),
    ("provider.rs", "provider::TransformedData", "        &self.value\n", "        { static WRONG: serde_json::Value = serde_json::Value::Null; &WRONG }\n"),
    ("fallback.rs", "fallback::WsDirectFallback", ".checked_sub(1)", ".checked_sub(0)"),
    ("retry.rs", "retry::RetryPolicy", "milliseconds: Box::new(milliseconds),", "milliseconds: Box::new(move |n: usize| { let _ = &milliseconds; n as u64 }),"),
]


def run(name, restored):
    RESULTS.unlink(missing_ok=True)
    result = subprocess.run(["node", str(RUNNER), "--inventory=" + str(INVENTORY), "--out=" + str(RESULTS)], cwd=ROOT, capture_output=True, text=True)
    output = result.stdout + result.stderr
    suffix = "restored" if restored else "disconnected"
    Path(f"/tmp/transport-value-{name.split('::')[-1]}-{suffix}.log").write_text(output)
    assert RESULTS.exists(), f"{name}: no native receipts; compilation failure is not a control\n{output}"
    receipts = json.loads(RESULTS.read_text())
    failed = [row for row in receipts if row["failures"]]
    assert len(receipts) == 24, (name, len(receipts))
    if restored:
        assert result.returncode == 0 and not failed, output
    else:
        assert result.returncode != 0 and len(failed) == 1, output
        assert failed[0]["name"] == "safe_cex_transport::" + name, failed
        assert all("function output mismatch" in failure for failure in failed[0]["failures"]), failed
    print(f"{name}: {suffix} native {'PASS' if restored else 'named output FAIL'}", flush=True)


# Discover the actual unmodified compiler owners once before changing effects.
# Every mutant keeps signatures and line counts; fresh native compilation still
# invokes those exact APIs, and no source guard permission is relaxed.
prepare = subprocess.run(
    ["node", str(ROOT / "sdk-rs/safe-cex-parity/scripts/accepted-set.mjs"), "--package=safe-cex-transport", "--inventory", "--jobs=1"],
    cwd=ROOT / "sdk-rs", capture_output=True, text=True,
)
Path("/tmp/transport-value-owner-control-discovery.log").write_text(prepare.stdout + prepare.stderr)
assert prepare.returncode == 0, prepare.stdout + prepare.stderr
metadata = subprocess.run(["cargo", "metadata", "--locked", "--format-version=1"], cwd=ROOT / "sdk-rs", capture_output=True, text=True)
assert metadata.returncode == 0, metadata.stderr
INVENTORY.write_bytes((Path(json.loads(metadata.stdout)["target_directory"]) / "accepted-set-inventory.json").read_bytes())

snapshots = {PACKAGE / "src" / filename: (PACKAGE / "src" / filename).read_bytes() for filename, *_ in CASES}
try:
    for filename, name, old, new in CASES:
        path = PACKAGE / "src" / filename
        source = snapshots[path].decode()
        assert source.count(old) == 1, (name, source.count(old))
        try:
            path.write_text(source.replace(old, new))
            run(name, False)
        finally:
            path.write_bytes(snapshots[path])
        run(name, True)
finally:
    for path, source in snapshots.items():
        path.write_bytes(source)
assert all(path.read_bytes() == source for path, source in snapshots.items())
print("Five compiling producer/getter/state faults failed their named native outputs; exact restoration passed", flush=True)
