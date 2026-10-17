#!/usr/bin/env python3
"""Check retry condition coverage through the production HTTP factory."""
from pathlib import Path
import subprocess
import tempfile

root = Path(__file__).resolve().parents[2]
source = root / "safe-cex-venue-bybit/src/api_factory.rs"
original = source.read_bytes()
before = b'error.request.url != "/v5/market/instruments-info"'
after = b'error.request.url == "/v5/market/instruments-info"'
assert original.count(before) == 1
command = ["cargo", "test", "--manifest-path", str(root / "Cargo.toml"),
           "-p", "safe-cex-venue-bybit", "--test", "api_factory", "--locked",
           "--jobs", "1", "native_retry_failure_config_and_original_read_guard",
           "--", "--exact"]
logs = Path(tempfile.mkdtemp(prefix="bybit-factory-condition-"))


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
    assert "factory_retry_config_native_attempts" in output, output
finally:
    source.write_bytes(original)

code, output = run("restored")
assert code == 0, output
assert source.read_bytes() == original
print("PASS: compiled production retry predicate defect fails; exact restore passes")
print(logs)
