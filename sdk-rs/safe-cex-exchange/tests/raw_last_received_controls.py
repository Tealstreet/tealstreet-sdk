#!/usr/bin/env python3
"""Require runtime failures for raw socket timestamp storage regressions."""
from pathlib import Path
import os
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[3]
SOURCE = ROOT / "sdk-rs/safe-cex-exchange/src/websocket.rs"
LOGS = Path(tempfile.mkdtemp(prefix="raw-last-received-controls-"))
COMMAND = [
    os.environ.get("CARGO", "cargo"), "test", "--manifest-path",
    str(ROOT / "sdk-rs/Cargo.toml"), "-p", "safe-cex-exchange",
    "--lib", "raw_last_received", "--locked", "--jobs", "2",
]
ORIGINAL = SOURCE.read_bytes()
ASSIGNMENT = b"        self.state.borrow_mut().last_received = value;\n"
FAULTS = [
    ("omit-assignment", b"        let _ = value;\n", [
        "raw_last_received_preserves_native_bits",
        "raw_last_received_drives_health_timestamp_and_reset",
    ]),
    ("erase-negative-zero", b"        self.state.borrow_mut().last_received = if value == 0.0 { 0.0 } else { value };\n", [
        "raw_last_received_preserves_native_bits",
    ]),
]


def run(label, failures=()):
    # A timeout raises through the finally blocks, restoring exact source bytes.
    result = subprocess.run(COMMAND, cwd=ROOT, capture_output=True, timeout=600)
    output = (result.stdout + result.stderr).decode(errors="replace")
    log = LOGS / f"{label}.txt"
    log.write_text(output)
    assert "Finished `test`" in output and "could not compile" not in output, str(log)
    assert result.returncode == (101 if failures else 0), str(log)
    if failures:
        for name in failures:
            assert f"test websocket::tests::{name} ... FAILED" in output, str(log)
        assert "panicked at" in output, str(log)
    else:
        assert "2 passed; 0 failed" in output, str(log)
    print(f"{label}: compiled {'runtime failure caught' if failures else '2 tests pass'}", flush=True)


try:
    assert ORIGINAL.count(ASSIGNMENT) == 1
    run("baseline")
    for label, replacement, failures in FAULTS:
        SOURCE.write_bytes(ORIGINAL.replace(ASSIGNMENT, replacement))
        try:
            run(label, failures)
        finally:
            SOURCE.write_bytes(ORIGINAL)
        run(f"{label}-restored")
finally:
    SOURCE.write_bytes(ORIGINAL)
assert SOURCE.read_bytes() == ORIGINAL
print(f"Two compiled faults caught; exact restorations passed; logs: {LOGS}")
