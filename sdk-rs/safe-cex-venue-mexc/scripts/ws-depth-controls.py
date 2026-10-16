#!/usr/bin/env python3
"""Prove a production Rust depth-frame mutation fails its named live oracle."""
from __future__ import annotations

import os
import subprocess
from pathlib import Path

crate = Path(__file__).resolve().parents[1]
sdk = crate.parent
repo = sdk.parent
source = crate / "src/ws_public.rs"
original = source.read_text()
before = "let raw = data.unwrap_or(frame);"
after = "let raw = frame.get(\"symbol\").unwrap_or(frame);"
if original.count(before) != 1:
    raise SystemExit("expected one depth-frame source fallback mutation anchor")
command = [
    "cargo", "test", "--manifest-path", str(sdk / "Cargo.toml"),
    "-p", "safe-cex-venue-mexc", "--test", "ws", "--jobs", "2", "--",
    "mexc_public_depth_handler_matches_live_typescript", "--exact", "--nocapture",
]
try:
    source.write_text(original.replace(before, after))
    negative = subprocess.run(command, cwd=repo, env=os.environ.copy(), text=True, capture_output=True)
    combined = negative.stdout + negative.stderr
    if negative.returncode != 101:
        raise SystemExit(f"Rust mutation should fail with 101, got {negative.returncode}:\n{combined}")
    if "mexc_public_depth_handler_matches_live_typescript ... FAILED" not in combined:
        raise SystemExit(f"mutation did not fail the named depth handler test:\n{combined}")
    if "depth frame" not in combined:
        raise SystemExit(f"mutation failed for an unexpected reason:\n{combined}")
finally:
    source.write_text(original)

positive = subprocess.run(command, cwd=repo, env=os.environ.copy(), text=True, capture_output=True)
if positive.returncode != 0:
    raise SystemExit(f"restored Rust depth handler did not pass:\n{positive.stdout}{positive.stderr}")
print("Rust depth fallback mutation failed at named live-source assertion (101); restored source passes.")
