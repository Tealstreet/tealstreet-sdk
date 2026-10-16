"""Prove each concrete string frontdoor excludes Serde's tagged-map form."""
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / "safe-cex-venue-cryptocom/src/wire.rs"
OWNERS = [
    "CryptocomTradeSide", "CryptocomJournalType", "CryptocomLiquidityIndicator",
    "CryptocomOrderType", "CryptocomTimeInForce", "CryptocomRefPriceType",
    "CryptocomOrderStatus",
]


def run(test):
    result = subprocess.run([
        "cargo", "test", "--locked", "--jobs", "1", "-q", "-p",
        "safe-cex-venue-cryptocom", "--test", "accepted_string_shapes", test,
        "--", "--exact",
    ], cwd=ROOT, capture_output=True, text=True)
    return result.returncode, result.stdout + result.stderr


def passing(test):
    code, output = run(test)
    assert code == 0 and "test result: ok. 1 passed" in output, output


for owner in OWNERS:
    test = "string_only_" + owner.removeprefix("Cryptocom").lower()
    passing(test)
    original = SOURCE.read_bytes()
    start = original.index(f"impl<'de> Deserialize<'de> for {owner} {{".encode())
    end = original.index(f"impl JsNumberOutput for {owner} {{".encode(), start)
    region = original[start:end]
    assert region.count(b"String::deserialize(deserializer)?") == 1, owner
    assert f"Wire{owner}::deserialize".encode() in region, owner
    body_start = region.index(b"{\n", region.index(b"fn deserialize")) + 2
    body_end = region.rindex(b"\n    }")
    widened = region[:body_start] + f"        Wire{owner}::deserialize(deserializer)".encode() + region[body_end:]
    try:
        SOURCE.write_bytes(original[:start] + widened + original[end:])
        code, output = run(test)
        assert code == 101 and "test result: FAILED" in output and f"---- {test} stdout ----" in output, output
        assert "tagged-shape admission" in output, output
    finally:
        SOURCE.write_bytes(original)
    assert SOURCE.read_bytes() == original
    passing(test)
    print(f"{owner}: string-only removal compiled, tagged map assertion failed101; exact restoration PASS", flush=True)
