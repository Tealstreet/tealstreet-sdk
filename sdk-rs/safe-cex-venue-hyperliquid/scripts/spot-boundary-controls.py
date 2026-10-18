#!/usr/bin/env python3
"""Compile old Store/candle bridges and run the wrong source-import control.

These controls preserve the oracle source count and restore exact source bytes.
The wrong omitUndefined stub must fail the native result assertion, not a hash
or cardinality guard. Logs remain outside the repository.
"""
from pathlib import Path
import subprocess
import tempfile
import sys

CRATE = Path(__file__).resolve().parents[1]
ROOT = CRATE.parents[1]
CARGO = ["cargo", "+1.99.0", "test", "--locked", "--manifest-path", "sdk-rs/Cargo.toml",
         "-p", "safe-cex-venue-hyperliquid"]
CONTROLS = [
    ("native-spot-flag", "src/spot_binding.rs", "spot_binding",
     "bound_start_loads_spot_markets_balance_and_both_sockets",
     "base.is_spot = true;", "base.is_spot = false;"),
    ("json-store-update", "src/realm/facade_store.rs", "spot_binding",
     "bound_start_loads_spot_markets_balance_and_both_sockets",
     "let changes: StoreUpdate = r.to_typed(&arg(a, 0))?;",
     'let changes: StoreUpdate = serde_json::from_value(r.to_serde(&arg(a, 0))?).map_err(|error| r.type_error(&error.to_string()))?;'),
    ("array-candle-binding", "src/realm/host_core.rs", "spot_binding",
     "bound_ohlcv_reads_candles_with_source_interval_and_times",
     'let values = self.object_pattern(v, &["t", "o", "h", "l", "c", "v"])?;',
     'let values = self.array_slots(v, 6)?;'),
    ("wrong-omitUndefined-stub", "tests/spot_runtime_oracle.cjs", "spot_runtime",
     "place_native_signing_inputs",
     "'../../utils/omitUndefined':run(source('packages/safe-cex/src/utils/omitUndefined.ts'),{'lodash/omitBy':{__esModule:true,default:require('lodash/omitBy')}})",
     "'../../utils/omitUndefined':Object.assign(run(source('packages/safe-cex/src/utils/omitUndefined.ts'),{'lodash/omitBy':{__esModule:true,default:require('lodash/omitBy')}}),{omitUndefined:a=>a.filter(v=>v!==undefined)})"),
]

def main():
    logs = Path(tempfile.mkdtemp(prefix="hyperliquid-spot-boundary-controls-"))
    print(f"logs: {logs}", flush=True)
    executed = 0
    for label, file, binary, test, old, new in CONTROLS:
        if len(sys.argv) > 1 and label not in sys.argv[1:]:
            continue
        executed += 1
        source = CRATE / file
        original = source.read_bytes()
        text = original.decode()
        assert text.count(old) == 1, (label, text.count(old))
        try:
            source.write_text(text.replace(old, new))
            result = subprocess.run(CARGO + ["--test", binary, test, "--", "--exact"],
                                    cwd=ROOT, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
            (logs / f"{label}.log").write_text(result.stdout)
            assert result.returncode == 101 and "error[E" not in result.stdout, result.stdout
            assert f"test {test} ... FAILED" in result.stdout, result.stdout
            assert "0 passed; 1 failed" in result.stdout, result.stdout
            if label == "wrong-omitUndefined-stub":
                assert "RUNTIME source file floor" not in result.stdout, result.stdout
                assert "RUNTIME independent case floor" not in result.stdout, result.stdout
            print(f"{label}: compiling named test failed", flush=True)
        finally:
            source.write_bytes(original)
        assert source.read_bytes() == original
    assert executed > 0, "select a known control label"
    result = subprocess.run(CARGO + ["--test", "spot_binding", "--test", "spot_runtime"],
                            cwd=ROOT, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    (logs / "restored.log").write_text(result.stdout)
    assert result.returncode == 0, result.stdout
    assert "21 passed; 0 failed" in result.stdout and "11 passed; 0 failed" in result.stdout, result.stdout
    print(f"{executed}/{executed} boundary controls failed; restored Spot21 + runtime11 pass", flush=True)

if __name__ == "__main__":
    main()
