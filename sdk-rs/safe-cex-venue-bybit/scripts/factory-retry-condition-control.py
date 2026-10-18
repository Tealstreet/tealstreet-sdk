#!/usr/bin/env python3
"""Check retry condition coverage through the production HTTP factory."""
from pathlib import Path
import subprocess
import tempfile

root = Path(__file__).resolve().parents[2]
source = root / "safe-cex-venue-bybit/src/api_factory.rs"
original = source.read_bytes()
controls = [
    (b'error.request.url != "/v5/market/instruments-info"',
     b'error.request.url == "/v5/market/instruments-info"',
     "native_retry_failure_config_and_original_read_guard",
     "factory_retry_config_native_attempts"),
    (b"                && original_read\n", b"                && (original_read || true)\n",
     "native_order_mutation_network_failures_never_replay_post_bodies",
     "mutation_network_single_attempt"),
]
command = ["cargo", "test", "--manifest-path", str(root / "Cargo.toml"),
           "-p", "safe-cex-venue-bybit", "--test", "api_factory", "--locked",
           "--jobs", "1"]
logs = Path(tempfile.mkdtemp(prefix="bybit-factory-condition-"))


def run(label, target):
    path = logs / (label + ".log")
    with path.open("w") as output:
        result = subprocess.run(command + [target, "--", "--exact"], cwd=root, stdout=output,
                                stderr=subprocess.STDOUT, timeout=300)
    return result.returncode, path.read_text()


for index, (before, after, target, failure) in enumerate(controls):
    assert original.count(before) == 1
    try:
        source.write_bytes(original.replace(before, after))
        code, output = run(f"mutant-{index}", target)
        assert code == 101 and "test result: FAILED" in output, output
        assert failure in output, output
    finally:
        source.write_bytes(original)
    code, output = run(f"restored-{index}", target)
    assert code == 0, output
    assert source.read_bytes() == original
print("PASS: both compiled retry predicate defects fail; exact restores pass")
print(logs)
