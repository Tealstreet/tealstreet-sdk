#!/usr/bin/env python3
"""Compile native production faults and restore exact bytes after each control."""
import os
from pathlib import Path
import subprocess

workspace = Path(__file__).resolve().parents[2]
crate = workspace / "safe-cex-venue-lighter"
env = {**os.environ, "CARGO_BUILD_JOBS": "1", "RUSTUP_TOOLCHAIN": "1.99.0"}
unit = ["cargo", "+1.99.0", "test", "--locked", "--jobs", "1", "-p", "safe-cex-venue-lighter", "--lib", "binding::exchange::alias_tests::structural_alias_"]
placement = ["cargo", "+1.99.0", "test", "--locked", "--jobs", "1", "-p", "safe-cex-venue-lighter", "--test", "exchange_binding", "place_order_signs_an_attributed_create_order", "--", "--exact"]
controls = [
    ("SDK record projection", "src/binding/sdk_client.rs", "object(params.into_iter().map(|(k, v)| (k, to_js(&v))).collect())", "object(params.into_iter().filter(|(k, _)| k != &\"cursor\".encode_utf16().collect::<Vec<_>>()).map(|(k, v)| (k, to_js(&v))).collect())", unit, "UTF16 record cursor code units"),
    ("promise continuation", "src/transaction_promise.rs", "std::mem::take(&mut state.observers)", "{ state.observers.clear(); std::mem::take(&mut state.observers) }", unit, "native promise did not settle"),
    ("shared warning cooldown", "src/ws_private_warnings.rs", "if now - last_warning_at < READONLY_WARNING_COOLDOWN_MS {", "if now - last_warning_at > READONLY_WARNING_COOLDOWN_MS {", unit, "first warning records one account"),
    ("crypto composition", "src/binding/signer/transaction.rs", 'self.call(\n            "schnorrSign",', 'self.call(\n            "poseidon2Hash",', placement, "base64 of a 80-byte Schnorr signature"),
    ("exported SDK validation", "src/sdk_validation.rs", 'let n = bigint(v, "Account index")?;\n    if n < &BigInt::from(0) {', 'let n = bigint(v, "Account index")?;\n    if n > &BigInt::from(0) {', unit, "native exported valid account"),
]

def run(command):
    result = subprocess.run(command, cwd=workspace, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    print(result.stdout, flush=True)
    return result

for label, filename, anchor, fault, command, assertion in controls:
    source = crate / filename
    original = source.read_bytes()
    text = original.decode()
    assert text.count(anchor) == 1, (label, "unique production anchor")
    try:
        source.write_text(text.replace(anchor, fault))
        result = run(command)
        assert result.returncode != 0 and "test result: FAILED" in result.stdout and assertion in result.stdout, (label, "compiled named assertion must fail")
        print(f"{label}: compiled named RED", flush=True)
    finally:
        source.write_bytes(original)
    assert source.read_bytes() == original
    assert run(command).returncode == 0, (label, "exact restoration must pass")
    print(f"{label}: exact restoration GREEN", flush=True)
