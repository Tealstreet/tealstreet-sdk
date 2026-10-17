#!/usr/bin/env python3
"""Compile the old iterator-object rejection against both MEXC witnesses."""
from pathlib import Path
import os
import subprocess
import tempfile

root = Path(__file__).resolve().parents[3]
target = root / "sdk-rs/safe-cex-venue-mexc/src/exchange_binding.rs"
original = target.read_bytes()
logs = Path(tempfile.mkdtemp(prefix="mexc-iterator-controls-"))
print(f"Control logs: {logs}", flush=True)
command = ["cargo", "test", "--manifest-path", "sdk-rs/Cargo.toml", "--locked", "--jobs", "1", "-p", "safe-cex-venue-mexc", "--lib", "mexc_map_iterator_"]
before = b"    let iterator = value.iterator()?;"
after = b'''    if matches!(&value, Js::Object(_)) {
        return Err(Thrown::type_error("iterator objects are not iterable"));
    }
    let iterator = value.iterator()?;'''
assert original.count(before) == 1

def run(name):
    path = logs / f"{name}.txt"
    with path.open("w") as output:
        result = subprocess.run(command, cwd=root, env=os.environ, stdout=output, stderr=subprocess.STDOUT, timeout=600)
    return result.returncode, path.read_text()

code, output = run("baseline")
assert code == 0 and "2 passed" in output, output
try:
    target.write_bytes(original.replace(before, after, 1))
    code, output = run("reject-iterator-objects")
    assert code == 101 and "2 failed" in output and "panicked at" in output and "error[E" not in output, output
    assert "mexc_empty_map_iterator_is_iterable values" in output, output
    assert "mexc_nonempty_map_iterator_is_iterable values" in output, output
    print("Compiled iterator rejection fails empty and non-empty native Map iterator assertions.", flush=True)
finally:
    target.write_bytes(original)
code, output = run("restored")
assert code == 0 and "2 passed" in output, output
assert target.read_bytes() == original
print("Exact source restoration passes both native witnesses.", flush=True)
