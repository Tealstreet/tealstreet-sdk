#!/usr/bin/env python3
"""Run each placement-record ingress regression with one compiled defect restored."""
import os
import re
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / "safe-cex-venue-hyperliquid/src/cloid.rs"
ORIGINAL = SOURCE.read_bytes()
TEXT = ORIGINAL.decode()
ENV = {**os.environ, "RUSTUP_TOOLCHAIN": "1.99.0", "RUSTC_BOOTSTRAP": "1"}
COMMAND = ["cargo", "+1.99.0", "test", "--locked", "-p", "safe-cex-venue-hyperliquid",
           "--jobs", "1", "--test", "placement_record_ingress"]


def run(test=None, fail=False):
    command = COMMAND + ([test, "--", "--exact"] if test else [])
    result = subprocess.run(command, cwd=ROOT, env=ENV, text=True,
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    print(result.stdout, flush=True)
    if fail:
        assert result.returncode == 101, "control must compile and fail a native test"
        assert re.search(rf"^test {re.escape(test)} \.\.\. FAILED$", result.stdout, re.M)
        assert "error[E" not in result.stdout, "compiler errors are not evidence"
    else:
        assert result.returncode == 0, "restored native suite must pass"
        if test:
            assert re.search(rf"^test {re.escape(test)} \.\.\. ok$", result.stdout, re.M)


CONTROLS = [
    ("placement_record_rejects_complete_positional_array",
     "HyperliquidPlacementRecordWire::deserialize(safe_cex_types::strict_decode::object(\n"
     "                    decoder,\n                ))",
     "HyperliquidPlacementRecordWire::deserialize(decoder)"),
    ("placement_record_optional_time_in_force_rejects_null",
     '#[serde(default, deserialize_with = "crate::spot_json::present")]',
     "#[serde(default)]"),
    ("placement_record_captures_binary64_before_serde_json",
     "let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;",
     "let buffered = serde_json::Value::deserialize(deserializer)?;\n"
     "        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(buffered)"
     ".map_err(serde::de::Error::custom)?;"),
]

try:
    run()
    for name, old, new in CONTROLS:
        assert TEXT.count(old) == 1, f"control source moved: {name}"
        print(f"CONTROL {name}", flush=True)
        SOURCE.write_text(TEXT.replace(old, new))
        try:
            run(name, fail=True)
        finally:
            SOURCE.write_bytes(ORIGINAL)
        run(name)
    run()
finally:
    SOURCE.write_bytes(ORIGINAL)
    assert SOURCE.read_bytes() == ORIGINAL

print("3 compiled native failures / 3 restored passes; final 5-test suite PASS", flush=True)
