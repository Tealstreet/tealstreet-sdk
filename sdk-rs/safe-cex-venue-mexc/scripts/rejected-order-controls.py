#!/usr/bin/env python3
"""Prove Rust and live-source clean-ID mutations fail the named MEXC test."""
from __future__ import annotations

import os
import subprocess
from pathlib import Path

crate = Path(__file__).resolve().parents[1]
sdk = crate.parent
source = crate / "src/exchange.rs"
test = "mexc_rejected_order_state_matches_live_source"
base_env = os.environ.copy()
command = [
    "cargo", "test", "--manifest-path", str(sdk / "Cargo.toml"),
    "-p", "safe-cex-venue-mexc", "--lib", test, "--jobs", "1", "--",
    "--nocapture",
]


def expect_named_failure(env: dict[str, str], label: str) -> None:
    result = subprocess.run(
        command, cwd=sdk.parent, env=env, text=True,
        stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
    )
    assert result.returncode == 101, f"{label} must fail with 101:\n{result.stdout}"
    assert f"{test} ... FAILED" in result.stdout, result.stdout
    assert "running 1 test" in result.stdout, result.stdout
    assert "case " in result.stdout or "shield" in result.stdout, result.stdout


ts_env = base_env.copy()
ts_env["MEXC_REJECTED_TS_CONTROL"] = "keep-suffix"
expect_named_failure(ts_env, "TypeScript clean-ID mutation")
print("TypeScript clean-ID mutation failed at the named Rust parity assertion (101).", flush=True)

original = source.read_text()
before = 'let clean_id = order_id.split("__").next().unwrap_or_default();'
after = "let clean_id = order_id;"
assert original.count(before) == 1, "unique Rust clean-ID mutation anchor"
try:
    source.write_text(original.replace(before, after))
    expect_named_failure(base_env, "Rust clean-ID mutation")
    print("Rust clean-ID mutation failed at the named live-source assertion (101).", flush=True)
finally:
    source.write_text(original)

subprocess.run(command, cwd=sdk.parent, env=base_env, check=True)
print("Restored MEXC rejection handler passes.")

throw_test = "exchange::tests::mexc_rejected_order_propagates_notification_after_commit"
throw_command = [
    "cargo", "test", "--manifest-path", str(sdk / "Cargo.toml"),
    "-p", "safe-cex-venue-mexc", "--lib", "--jobs", "1", "--",
    "--exact", throw_test, "--nocapture",
]
throw_env = base_env.copy()
throw_env["MEXC_REJECTED_THROW_TS_CONTROL"] = "suppress-update"
result = subprocess.run(
    throw_command, cwd=sdk.parent, env=throw_env, text=True,
    stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
)
assert result.returncode == 101, result.stdout
assert f"{throw_test} ... FAILED" in result.stdout, result.stdout
assert "source propagates subscriber throw" in result.stdout, result.stdout
print("TypeScript Store-write suppression fails the named notification assertion (101).", flush=True)

before = ".update_order_status(clean_id, OrderStatus::Canceled)?;"
after = ".update_order_status(clean_id, OrderStatus::Canceled).unwrap_or(());"
original = source.read_text()
assert original.count(before) == 1, "unique Rust notification propagation anchor"
try:
    source.write_text(original.replace(before, after))
    result = subprocess.run(
        throw_command, cwd=sdk.parent, env=base_env, text=True,
        stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
    )
    assert result.returncode == 101, result.stdout
    assert f"{throw_test} ... FAILED" in result.stdout, result.stdout
    assert "called `Result::unwrap_err()` on an `Ok` value" in result.stdout, result.stdout
    print("Rust notification-discard mutation compiles then fails the named test (101).", flush=True)
finally:
    source.write_text(original)

subprocess.run(throw_command, cwd=sdk.parent, env=base_env, check=True)
print("Restored rejected-order notification propagation passes.")
