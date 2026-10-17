#!/usr/bin/env python3
"""Execute the selected reviewed record cohort; no package clearance.

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
args = sys.argv[1:]
cohort_flags = [arg for arg in args if arg.startswith('--cohort=')]
assert len(cohort_flags) <= 1 and all(arg == '--cohort=optional' for arg in cohort_flags)
optional = bool(cohort_flags)
args = [arg for arg in args if not arg.startswith('--cohort=')]
schema_file, expected_count = ('accepted_optional_record_schema.json', 24) if optional else ('accepted_nested_record_schema.json', 20)
SCHEMA = json.loads((CRATE / 'tests/fixtures' / schema_file).read_text())
selected = list(SCHEMA['selected'])
assert len(selected) == expected_count
assert len(set(args)) == len(args)
assert all(arg == '--array-only' or arg.startswith(('--owner=', '--null-only=', '--missing-only=')) for arg in args)
owners = [arg.split('=', 1)[1] for arg in args if arg.startswith('--owner=')]
assert len(owners) <= 1 and all(owner in selected for owner in owners)
array_only = '--array-only' in args
assert not array_only or owners
null_fields = [arg.split('=', 1)[1] for arg in args if arg.startswith('--null-only=')]
assert len(null_fields) <= 1
assert not null_fields or optional or (owners == ['SpotClearinghouseBalance'] and null_fields[0] in ['token', 'entryNtl'] and not array_only)
missing_fields = [arg.split('=', 1)[1] for arg in args if arg.startswith('--missing-only=')]
assert len(missing_fields) <= 1 and not (missing_fields and null_fields)
assert not missing_fields or (optional and owners and not array_only)
assert not (optional and null_fields) or (owners and not array_only)
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
    if null_fields:
        field = null_fields[0]
        cases = [case for case in cases if isinstance((value := json.loads(case['input'])), dict) and field in value and value[field] is None]
        if optional:
            cases = cases[-1:]
        assert len(cases) == 1 and (optional or cases[0]['output'] == {'ok': False})
    if missing_fields:
        cases = [case for case in cases if isinstance((value := json.loads(case['input'])), dict) and missing_fields[0] not in value][-1:]
        assert len(cases) == 1 and cases[0]['output'] == {'ok': False}
    call = native['call'].replace('$value', 'input').replace('$type', 'safe_cex_venue_hyperliquid::types::' + ident)
    body = ['fn observe(input:&serde_json::Value)->serde_json::Value ' + call]
    for index, case in enumerate(cases):
        input_json = json.dumps(case['input'], ensure_ascii=True)
        output_json = json.dumps(case['output'], ensure_ascii=True)
        label = ident + (' full valid declaration-order array must be rejected' if array_only else ' authored case ' + str(index))
        if null_fields:
            label = ident + ' present null ' + null_fields[0] + (' preserves native presence' if optional else ' must be rejected')
        if missing_fields:
            label = ident + ' required nullable ' + missing_fields[0] + ' omission must be rejected'
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
