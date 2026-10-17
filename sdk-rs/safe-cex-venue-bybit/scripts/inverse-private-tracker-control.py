#!/usr/bin/env python3
"""Prove the real inverse socket retains its incremental-fill tracker."""

from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / "safe-cex-venue-bybit/src/ws_private/messages.rs"
original = SOURCE.read_bytes()
needle = b"                let fill = (self.fills.borrow_mut())("
replacement = (
    b"                drop(self.fills.replace(Box::new("
    b"safe_cex_store::incremental_fills::create_incremental_fills(500.))));\n"
    + needle
)
assert original.count(needle) == 1
command = [
    "cargo", "test", "--manifest-path", str(ROOT / "Cargo.toml"),
    "-p", "safe-cex-venue-bybit", "--test", "inverse_binding", "--locked",
    "--jobs", "1", "inverse_private_socket_delivers_native_contracts_and_fill_prices",
    "--", "--exact",
]
logs = Path(tempfile.mkdtemp(prefix="bybit-inverse-tracker-control-"))


def run(label):
    with (logs / (label + ".log")).open("w") as output:
        result = subprocess.run(command, cwd=ROOT, stdout=output,
                                stderr=subprocess.STDOUT, timeout=300)
    return result.returncode, (logs / (label + ".log")).read_text()


try:
    SOURCE.write_bytes(original.replace(needle, replacement))
    code, output = run("mutant")
    assert code == 101, output
    assert "inverse duplicate cumulative update emits no fill" in output, output
    assert "test result: FAILED" in output, output
finally:
    SOURCE.write_bytes(original)

code, output = run("restored")
assert code == 0, output
assert SOURCE.read_bytes() == original
print("PASS: compiled production tracker reset fails; exact restored bytes pass")
print(logs)
