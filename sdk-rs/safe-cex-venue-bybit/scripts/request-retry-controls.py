#!/usr/bin/env python3
"""Prove the request namespace and factory budget independently, then restore."""
from pathlib import Path
import os
import subprocess
import tempfile

root = Path(__file__).resolve().parents[3]
factory = root / "sdk-rs/safe-cex-venue-bybit/src/api_factory.rs"
bridge = root / "sdk-rs/safe-cex-venue-bybit/src/binding/http.rs"
original = {path: path.read_bytes() for path in (factory, bridge)}
log_dir = Path(tempfile.mkdtemp(prefix="bybit-request-retry-"))
print(f"Control logs: {log_dir}", flush=True)
env = dict(os.environ)
env.setdefault("SAFE_CEX_TYPESCRIPT_PATH", str(root / "node_modules/typescript"))
base = ["cargo", "test", "--locked", "--manifest-path", "sdk-rs/Cargo.toml", "-p", "safe-cex-venue-bybit", "--jobs", "1"]
checks = [
    ("factory", factory, b"retries - flags.retries_consumed as f64", b"({ let _ = retries; 3.0 }) - flags.retries_consumed as f64",
     ["--test", "api_factory", "native_request_retry_override_retains_source_budget", "--", "--exact"], "request_retry_budget_attempts rejected-sign-zero"),
    ("bridge", bridge, b'let retry = config.get("axios-retry")?;', b'let retry = Js::Undefined;',
     ["--lib", "request_keeps_per_request_retry_namespace"], "binding_retry_namespace_reaches_typed_factory"),
]

def run(label, args):
    with (log_dir / f"{label}.txt").open("w") as output:
        result = subprocess.run(base + args, cwd=root, env=env, stdout=output, stderr=subprocess.STDOUT, timeout=600)
    return result.returncode, (log_dir / f"{label}.txt").read_text()

for name, _, _, _, args, _ in checks:
    code, output = run(f"baseline-{name}", args)
    assert code == 0 and "1 passed" in output, output
try:
    for name, target, before, after, args, witness in checks:
        assert original[target].count(before) == 1, name
        target.write_bytes(original[target].replace(before, after, 1))
        code, output = run(f"fault-{name}", args)
        assert code == 101 and "panicked at" in output and witness in output and "error[E" not in output, output
        print(f"{name}: compiled fault caught by {witness}", flush=True)
        target.write_bytes(original[target])
finally:
    for target, content in original.items():
        target.write_bytes(content)
for name, _, _, _, args, _ in checks:
    code, output = run(f"restored-{name}", args)
    assert code == 0 and "1 passed" in output, output
assert all(target.read_bytes() == content for target, content in original.items())
print("Both faults compiled and failed their named runtime assertions; exact restoration passes.", flush=True)
