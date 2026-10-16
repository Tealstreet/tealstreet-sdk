#!/usr/bin/env python3
"""Prove a production Rust ticker-patch mutation fails the named live oracle."""
from __future__ import annotations

import os
import subprocess
from pathlib import Path

crate = Path(__file__).resolve().parents[1]
sdk = crate.parent
repo = sdk.parent
source = crate / "src/ws_public.rs"
original = source.read_text()
before = 'patch.set_number("quoteVolume", next.quote_volume);'
after = 'patch.set_number("quoteVolume", next.volume);'
if original.count(before) != 1:
    raise SystemExit("expected one quoteVolume Rust mutation anchor")
command = [
    "cargo", "test", "--manifest-path", str(sdk / "Cargo.toml"),
    "-p", "safe-cex-venue-mexc", "--test", "ws", "--jobs", "2", "--",
    "mexc_public_ticker_handler_patches_only_source_fields", "--exact", "--nocapture",
]
try:
    source.write_text(original.replace(before, after))
    negative = subprocess.run(command, cwd=repo, env=os.environ.copy(), text=True, capture_output=True)
    combined = negative.stdout + negative.stderr
    if negative.returncode != 101:
        raise SystemExit(f"Rust mutation should fail with 101, got {negative.returncode}:\n{combined}")
    if "mexc_public_ticker_handler_patches_only_source_fields ... FAILED" not in combined:
        raise SystemExit(f"mutation did not fail the named ticker handler test:\n{combined}")
    if "ticker field bits quoteVolume" not in combined:
        raise SystemExit(f"mutation failed for an unexpected reason:\n{combined}")
finally:
    source.write_text(original)

positive = subprocess.run(command, cwd=repo, env=os.environ.copy(), text=True, capture_output=True)
if positive.returncode != 0:
    raise SystemExit(f"restored Rust ticker handler did not pass:\n{positive.stdout}{positive.stderr}")
print("Rust quoteVolume mutation failed at named ticker field bits assertion (101); restored source passes.")
