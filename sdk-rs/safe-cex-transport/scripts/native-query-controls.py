#!/usr/bin/env python3
"""Execute two compiling query regressions serially, restoring exact bytes."""
from pathlib import Path
import os
import subprocess

SDK = Path(__file__).resolve().parents[2]
ENV = {**os.environ, "RUSTUP_TOOLCHAIN": "1.99.0", "CARGO_INCREMENTAL": "0"}
PREFIX = Path(os.environ.get("QUERY_CONTROL_LOG_PREFIX", "/tmp/native-query-control"))
BASE = ["cargo", "test", "--locked", "--jobs", "1"]


def check(label, path, old, new, command, name):
    original = path.read_bytes()
    source = original.decode()
    assert source.count(old) == 1, (label, "exact production mutation anchor")
    try:
        path.write_text(source.replace(old, new))
        log = Path(f"{PREFIX}-{label}-disconnected.log")
        with log.open("w") as output:
            result = subprocess.run(BASE + command, cwd=SDK, env=ENV, stdout=output, stderr=subprocess.STDOUT)
        text = log.read_text()
        assert result.returncode == 101 and f"test {name} ... FAILED" in text, text
        assert "could not compile" not in text, text
    finally:
        path.write_bytes(original)
    assert path.read_bytes() == original
    log = Path(f"{PREFIX}-{label}-restored.log")
    with log.open("w") as output:
        result = subprocess.run(BASE + command, cwd=SDK, env=ENV, stdout=output, stderr=subprocess.STDOUT)
    assert result.returncode == 0, log.read_text()
    print(f"{label}: compiled named failure -> exact restoration PASS", flush=True)


check("brackets", SDK / "safe-cex-transport/src/query.rs", 'b"-_.!~*\'():$,"', 'b"-_.!~*\'():$,[]"',
      ["-p", "safe-cex-transport", "--test", "axios_default_query", "default_query_matches_axios_traversal_and_encoding"],
      "default_query_matches_axios_traversal_and_encoding")
check("native-numbers", SDK / "safe-cex-exchange/src/js_runtime/client.rs", '&super::value::to_js_json(&params)?', '&to_json(&params)?.into()',
      ["-p", "safe-cex-exchange", "--features", "native", "--test", "native_default_query", "js_http_keeps_nonfinite_default_query_values_on_the_wire"],
      "js_http_keeps_nonfinite_default_query_values_on_the_wire")
