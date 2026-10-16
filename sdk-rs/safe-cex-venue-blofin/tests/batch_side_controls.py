"""Prove the BatchPositionSide String admission guard with a compiled mutation."""
import pathlib
import subprocess

sdk = pathlib.Path(__file__).resolve().parents[2]
source = sdk / "safe-cex-venue-blofin/src/types.rs"
baseline = source.read_bytes()
guard = (
    b"let value = String::deserialize(d)?;\n"
    b"        Wire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(value))"
)
assert baseline.count(guard) == 1, "review the changed String admission guard"
logs = sdk / "target/blofin-batch-side-controls"
logs.mkdir(parents=True, exist_ok=True)
command = [
    "cargo", "test", "-p", "safe-cex-venue-blofin", "--test", "types",
    "--locked", "--jobs", "1",
    "batch_position_side_accepts_only_declared_strings", "--", "--exact",
]


def run(name):
    log = logs / f"{name}.log"
    with log.open("w") as output:
        result = subprocess.run(command, cwd=sdk, stdout=output, stderr=subprocess.STDOUT)
    return result.returncode, log.read_text()


try:
    source.write_bytes(baseline.replace(guard, b"Wire::deserialize(d)"))
    code, text = run("guard-removed")
finally:
    source.write_bytes(baseline)

assert code == 101, text
assert "batch_position_side_string_only serde text" in text, text
assert "test result: FAILED" in text, text
code, text = run("guard-restored")
assert code == 0 and "1 passed; 0 failed" in text, text
assert source.read_bytes() == baseline, "production bytes changed during the control"
print("PASS: compiled guard removal fails tagged text; exact restoration passes")
