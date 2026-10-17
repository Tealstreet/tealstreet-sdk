"""Compile a Rust-round mutation and require the real placement witness to fail."""
from pathlib import Path
import signal
import subprocess

crate = Path(__file__).resolve().parents[1]
source = crate / "src/builder.rs"
baseline = source.read_bytes()
needle = "safe_cex_num::math_round(parsed * 10_000.0)"
assert baseline.decode().count(needle) == 1
command = ["cargo", "test", "-p", "safe-cex-venue-grvt", "--test", "exchange_binding",
           "placement_reports_negative_half_builder_caps_with_source_rounding", "--", "--exact"]

def interrupted(_signal, _frame):
    raise KeyboardInterrupt("restoring production bytes")
signal.signal(signal.SIGTERM, interrupted)
try:
    source.write_text(baseline.decode().replace(needle, "(parsed * 10_000.0).round()"))
    mutant = subprocess.run(command, cwd=crate.parent, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    Path("/tmp/grvt-negative-half-mutant.log").write_text(mutant.stdout)
    assert mutant.returncode == 101 and "test result: FAILED" in mutant.stdout
    assert "real placement exposes source-rounded cap" in mutant.stdout
    assert "could not compile" not in mutant.stdout
finally:
    source.write_bytes(baseline)
restored = subprocess.run(command, cwd=crate.parent, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
Path("/tmp/grvt-negative-half-restored.log").write_text(restored.stdout)
assert restored.returncode == 0 and "test result: ok" in restored.stdout
assert source.read_bytes() == baseline
print("Rust round mutation: compiled binding failure; exact restoration passed")
