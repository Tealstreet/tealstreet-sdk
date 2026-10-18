#!/usr/bin/env python3
"""Compile the old String conversion, require both real bindings RED, restore GREEN."""
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[2]
SOURCE_FILE = ROOT / "safe-cex-venue-binance/src/exchange_classes.rs"
COMMAND = ["cargo", "test", "--locked", "-p", "safe-cex-venue-binance", "--features", "native",
           "--test", "json_error_binding", "--jobs", "1"]
CURRENT = '''    base.emit_and_log_error(
        safe_cex_exchange::error_reporting::ExchangeErrorInput::Raw(
            crate::private_session::native(value.clone()),
        ),'''
OLD = '''    let text = safe_cex_num::js_string(Some(value)).map_err(|e| ExchangeError::new(e.message))?;
    base.emit_and_log_error(
        safe_cex_exchange::error_reporting::ExchangeErrorInput::Message(text),'''
original = SOURCE_FILE.read_bytes()
text = original.decode()
start = text.index("fn emit_json(")
head, body = text[:start], text[start:]
assert body.count(CURRENT) == 1
try:
    SOURCE_FILE.write_text(head + body.replace(CURRENT, OLD, 1))
    with open("/tmp/binance-json-error-control-red.log", "w") as log:
        status = subprocess.run(COMMAND, cwd=ROOT, env=dict(os.environ, CARGO_BUILD_JOBS="1"), stdout=log, stderr=subprocess.STDOUT)
    output = Path("/tmp/binance-json-error-control-red.log").read_text()
    assert status.returncode == 101 and "error[E" not in output, output[-6000:]
    for family in ["futures", "pm"]:
        assert f"test {family}_leverage_error_messages_keep_raw_json_shape ... FAILED" in output, output
finally:
    SOURCE_FILE.write_bytes(original)
assert SOURCE_FILE.read_bytes() == original
with open("/tmp/binance-json-error-control-restored.log", "w") as log:
    status = subprocess.run(COMMAND + ["--", "--include-ignored"], cwd=ROOT,
                            env=dict(os.environ, CARGO_BUILD_JOBS="1"), stdout=log, stderr=subprocess.STDOUT)
output = Path("/tmp/binance-json-error-control-restored.log").read_text()
assert status.returncode == 0 and "3 passed; 0 failed" in output, output[-6000:]
print("Old String conversion: both compiled native bindings RED; exact restoration: native2 + maintained-source6 GREEN.")
