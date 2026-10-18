#!/usr/bin/env python3
"""Execute the exact request-weight contract; --comma-only selects its text witness."""
import json
import os
from pathlib import Path
import subprocess
import sys
ROOT = Path(__file__).resolve().parents[2]
CRATE = ROOT / 'safe-cex-venue-hyperliquid'
assert sys.argv[1:] in ([], ['--comma-only'])
row = json.loads((ROOT / 'safe-cex-parity/accepted-set/contracts/safe-cex-venue-hyperliquid.json').read_text())['safe_cex_venue_hyperliquid::rate_limit::get_hyperliquid_rest_request_weight']
native = row['nativeObservation']
cases = native['cases']
assert len(cases) == 51
if sys.argv[1:]:
    cases = [case for case in cases if case['input']['case'] == 'comma join']
    assert len(cases) == 1
call = native['call'].replace('$type', 'safe_cex_venue_hyperliquid::rate_limit::get_hyperliquid_rest_request_weight').replace('$value', 'input')
body = ['fn observe(input:&serde_json::Value)->serde_json::Value ' + call]
for case in cases:
    inp = json.dumps(case['input'], ensure_ascii=True)
    out = json.dumps(case['output'], ensure_ascii=True)
    label = json.dumps('rate text caller: ' + case['input']['case'])
    body.append('{let input:serde_json::Value=serde_json::from_str(r#####"' + inp + '"#####).unwrap();let expected:serde_json::Value=serde_json::from_str(r#####"' + out + '"#####).unwrap();assert_eq!(observe(&input),expected,' + label + ');}')
fixture = CRATE / 'tests/__accepted_rate_weight_observations.rs'
assert not fixture.exists()
try:
    fixture.write_text('#[test] fn rate_weight_contract_native_fields(){' + '\n'.join(body) + '}')
    result = subprocess.run(['cargo', '+1.99.0', 'test', '--locked', '-p', 'safe-cex-venue-hyperliquid', '--jobs', '1', '--test', '__accepted_rate_weight_observations'], cwd=ROOT,
        env={**os.environ, 'RUSTC_BOOTSTRAP': '1', 'RUSTUP_TOOLCHAIN': '1.99.0', 'CARGO_BUILD_JOBS': '1'})
finally:
    fixture.unlink()
print(f'{len(cases)} direct native caller cases; exit {result.returncode}', flush=True)
raise SystemExit(result.returncode)
