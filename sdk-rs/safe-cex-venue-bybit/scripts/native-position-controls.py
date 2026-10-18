#!/usr/bin/env python3
"""Compile four projection faults with bounded Cargo runs and private logs.

The log directory is printed before execution; faults restore exact source bytes.
"""
from pathlib import Path
import os
import subprocess
import tempfile

root = Path(__file__).resolve().parents[3]
controls = root / "sdk-rs/safe-cex-venue-bybit/src/binding/members/controls.rs"
orders = root / "sdk-rs/safe-cex-venue-bybit/src/binding/members/orders.rs"
original = {p: p.read_text() for p in (controls, orders)}
command = ["cargo", "test", "--locked", "--manifest-path", "sdk-rs/Cargo.toml", "-p", "safe-cex-venue-bybit", "--test", "exchange_binding", "--", "--include-ignored"]
env = dict(os.environ)
env.setdefault("SAFE_CEX_TYPESCRIPT_PATH", str(root / "node_modules/typescript"))
faults = [
    ("cached", controls, "let current = if current.is_nullish() {", "let current = from_json(&to_json(&current)?);\n        let current = if current.is_nullish() {", "current_nan_leverage_still_sends_the_source_update"),
    ("initial", controls, 'store.call_method("addOrUpdatePositions", vec![positions.clone()])?;', 'store.call_method("addOrUpdatePositions", vec![from_json(&to_json(&positions)?)])?;', "native_leverage_keeps_option_refusal_and_not_modified_policy"),
    ("refresh", controls, 'store.call_method("addOrUpdatePositions", vec![positions])?;', 'store.call_method("addOrUpdatePositions", vec![from_json(&to_json(&positions)?)])?;', "native_leverage_keeps_option_refusal_and_not_modified_policy"),
    ("mode", orders, 'store.call_method("addOrUpdatePositions", vec![mapped.clone()])?;', 'store.call_method("addOrUpdatePositions", vec![from_json(&to_json(&mapped)?)])?;', "position_mode_publication_retains_native_nan_and_negative_zero"),
]

log_dir = Path(tempfile.mkdtemp(prefix="bybit42-controls-"))
print(f"Control logs: {log_dir}", flush=True)

def run(label):
    result = subprocess.run(command, cwd=root, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=600)
    (log_dir / (label + ".txt")).write_text(result.stdout)
    return result

baseline = run("baseline")
assert baseline.returncode == 0 and "28 passed" in baseline.stdout, baseline.stdout
try:
    for label, target, before, after, witness in faults:
        assert original[target].count(before) == 1, label
        target.write_text(original[target].replace(before, after, 1))
        result = run(label)
        assert result.returncode == 101 and "panicked at" in result.stdout and witness in result.stdout and "error[E" not in result.stdout, (label, result.stdout)
        print(label + ": compiled projection fault caught", flush=True)
        target.write_text(original[target])
finally:
    for target, content in original.items():
        target.write_text(content)
restored = run("restored")
assert restored.returncode == 0 and "28 passed" in restored.stdout, restored.stdout
assert all(target.read_text() == content for target, content in original.items())
print("Four compiled numeric projection faults caught; exact restoration passes all 28 binding tests.", flush=True)
