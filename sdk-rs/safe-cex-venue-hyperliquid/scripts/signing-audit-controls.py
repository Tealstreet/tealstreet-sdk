"""Require named runtime failures, then restore exact production bytes."""
import os
from pathlib import Path
import subprocess
import tempfile

sdk = Path(__file__).resolve().parents[2]
source = sdk / "safe-cex-venue-hyperliquid/src/signing.rs"
original = source.read_bytes()
logs = Path(tempfile.mkdtemp(prefix="hl-signing-audit-controls-"))
nonce_start = original.index(b"    // BigInt(number) rejects fractional numeric nonces")
nonce_end = original.index(b"    data.push(u8::from(vault.is_some()));", nonce_start)
old_nonce = b"    data.extend_from_slice(&nonce_u64(nonce).map_err(signing_error)?.to_be_bytes());\n"
scenes = [
    ("original-nonce-error", original[nonce_start:nonce_end], old_nonce,
     ["signing_audit_bound_fractional_nonce_retains_range_error", "signing_audit_api_nonce_error_retains_transport_cause"]),
    ("wrong-network", b'("message", construct_phantom_agent(&hash, mainnet)),',
     b'("message", construct_phantom_agent(&hash, !mainnet)),',
     ["signing_audit_bound_nonce_signatures_match_maintained_source"]),
]


def run(log):
    command = ["cargo", "test", "--locked", "--jobs", "1", "-p",
               "safe-cex-venue-hyperliquid", "--lib", "binding::signing_audit::"]
    result = subprocess.run(command, cwd=sdk, env={**os.environ, "CARGO_INCREMENTAL": "0"},
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    log.write_text(result.stdout)
    return result


for scene, before, after, names in scenes:
    assert source.read_bytes() == original
    assert original.count(before) == 1, scene
    try:
        source.write_bytes(original.replace(before, after))
        red = run(logs / f"{scene}-red.log")
        assert red.returncode == 101, (scene, red.returncode, logs)
        assert "HL_SIGNING" in red.stdout and "panicked at" in red.stdout, (scene, logs)
        assert "could not compile" not in red.stdout, (scene, logs)
        for name in names:
            assert name + " ... FAILED" in red.stdout, (scene, name, logs)
    finally:
        source.write_bytes(original)
    assert source.read_bytes() == original
    restored = run(logs / f"{scene}-restored.log")
    assert restored.returncode == 0 and "3 passed; 0 failed" in restored.stdout, (scene, logs)
    print(f"{scene}: named compiled RED{len(names)} / exact-restored GREEN3; {logs}", flush=True)
