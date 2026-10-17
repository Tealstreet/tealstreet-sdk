#!/usr/bin/env python3
"""Run this two-owner cohort's actual contract expressions and restore its fixture.

This supplies direct native evidence only. The ordinary accepted-set runner owns
compiler discovery, trait probes, local scopes, dependencies and target coverage.
"""
import json
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[2]
CRATE = ROOT / "safe-cex-venue-hyperliquid"
IDENTS = ['HyperliquidUserAbstractionMode', 'WsUserNonFundingLedgerUpdateType', ]
ROWS = json.loads((ROOT / "safe-cex-parity/accepted-set/contracts/safe-cex-venue-hyperliquid.json").read_text())
FIXTURE = CRATE / "tests/__accepted_literal_mode_observations.rs"
assert not FIXTURE.exists(), "refuse to replace an existing fixture"
tests = []
case_count = 0
for ident in IDENTS:
    row = ROWS[f"safe_cex_venue_hyperliquid::types::{ident}"]
    native = row["nativeObservation"]
    call = native["call"].replace("$value", "input").replace("$type", f"safe_cex_venue_hyperliquid::types::{ident}")
    body = [f"fn observe(input: &serde_json::Value) -> serde_json::Value {call}"]
    for index, case in enumerate(native["cases"]):
        input_json = json.dumps(case["input"], ensure_ascii=False)
        output_json = json.dumps(case["output"], ensure_ascii=False)
        body.append(f'''{{
            let input: serde_json::Value = serde_json::from_str(r####"{input_json}"####).unwrap();
            let expected: serde_json::Value = serde_json::from_str(r####"{output_json}"####).unwrap();
            assert_eq!(observe(&input), expected, "{ident} independently authored case {index}");
        }}''')
    case_count += len(native["cases"])
    tests.append(f"#[test] fn {ident.lower()}_contract() {{" + "\n".join(body) + "}")
try:
    FIXTURE.write_text("\n".join(tests))
    result = subprocess.run(
        ["cargo", "+1.99.0", "test", "--locked", "-p", "safe-cex-venue-hyperliquid",
         "--jobs", "1", "--test", "__accepted_literal_mode_observations"], cwd=ROOT,
        env={**os.environ, "RUSTUP_TOOLCHAIN": "1.99.0", "RUSTC_BOOTSTRAP": "1", "CARGO_BUILD_JOBS": "1"},
    )
finally:
    FIXTURE.unlink()
    assert not FIXTURE.exists()
print(f"Two direct contract tests, {case_count} authored cases, six native routes per case; exit {result.returncode}.", flush=True)
raise SystemExit(result.returncode)
