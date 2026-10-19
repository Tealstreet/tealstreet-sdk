"""Require named compiled failures in the actual constructed cloid consumers."""
import os
from pathlib import Path
import subprocess
import tempfile

package = Path(__file__).resolve().parents[1]
workspace = package.parent
source = package / "src/cloid.rs"
original = source.read_bytes()
text = original.decode()
logs = Path(tempfile.mkdtemp(prefix="hl-cloid-audit-controls-"))
env = {**os.environ, "CARGO_INCREMENTAL": "0"}
command = ["cargo", "test", "--locked", "--jobs", "1", "-p",
           "safe-cex-venue-hyperliquid", "--lib", "binding::cloid_audit::cloid_audit_"]


def run(label):
    result = subprocess.run(command, cwd=workspace, env=env, capture_output=True, text=True)
    output = result.stdout + result.stderr
    (logs / f"{label}.log").write_text(output)
    return result.returncode, output


print(f"logs: {logs}", flush=True)
status, output = run("baseline")
assert status == 0 and "2 passed; 0 failed" in output, output
faults = [
    ("pattern", 'bytes.starts_with(b"0x")', 'bytes.starts_with(b"0X")',
     "cloid_audit_pattern_controls_the_exported_lookup_request"),
    ("record", "Some(Value::Bool(flag)) => *flag,", "Some(Value::Bool(flag)) => !*flag,",
     "cloid_audit_generated_wires_reach_the_real_placement_cache"),
]
try:
    for name, before, after, test in faults:
        assert text.count(before) == 1, before
        source.write_text(text.replace(before, after))
        status, output = run(f"{name}-red")
        assert status == 101 and "error[E" not in output, output
        assert f"{test} ... FAILED" in output and "HL_CLOID_AUDIT" in output, output
        print(f"{name}: compiled named consumer RED", flush=True)
        source.write_bytes(original)
        status, output = run(f"{name}-restored")
        assert status == 0 and "2 passed; 0 failed" in output, output
        assert source.read_bytes() == original
        print(f"{name}: exact restoration GREEN", flush=True)
finally:
    source.write_bytes(original)
print("2 compiled cloid consumer controls/restorations PASS", flush=True)
