#!/usr/bin/env python3
"""Remove only the ten String frontdoors; require eleven native owner failures."""
import json
import os
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[3]
SOURCE = ROOT / 'sdk-rs/safe-cex-venue-asterdex/src/types.rs'
RESULTS = ROOT / 'sdk-rs/target/asterdex-enum-contract-results.json'
LOGS = Path(tempfile.mkdtemp(prefix='asterdex-enum-controls-'))
COMMAND = ['node', 'sdk-rs/safe-cex-venue-asterdex/scripts/accepted-enum-contracts.mjs']
assert len(os.sys.argv) == 1, 'enum controls take no scope-changing arguments'
STRING_OWNERS = [
    'AsterDEXAPIVersion', 'AsterDEXFapiVersion', 'AsterDEXStpMode',
    'AsterDEXPositionModeResponseDualSidePositionString',
    'AsterDEXStreamTickerE', 'AsterDEXStreamDepthE', 'AsterDEXStreamTradeE',
    'AsterDEXStreamMarkPriceE', 'AsterDEXAccountUpdateE', 'AsterDEXOrderUpdateE',
]
EXPECTED = {f'safe_cex_venue_asterdex::types::{name}' for name in STRING_OWNERS}
EXPECTED.add('safe_cex_venue_asterdex::types::AsterDEXPositionModeResponseDualSidePosition')


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
    assert {row['name'] for row in rows} == EXPECTED, f'{label}: exact native owner set; {LOGS}'
    assert all(not row.get('classificationOnly') for row in rows), LOGS
    failed = [row for row in rows if row['failures']]
    if mutant:
        assert result.returncode == 1, f'{label}: native assertion exit required; {LOGS}'
        assert {row['name'] for row in failed} == EXPECTED, f'{label}: every individual owner must fail; {LOGS}'
        for row in failed:
            assert any('native observation mismatch' in failure for failure in row['failures']), (
                f'{label}: {row["name"]} must fail a direct native observation; {LOGS}')
    else:
        assert result.returncode == 0 and not failed, f'{label}: baseline/restoration failure; {LOGS}'
    print(f'{label}: exit {result.returncode}; {len(failed)} failed native owners', flush=True)


original = SOURCE.read_bytes()
text = original.decode()
before = '''        let text = <String as serde::Deserialize>::deserialize(d)?;
        let wire =
            <Wire as serde::Deserialize>::deserialize(serde::de::value::StringDeserializer::<
                D::Error,
            >::new(text))?;'''
mutant = text
for owner in STRING_OWNERS:
    start = mutant.index(f"impl<'de> serde::Deserialize<'de> for {owner} {{")
    end = mutant.index('\n}\n', start) + 3
    region = mutant[start:end]
    assert region.count(before) == 1, owner
    mutant = mutant[:start] + region.replace(before, '        let wire = <Wire as serde::Deserialize>::deserialize(d)?;') + mutant[end:]
assert mutant != text
run('baseline')
try:
    SOURCE.write_text(mutant)
    run('ten-string-frontdoors-removed', mutant=True)
finally:
    SOURCE.write_bytes(original)
assert SOURCE.read_bytes() == original
run('exact-restoration')
print(f'Ten own enum assertions plus the dependent union catch tagged-map widening; all eleven restore PASS; logs {LOGS}')
