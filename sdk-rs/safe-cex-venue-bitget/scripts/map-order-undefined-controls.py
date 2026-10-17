"""Compile production representation faults against the real binding and oracle."""
from pathlib import Path
import subprocess

root = Path(__file__).resolve().parents[2]
source = root / "safe-cex-venue-bitget/src/orders.rs"
original = source.read_bytes()
base = ["cargo", "test", "--locked", "--jobs", "1", "-p", "safe-cex-venue-bitget", "--test"]
binding = base + ["algo_frame_binding", "_side_"]
oracle = base + ["order", "frozen_order_decisions", "--", "--exact"]
binding_names = ["real_algo_missing_side_retains_native_undefined_before_store_projection",
                 "real_rest_missing_side_retains_undefined_but_unknown_raw_side_refuses"]

def run(label, command, failures=()):
    result = subprocess.run(command, cwd=root, text=True, stdout=subprocess.PIPE,
                            stderr=subprocess.STDOUT)
    Path("/tmp/bitget-undefined-control-" + label + ".log").write_text(result.stdout)
    if failures:
        assert result.returncode == 101 and "test result: FAILED" in result.stdout, result.stdout
        assert all("test " + name + " ... FAILED" in result.stdout for name in failures), result.stdout
    else:
        assert result.returncode == 0 and "test result: ok" in result.stdout, result.stdout
    print(label, "named failure" if failures else "passed", flush=True)

run("baseline-binding", binding)
run("baseline-oracle", oracle)
s = original.decode()
needle = "        TableHit::Missing => Ok(ModelField::Undefined),"
assert s.count(needle) == 1
controls = [
    ("old-throw", s.replace(needle, '        TableHit::Missing => Err(failure("RepresentationError", &format!("mapOrder cannot represent {field} in shared Order"))),'), binding_names),
    ("absent-property", s.replace(needle, "        TableHit::Missing => Ok(ModelField::Absent),"), binding_names),
]
needle = "        Some(_) => (TableHit::Missing, true),"
assert s.count(needle) == 1
controls.append(("unsupported-raw-side", s.replace(needle, "        Some(_) => (TableHit::Missing, false),"), binding_names[1:]))
for label, mutation, failures in controls:
    try:
        source.write_text(mutation)
        run(label + "-binding", binding, failures)
        run(label + "-oracle", oracle, ["frozen_order_decisions"])
    finally:
        source.write_bytes(original)
    run(label + "-restored-binding", binding)
    run(label + "-restored-oracle", oracle)
assert source.read_bytes() == original
