#!/usr/bin/env python3
"""Prove the stock-F64 witnesses execute the concrete retained capture visitor."""
from pathlib import Path
import subprocess

workspace = Path(__file__).resolve().parents[2]
source = workspace / "safe-cex-types/src/retained_json.rs"
original = source.read_bytes()
method = b"    fn visit_f64<E: de::Error>(self, v: f64) -> Result<Self::Value, E> {\n        BufferVisitor.visit_f64(v)\n    }\n"
assert original.count(method) == 1, "expected exactly the reviewed Capture float visitor"
targets = [
    ("safe-cex-types", "retained_native_f64", "stock_f64_events_retain_native_bits_before_json_projection"),
    ("safe-cex-num", "passthrough_native_f64", "passthrough_accepts_stock_f64_events_and_refuses_nan_projection"),
]


def run(package, target):
    result = subprocess.run(
        ["cargo", "test", "-p", package, "--test", target],
        cwd=workspace, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
    )
    print(result.stdout, end="", flush=True)
    return result


try:
    source.write_bytes(original.replace(method, b""))
    for package, target, name in targets:
        result = run(package, target)
        assert result.returncode == 101, f"{target}: missing runtime rejection"
        assert f"test {name} ... FAILED" in result.stdout, f"{target}: named witness did not fail"
        assert "invalid type: floating point" in result.stdout, f"{target}: wrong failure cause"
finally:
    source.write_bytes(original)

assert source.read_bytes() == original, "production source must be exactly restored"
for package, target, _ in targets:
    assert run(package, target).returncode == 0, f"{target}: restoration must pass"
print("Both missing-float-visitor controls compiled and failed their named assertions; exact restoration passes.")
