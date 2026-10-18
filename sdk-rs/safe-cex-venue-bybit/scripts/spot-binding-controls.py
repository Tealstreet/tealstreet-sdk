#!/usr/bin/env python3
"""Compiled facade controls. Writes logs under /tmp and always restores the source."""
from pathlib import Path
import re
import subprocess

root = Path(__file__).resolve().parents[3]
source = root / "sdk-rs/safe-cex-venue-bybit/src/binding/exchange.rs"
original = source.read_text()
command = ["cargo", "test", "--locked", "--manifest-path", "sdk-rs/Cargo.toml",
           "-p", "safe-cex-venue-bybit", "--test", "spot_binding"]
controls = {
    "start": None, "get_account": None, "validate_account": None,
    "fetch_positions": None, "fetch_orders": None, "fetch_markets": None,
    "fetch_tickers": None, "place_order": None, "place_orders": None,
    "update_order": None, "cancel_orders": None,
    "set_leverage": "()", "change_position_mode": "()", "change_margin_type": "()",
    "fetch_symbol_max_leverage": "1.", "schedule_cancel": "()",
    "fetch_public_trades": "vec![]",
}

def run(label):
    result = subprocess.run(command, cwd=root, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    Path("/tmp/bybit-spot-control-" + label + ".txt").write_text(result.stdout)
    return result

baseline = run("baseline")
assert baseline.returncode == 0 and "6 passed" in baseline.stdout, baseline.stdout
try:
    for name, replacement in controls.items():
        # Only replace the public trait consumer; the untouched ported body still compiles.
        begin = original.index("impl Exchange for BybitExchange")
        match = re.search(r"    fn " + name + r"(?:<'[^>]+>)?\(", original[begin:])
        assert match, name
        start = original.index("{", begin + match.start())
        end, depth = start + 1, 1
        while depth:
            depth += (original[end] == "{") - (original[end] == "}")
            end += 1
        body = 'Box::pin(async { Err(ExchangeError::new("disabled Spot ' + name + '")) })' if replacement is None else "Box::pin(async { Ok(" + replacement + ") })"
        fn_start = begin + match.start()
        source.write_text(original[:fn_start] + "    #[allow(unused_variables)]\n" + original[fn_start:start+1] + "\n        " + body + "\n    " + original[end-1:])
        result = run(name)
        assert result.returncode == 101 and "panicked at" in result.stdout and "error[E" not in result.stdout, (name, result.stdout)
        print(name + ": compiled control caught", flush=True)
        source.write_text(original)
    flag = '                    this.define("isSpot", Js::Bool(true))?;'
    assert original.count(flag) == 1
    source.write_text(original.replace(flag, ""))
    result = run("source_is_spot")
    assert result.returncode == 101 and "Spot source flag reaches typed base" in result.stdout and "panicked at" in result.stdout and "error[E" not in result.stdout, result.stdout
    print("source_is_spot: compiled control caught", flush=True)
finally:
    source.write_text(original)
restored = run("restored")
assert restored.returncode == 0 and "6 passed" in restored.stdout, restored.stdout
assert source.read_text() == original
print("17 method + 1 identity-field controls caught; restored 6 Spot E2Es pass.", flush=True)
