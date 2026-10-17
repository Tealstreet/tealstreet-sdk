"""Remove the production hash upper bound; require live placement failure."""
from pathlib import Path
import signal
import subprocess
crate = Path(__file__).resolve().parents[1]
source = crate / "src/signing_metadata.rs"
baseline = source.read_bytes()
needle = b"        && hash.len() <= 66\n"
assert baseline.count(needle) == 1
command = ["cargo", "test", "-p", "safe-cex-venue-grvt", "--test", "exchange_binding",
           "placement_enforces_instrument_hash_length_before_builder_lookup_and_signing", "--", "--exact"]
def interrupted(_signal, _frame):
    raise KeyboardInterrupt("restoring production bytes")
signal.signal(signal.SIGTERM, interrupted)
try:
    source.write_bytes(baseline.replace(needle, b""))
    mutant = subprocess.run(command, cwd=crate.parent, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    Path("/tmp/grvt-hash-length-mutant.log").write_text(mutant.stdout)
    assert mutant.returncode == 101 and "test result: FAILED" in mutant.stdout
    assert "placement hash-length boundary" in mutant.stdout
    assert "could not compile" not in mutant.stdout
finally:
    source.write_bytes(baseline)
restored = subprocess.run(command, cwd=crate.parent, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
Path("/tmp/grvt-hash-length-restored.log").write_text(restored.stdout)
assert restored.returncode == 0 and "test result: ok" in restored.stdout
assert source.read_bytes() == baseline
print("Hash upper bound: compiled placement failure; exact restoration passed")
