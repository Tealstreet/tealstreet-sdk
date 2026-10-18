#!/usr/bin/env python3
"""Execute exactly the 19 reviewed record adapters; no package clearance.

--owner=<ident> --array-only runs the named complete valid positional-array
witness for a guard-removal control. Always restore the temporary Rust fixture.
"""
import json
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
CRATE = ROOT / 'safe-cex-venue-hyperliquid'
SCHEMA = json.loads((CRATE / 'tests/fixtures/accepted_nested_record_schema.json').read_text())
selected = list(SCHEMA['selected'])
assert len(selected) == 19
args = sys.argv[1:]
assert len(set(args)) == len(args)
assert all(arg == '--array-only' or arg.startswith('--owner=') for arg in args)
owners = [arg.split('=', 1)[1] for arg in args if arg.startswith('--owner=')]
assert len(owners) <= 1 and all(owner in selected for owner in owners)
array_only = '--array-only' in args
assert not array_only or owners
if owners:
    selected = owners
rows = json.loads((ROOT / 'safe-cex-parity/accepted-set/contracts/safe-cex-venue-hyperliquid.json').read_text())
fixture = CRATE / 'tests/__accepted_nested_record_observations.rs'
assert not fixture.exists(), 'refuse to replace an existing fixture'
tests = ['use safe_cex_venue_hyperliquid::types;'] if any('crate::types::' in rows['safe_cex_venue_hyperliquid::types::' + ident]['nativeObservation']['call'] for ident in selected) else []
case_count = 0
for ident in selected:
    native = rows['safe_cex_venue_hyperliquid::types::' + ident]['nativeObservation']
    cases = native['cases']
    assert len(cases) == SCHEMA['selected'][ident]['caseCount']
    if array_only:
        cases = [case for case in cases if case['input'].startswith('[')]
        assert len(cases) == 1 and cases[0]['output'] == {'ok': False}
    call = native['call'].replace('$value', 'input').replace('$type', 'safe_cex_venue_hyperliquid::types::' + ident)
    body = ['fn observe(input:&serde_json::Value)->serde_json::Value ' + call]
    for index, case in enumerate(cases):
        input_json = json.dumps(case['input'], ensure_ascii=True)
        output_json = json.dumps(case['output'], ensure_ascii=True)
        label = ident + (' full valid declaration-order array must be rejected' if array_only else ' authored case ' + str(index))
        body.append('{let input:serde_json::Value=serde_json::from_str(r#####"' + input_json + '"#####).unwrap();let expected:serde_json::Value=serde_json::from_str(r#####"' + output_json + '"#####).unwrap();assert_eq!(observe(&input),expected,"' + label + '");}')
    case_count += len(cases)
    tests.append('#[test] fn ' + ident.lower() + '_native_contract(){' + '\n'.join(body) + '}')
try:
    fixture.write_text('\n'.join(tests))
    result = subprocess.run(['cargo', '+1.99.0', 'test', '--locked', '-p', 'safe-cex-venue-hyperliquid', '--jobs', '1', '--test', '__accepted_nested_record_observations'], cwd=ROOT,
        env={**os.environ, 'RUSTC_BOOTSTRAP': '1', 'RUSTUP_TOOLCHAIN': '1.99.0', 'CARGO_BUILD_JOBS': '1'})
finally:
    fixture.unlink()
    assert not fixture.exists()
print(f'{len(selected)} contract tests; {case_count} authored cases; {case_count * 6} route observations; exit {result.returncode}', flush=True)
raise SystemExit(result.returncode)
