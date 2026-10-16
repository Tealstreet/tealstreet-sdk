"""Compile each production mutation, require its named native failure, restore."""
from pathlib import Path
import hashlib
import json
import os
import subprocess
import time

ROOT = Path(__file__).resolve().parents[3]
SOURCE = ROOT / "sdk-rs/safe-cex-venue-bybit/src/inverse_socket_bodies.rs"
REVISION = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
OUTPUT = ROOT / ".aimux/inverse-socket-controls" / REVISION / str(time.time_ns())
OUTPUT.mkdir(parents=True)
ORIGINAL = SOURCE.read_text()
COMMAND = ["cargo", "test", "--manifest-path", "sdk-rs/Cargo.toml", "--locked", "-p", "safe-cex-venue-bybit", "--test", "inverse_socket_bodies", "--jobs", "2"]
ENV = dict(os.environ, CARGO_BUILD_JOBS="2")
subprocess.run(["git", "diff", "--exit-code"], cwd=ROOT, check=True, capture_output=True)

RECIPES = [
    ("initializer-double-factory", '    Ok(imported!(h, "createIncrementalFills", vec![]))', '    let _ = imported!(h, "createIncrementalFills", vec![]);\n    Ok(imported!(h, "createIncrementalFills", vec![]))', "factory_retains_fresh_callable_state", "initializer invokes factory exactly once per field"),
    ("private-retry-boundary", "h.number(3.0)", "h.number(2.0)", "private_dispatch_rereads_frame_and_starts_retry_eagerly", "private native retry threshold"),
    ("order-inverse-price", 'vec![h.get(&fill, "qty")?, h.get(&fill, "value")?]', 'vec![h.get(&fill, "value")?, h.get(&fill, "qty")?]', "order_guard_fills_statuses_and_outer_iteration_have_native_values", "order native inverse execution price"),
    ("position-pair-alias", 'h.array(vec![first.clone(), first])', 'h.array(vec![first.clone(), h.undefined()])', "positions_retain_pair_identity_and_timestamp_assignment_receiver", "position native pair aliases"),
    ("public-prefix-drop", "invoke_handlers(h, prefix, Some(json))?;", "let _ = (prefix, json);", "public_dispatch_retains_arrays_utf16_and_live_iteration", "public captured prefix root and live append"),
    ("iterator-object-validation", '        h.intrinsic("require iterator result object", vec![result.clone()])?;\n', "", "iterator_rejects_primitive_result_before_property_gets", "iterator native primitive result rejected"),
]
assert [r[0] for r in RECIPES] == ["initializer-double-factory", "private-retry-boundary", "order-inverse-price", "position-pair-alias", "public-prefix-drop", "iterator-object-validation"]
for name, needle, _, _, _ in RECIPES:
    assert ORIGINAL.count(needle) == 1, (name, "unique production mutation anchor")

report = {"tested_commit": REVISION, "source_sha256": hashlib.sha256(ORIGINAL.encode()).hexdigest(), "controls": [], "survivors": [], "invalid_controls": [], "floor_stopped_at": "iterator-object-validation"}
def save():
    (OUTPUT / "controls.json").write_text(json.dumps(report, indent=2) + "\n")
def run(label, test=None):
    assert subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip() == REVISION
    command = COMMAND + ([test, "--", "--exact"] if test else [])
    result = subprocess.run(command, cwd=ROOT, env=ENV, text=True, capture_output=True)
    output = result.stdout + result.stderr
    (OUTPUT / (label + ".log")).write_text(output.replace(str(ROOT), "<worktree>").replace(str(Path.home()), "<home>"))
    return result.returncode, output

report["baseline_exit"], baseline = run("baseline")
save()
assert report["baseline_exit"] == 0, baseline
for name, needle, replacement, test, assertion in RECIPES:
    row = {"id": name, "test": test, "assertion": assertion}
    report["controls"].append(row)
    try:
        assert SOURCE.read_text() == ORIGINAL
        SOURCE.write_text(ORIGINAL.replace(needle, replacement))
        row["mutation_exit"], output = run(name, test)
        row["compiled_named_failure"] = row["mutation_exit"] == 101 and "test result: FAILED" in output and assertion in output
        if row["mutation_exit"] == 0:
            report["survivors"].append({"id": name, "call_site": str(SOURCE.relative_to(ROOT)), "test": test})
        elif not row["compiled_named_failure"]:
            report["invalid_controls"].append(name)
    finally:
        SOURCE.write_text(ORIGINAL)
        save()
    row["restored_exit"], restored = run(name + "-restored", test)
    save()
    assert row["compiled_named_failure"], (name, output)
    assert row["restored_exit"] == 0, (name, restored)
    print(name, "compiled named assertion 101 -> restored 0", flush=True)
report["final_baseline_exit"], final = run("final-baseline")
report["source_hash_restored"] = hashlib.sha256(SOURCE.read_bytes()).hexdigest() == report["source_sha256"]
report["scoped_diff_exit"] = subprocess.run(["git", "diff", "--exit-code", "--", str(SOURCE.relative_to(ROOT))], cwd=ROOT, capture_output=True).returncode
save()
assert report["final_baseline_exit"] == 0, final
assert report["source_hash_restored"] and report["scoped_diff_exit"] == 0
print("receipt", OUTPUT.relative_to(ROOT), flush=True)
