#!/usr/bin/env python3
"""Named runtime failures for the seven concrete ReadHost receiver methods."""
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile

sdk = Path(__file__).resolve().parents[2]
source = sdk / "safe-cex-venue-kraken/src/binding/host.rs"
original = source.read_bytes()
receipts = Path(tempfile.mkdtemp(prefix="kraken-read-receiver-controls-"))
cases = [
    ("keyed", "keyed_preserves_native_cells_and_key_precedence",
     "safe_cex_exchange::js_runtime::library::in_operator(value, &name)?", "value.has_own(&name)"),
    ("starts_with", "starts_with_uses_the_native_string_method", '"startsWith"', '"endsWith"'),
    ("includes", "includes_uses_the_native_string_method", 'Js::str(text)', 'Js::str(&format!("{text}__missing__"))'),
    ("current", "current_reads_the_live_exchange_and_store_slots", '"disposed" => self.this.get("isDisposed")?', '"disposed" => self.this.get("isPublicDataOnly")?'),
    ("preference", "preference_reads_real_maps_without_projecting_values", 'call_method("get",', 'call_method("has",'),
    ("read", "read_forwards_real_get_params_and_post_body_to_http", '.get("data")?', '.get("headers")?'),
    ("error_message", "error_message_retains_truthy_values_and_uses_fallback_for_falsy_values", 'if message.truthy()', 'if !message.truthy()'),
]
results = []


def run(test, label, expected_failure):
    name = f"binding::exchange::accepted_read_receivers::{test}"
    command = ["cargo", "test", "--locked", "--jobs", "1", "-p", "safe-cex-venue-kraken", "--lib", name, "--", "--exact", "--nocapture"]
    result = subprocess.run(command, cwd=sdk, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    (receipts / f"{label}.log").write_text(result.stdout)
    if expected_failure:
        assert result.returncode != 0 and f"test {name} ... FAILED" in result.stdout, result.stdout
        assert "could not compile" not in result.stdout, result.stdout
        assert "panicked at" in result.stdout, result.stdout
    else:
        assert result.returncode == 0 and f"test {name} ... ok" in result.stdout, result.stdout
    return {"command": command, "exit": result.returncode, "log": str(receipts / f"{label}.log")}


try:
    for method, test, needle, replacement in cases:
        text = original.decode()
        start = text.index(f"    fn {method}(")
        end = text.find("\n    fn ", start + 1)
        assert end != -1, method
        body = text[start:end]
        assert body.count(needle) == 1, (method, needle)
        changed = text[:start] + body.replace(needle, replacement) + text[end:]
        try:
            source.write_text(changed)
            red = run(test, f"{method}-red", True)
        finally:
            source.write_bytes(original)
        assert source.read_bytes() == original
        green = run(test, f"{method}-restored", False)
        results.append({"method": method, "red": red, "restored": green})
        (receipts / "receipts.json").write_text(json.dumps({"sourceSha256": hashlib.sha256(original).hexdigest(), "results": results}, indent=2) + "\n")
        print(f"{method}: named runtime RED, exact restoration GREEN", flush=True)
finally:
    source.write_bytes(original)
print(receipts, flush=True)
