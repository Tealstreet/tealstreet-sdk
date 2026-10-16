#!/usr/bin/env python3
"""Remove only twenty object guards and require each complete-array assertion."""
import json
import os
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[3]
SOURCE = ROOT / 'sdk-rs/safe-cex-venue-asterdex/src/types.rs'
RESULTS = ROOT / 'sdk-rs/target/asterdex-record-contract-results.json'
LOGS = Path(tempfile.mkdtemp(prefix='asterdex-record-controls-'))
COMMAND = ['node', 'sdk-rs/safe-cex-venue-asterdex/scripts/accepted-record-contracts.mjs']
assert len(os.sys.argv) == 1, 'record controls take no scope-changing arguments'
OWNERS = [
    'AsterDEXV3AuthContext', 'AsterDEXAccountResponseAssetsItem',
    'AsterDEXAccountResponsePositionsItem', 'AsterDEXAccountResponse',
    'AsterDEXPositionModeResponse', 'AsterDEXOpenInterestResponse',
    'AsterDEXDepthResponse', 'AsterDEXTradeResponse', 'AsterDEXUserTradeResponse',
    'AsterDEXStreamTicker', 'AsterDEXStreamDepth', 'AsterDEXStreamTrade',
    'AsterDEXStreamMarkPrice', 'AsterDEXAccountUpdateABItem',
    'AsterDEXAccountUpdateAPItem', 'AsterDEXAccountUpdateA', 'AsterDEXAccountUpdate',
    'AsterDEXOrderUpdateO', 'AsterDEXOrderUpdate', 'AsterDEXListenKeyResponse',
]
EXPECTED = {f'safe_cex_venue_asterdex::types::{owner}' for owner in OWNERS}
CONTRACTS = json.loads((ROOT / 'sdk-rs/safe-cex-parity/accepted-set/contracts/safe-cex-venue-asterdex.json').read_text())


def run(label, mutant=False):
    if RESULTS.exists():
        RESULTS.unlink()
    result = subprocess.run(COMMAND, cwd=ROOT, env=os.environ,
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    output = result.stdout.decode(errors='replace')
    (LOGS / f'{label}.log').write_text(output)
    assert 'error[E' not in output, f'{label}: compiler failure is not evidence; {LOGS}'
    assert RESULTS.exists(), f'{label}: actual native receipts required; {LOGS}'
    rows = json.loads(RESULTS.read_text())
    (LOGS / f'{label}.json').write_text(json.dumps(rows, indent=2, ensure_ascii=False) + '\n')
    assert {row['name'] for row in rows} == EXPECTED, f'{label}: exact native owner set; {LOGS}'
    assert all(not row.get('classificationOnly') for row in rows), LOGS
    failed = [row for row in rows if row['failures']]
    if mutant:
        assert result.returncode == 1, f'{label}: native assertion exit required; {LOGS}'
        assert {row['name'] for row in failed} == EXPECTED, f'{label}: every owner must fail; {LOGS}'
        for row in failed:
            complete, = [case for case in CONTRACTS[row['name']]['nativeObservation']['cases']
                         if case['label'] == 'complete-serde-order-array']
            encoded = json.dumps(complete['input'], ensure_ascii=False, separators=(',', ':'))
            assert any('native observation mismatch' in failure and encoded in failure
                       for failure in row['failures']), (
                f'{label}: {row["name"]} must fail its complete valid array assertion; {LOGS}')
    else:
        assert result.returncode == 0 and not failed, f'{label}: baseline/restoration failure; {LOGS}'
    print(f'{label}: exit {result.returncode}; {len(failed)} failed native owners', flush=True)


original = SOURCE.read_bytes()
mutant = original.decode()
for owner in OWNERS:
    start = mutant.index(f"impl<'de> serde::Deserialize<'de> for {owner} {{")
    end = mutant.index('\n}\n', start) + 3
    region = mutant[start:end]
    guard = 'safe_cex_types::strict_decode::object(d)'
    assert region.count(guard) == 1, owner
    mutant = mutant[:start] + region.replace(guard, 'd') + mutant[end:]
run('baseline')
try:
    SOURCE.write_text(mutant)
    run('twenty-object-guards-removed', mutant=True)
finally:
    SOURCE.write_bytes(original)
assert SOURCE.read_bytes() == original
run('exact-restoration')
print(f'All twenty complete-array native assertions catch widening; exact restoration passes twenty; logs {LOGS}')
