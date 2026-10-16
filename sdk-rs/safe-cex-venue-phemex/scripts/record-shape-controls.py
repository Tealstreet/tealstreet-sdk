"""Prove each existing concrete record guard rejects its complete valid array."""
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / "safe-cex-venue-phemex/src/types.rs"
CASES = [
    ("PhemexAccount", "account_native_fields_and_complete_array_boundary", "ACCOUNT"),
    ("PhemexPosition", "position_native_fields_and_complete_array_boundary", "POSITION"),
    ("PositionsResponse", "positions_native_fields_and_complete_array_boundary", "POSITIONS"),
]


def run(test):
    result = subprocess.run([
        "cargo", "test", "--locked", "--jobs", "1", "-q", "-p",
        "safe-cex-venue-phemex", "--test", "accepted_shapes", test, "--", "--exact",
    ], cwd=ROOT, capture_output=True, text=True)
    return result.returncode, result.stdout + result.stderr


def passing(test):
    code, output = run(test)
    assert code == 0 and "test result: ok. 1 passed" in output, output


for owner, test, label in CASES:
    passing(test)
    original = SOURCE.read_bytes()
    start = original.index(f"impl<'de> Deserialize<'de> for {owner} {{".encode())
    end = original.index(f"impl JsNumberOutput for {owner} {{".encode(), start)
    region = original[start:end]
    guard = b"safe_cex_types::strict_decode::object(d)"
    assert region.count(guard) == 1
    try:
        SOURCE.write_bytes(original[:start] + region.replace(guard, b"d") + original[end:])
        code, output = run(test)
        assert code == 101 and "test result: FAILED" in output and f"---- {test} stdout ----" in output, output
        assert f"{label} complete valid array: text" in output, output
    finally:
        SOURCE.write_bytes(original)
    assert SOURCE.read_bytes() == original
    passing(test)
    print(f"{owner}: guard-only removal compiles, complete array fails101; exact restoration PASS", flush=True)
