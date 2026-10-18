#!/usr/bin/env python3
"""Remove each reviewed enum's string admission, prove failure, restore exactly."""
import os
from pathlib import Path
import re
import subprocess

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / "safe-cex-venue-hyperliquid/src/types.rs"
ORIGINAL = SOURCE.read_bytes()
ENV = {**os.environ, "RUSTUP_TOOLCHAIN": "1.99.0", "RUSTC_BOOTSTRAP": "1", "CARGO_BUILD_JOBS": "1"}
CONTROLS = [
    ("Tif", "tif_variants_and_string_only_ingress"),
    ("Tpsl", "tpsl_variants_and_string_only_ingress"),
    ("Grouping", "grouping_variants_and_string_only_ingress"),
]


def run(name=None, *, failure=False):
    command = ["cargo", "+1.99.0", "test", "--locked", "-p", "safe-cex-venue-hyperliquid",
               "--jobs", "1", "--test", "accepted_literal_contracts"]
    if name:
        command += [name, "--", "--exact"]
    result = subprocess.run(command, cwd=ROOT, env=ENV, text=True,
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    print(result.stdout, flush=True)
    if failure:
        assert result.returncode == 101 and "could not compile" not in result.stdout
        assert re.search(rf"^test {re.escape(name)} \.\.\. FAILED$", result.stdout, re.M)
        assert "tagged object widened literal" in result.stdout
    else:
        assert result.returncode == 0
        for expected in ([name] if name else [test for _, test in CONTROLS]):
            assert re.search(rf"^test {re.escape(expected)} \.\.\. ok$", result.stdout, re.M)


try:
    run()
    for ident, name in CONTROLS:
        text = ORIGINAL.decode()
        anchor = f"impl<'de> Deserialize<'de> for {ident} {{"
        assert text.count(anchor) == 1
        start = text.index(anchor)
        end = text.index("\n}", start) + len("\n}")
        block = text[start:end]
        assert "String::deserialize(deserializer)?" in block
        replacement = anchor + f'''
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {{
        {ident}Wire::deserialize(deserializer)
    }}
}}'''
        print(f"CONTROL {ident}: remove only String admission", flush=True)
        SOURCE.write_text(text[:start] + replacement + text[end:])
        try:
            run(name, failure=True)
        finally:
            SOURCE.write_bytes(ORIGINAL)
        run(name)
    run()
finally:
    SOURCE.write_bytes(ORIGINAL)
    assert SOURCE.read_bytes() == ORIGINAL
print(f"{len(CONTROLS)} compiled named tagged-shape failures / {len(CONTROLS)} restored passes; final native suite PASS; exact source bytes restored.", flush=True)
