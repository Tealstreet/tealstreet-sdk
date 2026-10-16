"""Restore the former flatten decoder, require its named numeric failure, restore."""
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / "safe-cex-venue-phemex/src/types.rs"
TEST = "envelope_native_fields_and_complete_array_boundary"


def run():
    result = subprocess.run([
        "cargo", "test", "--locked", "--jobs", "1", "-q", "-p",
        "safe-cex-venue-phemex", "--test", "accepted_shapes", TEST, "--", "--exact",
    ], cwd=ROOT, capture_output=True, text=True)
    return result.returncode, result.stdout + result.stderr


def passing():
    code, output = run()
    assert code == 0 and "test result: ok. 1 passed" in output, output


passing()
original = SOURCE.read_bytes()
start = original.index(b"impl<'de, T: Deserialize<'de>> Deserialize<'de> for PhemexApiResponse<T> {")
end = original.index(b"impl<T: JsNumberOutput> JsNumberOutput for PhemexApiResponse<T>", start)
mutant = original[:start] + original[end:]
needle = b"#[derive(Debug, Clone, PartialEq, Serialize)]\npub struct PhemexApiResponse<T>"
assert mutant.count(needle) == 1
mutant = mutant.replace(needle, b"#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]\npub struct PhemexApiResponse<T>")
try:
    SOURCE.write_bytes(mutant)
    code, output = run()
    assert code == 101 and "test result: FAILED" in output and f"---- {TEST} stdout ----" in output, output
    assert "invalid type: map, expected f64" in output, output
finally:
    SOURCE.write_bytes(original)
assert SOURCE.read_bytes() == original
passing()
print("Phemex former flattened decoder: compiled named numeric failure101; exact restoration PASS")
