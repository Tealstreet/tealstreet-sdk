#!/usr/bin/env python3
"""Check token consumption through the concrete production Spot host."""
from pathlib import Path
import subprocess
import tempfile

root = Path(__file__).resolve().parents[2]
source = root / "safe-cex-venue-hyperliquid/src/spot_client.rs"
original = source.read_bytes()
before = b'let remaining = h.binary("-", &tokens, &weight)?;'
after = b'let remaining = h.binary("+", &tokens, &weight)?;'
assert original.count(before) == 1
command = ["cargo", "test", "--manifest-path", str(root / "Cargo.toml"),
           "-p", "safe-cex-venue-hyperliquid", "--lib", "--locked",
           "--jobs", "1", "host::spot_limiter_tests::production_spot_limiter_debits_refills_and_recursively_waits",
           "--", "--exact"]
logs = Path(tempfile.mkdtemp(prefix="hyperliquid-spot-limiter-"))


def run(label):
    path = logs / (label + ".log")
    with path.open("w") as output:
        result = subprocess.run(command, cwd=root, stdout=output,
                                stderr=subprocess.STDOUT, timeout=300)
    return result.returncode, path.read_text()


try:
    source.write_bytes(original.replace(before, after))
    code, output = run("mutant")
    assert code == 101 and "test result: FAILED" in output, output
    assert "production spot limiter default token debit" in output, output
finally:
    source.write_bytes(original)

code, output = run("restored")
assert code == 0, output
assert source.read_bytes() == original
print("PASS: compiled production token debit defect fails; exact restore passes")
print(logs)
