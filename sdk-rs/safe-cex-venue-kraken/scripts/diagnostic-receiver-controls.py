#!/usr/bin/env python3
"""Reintroduce each native diagnostic loss, then restore exact production bytes."""
from pathlib import Path
import json
import subprocess
import tempfile

sdk = Path(__file__).resolve().parents[2]
receipts = Path(tempfile.mkdtemp(prefix="kraken-diagnostic-receiver-controls-"))
cases = [
    ("host.rs", "read_trace_preserves_the_original_native_error_argument",
     '.console_values("trace", &[error.value.clone()]);', '.console("trace", &error.value.to_display());'),
    ("host_more.rs", "poll_console_preserves_two_arguments_and_native_error_identity",
     '.console_values("error", &[Js::str(prefix), error.value]);', '.console("error", &format!("{prefix} {}", error.value.to_display()));'),
]
results = []
for filename, test, original_text, broken_text in cases:
    source = sdk / "safe-cex-venue-kraken/src/binding" / filename
    original = source.read_bytes()
    text = original.decode()
    assert text.count(original_text) == 1
    name = f"binding::exchange::accepted_diagnostic_receivers::{test}"
    command = ["cargo", "test", "--locked", "--jobs", "1", "-p", "safe-cex-venue-kraken", "--lib", name, "--", "--exact", "--nocapture"]
    try:
        source.write_text(text.replace(original_text, broken_text))
        red = subprocess.run(command, cwd=sdk, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        (receipts / f"{filename}-red.log").write_text(red.stdout)
        assert red.returncode != 0 and f"test {name} ... FAILED" in red.stdout and "panicked at" in red.stdout and "could not compile" not in red.stdout, red.stdout
    finally:
        source.write_bytes(original)
    assert source.read_bytes() == original
    green = subprocess.run(command, cwd=sdk, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    (receipts / f"{filename}-restored.log").write_text(green.stdout)
    assert green.returncode == 0 and f"test {name} ... ok" in green.stdout, green.stdout
    results.append({"file": filename, "test": name, "red": red.returncode, "restored": green.returncode})
    (receipts / "receipts.json").write_text(json.dumps(results, indent=2) + "\n")
    print(f"{filename}: named compiled RED, exact restoration GREEN", flush=True)
print(receipts, flush=True)
