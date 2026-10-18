#!/usr/bin/env python3
"""Compiled semantic controls on the actual concrete Kraken ErrorHost impl."""
from pathlib import Path
import json
import subprocess
import tempfile

sdk = Path(__file__).resolve().parents[2]
source = sdk / "safe-cex-venue-kraken/src/binding/host.rs"
original = source.read_bytes()
receipts = Path(tempfile.mkdtemp(prefix="kraken-trading-error-receiver-controls-"))
cases = [
    ("lower_method", "lower_method_calls_one_live_member_and_preserves_receiver_and_result", "if !method.is_callable()", "if !value.is_string()"),
    ("upper_method", "upper_method_calls_one_live_member_and_preserves_receiver_and_result", "if !method.is_callable()", "if !value.is_string()"),
    ("emit_and_log", "emit_and_log_reaches_real_base_console_and_event_with_context", "args.push(Js::str(context));", 'args.push(Js::str(&format!("{context} wrong")));'),
]
results = []


def run(name, label, failure):
    command = ["cargo", "test", "--locked", "--jobs", "1", "-p", "safe-cex-venue-kraken", "--lib", name, "--", "--exact", "--nocapture"]
    result = subprocess.run(command, cwd=sdk, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    (receipts / f"{label}.log").write_text(result.stdout)
    if failure:
        assert result.returncode != 0 and f"test {name} ... FAILED" in result.stdout and "panicked at" in result.stdout and "could not compile" not in result.stdout, result.stdout
    else:
        assert result.returncode == 0 and f"test {name} ... ok" in result.stdout, result.stdout
    return result.returncode


try:
    for method, test, needle, replacement in cases:
        text = original.decode()
        start = text.index(f"    fn {method}(")
        end = text.find("\n    fn ", start + 1)
        if end == -1:
            end = text.index("\n}\n", start)
        body = text[start:end]
        assert body.count(needle) == 1, (method, needle)
        name = f"binding::exchange::accepted_trading_error_receivers::{test}"
        try:
            source.write_text(text[:start] + body.replace(needle, replacement) + text[end:])
            red = run(name, f"{method}-red", True)
        finally:
            source.write_bytes(original)
        assert source.read_bytes() == original
        green = run(name, f"{method}-restored", False)
        results.append({"method": method, "test": name, "red": red, "restored": green})
        (receipts / "receipts.json").write_text(json.dumps(results, indent=2) + "\n")
        print(f"{method}: named compiled RED, exact restoration GREEN", flush=True)
finally:
    source.write_bytes(original)
print(receipts, flush=True)
