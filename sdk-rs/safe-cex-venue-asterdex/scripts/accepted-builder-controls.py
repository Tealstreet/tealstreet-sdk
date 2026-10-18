#!/usr/bin/env python3
"""Require six compiled builder faults and byte-exact passing restorations."""
import json
import os
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[3]
SOURCE = ROOT / 'sdk-rs/safe-cex-venue-asterdex/src/builder.rs'
RESULTS = ROOT / 'sdk-rs/target/asterdex-builder-contract-results.json'
LOGS = Path(tempfile.mkdtemp(prefix='asterdex-builder-controls-'))
COMMAND = ['node', 'sdk-rs/safe-cex-venue-asterdex/scripts/accepted-builder-contracts.mjs']
assert len(os.sys.argv) == 1, 'builder controls take no scope-changing arguments'


def run(label, owner=None):
    if RESULTS.exists():
        RESULTS.unlink()
    result = subprocess.run(COMMAND, cwd=ROOT, env=os.environ,
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    output = result.stdout.decode(errors='replace')
    (LOGS / f'{label}.log').write_text(output)
    assert 'error[E' not in output, f'{label}: compiler error is not evidence; {LOGS}'
    assert RESULTS.exists(), f'{label}: executed native results required; {LOGS}'
    rows = json.loads(RESULTS.read_text())
    assert len(rows) == 12 and all(not row.get('classificationOnly') for row in rows), LOGS
    failures = [row for row in rows if row['failures']]
    if owner is None:
        assert result.returncode == 0 and not failures, f'{label}: baseline failure; {LOGS}'
    else:
        assert result.returncode == 1, f'{label}: compiled native assertion exit required; {LOGS}'
        expected = {owner}
        if owner.endswith('::AsterBuilderApprovalStatus'):
            expected.add('safe_cex_venue_asterdex::builder::AsterBuilderApprovalCheckResult')
        assert {row['name'] for row in failures} == expected, f'{label}: wrong failed owners {failures}; {LOGS}'
        for row in failures:
            assert any('native observation mismatch' in failure or 'function output mismatch' in failure
                       for failure in row['failures']), f'{label}: named runtime mismatch required; {LOGS}'
    print(f'{label}: exit {result.returncode}; {len(failures)} failed native owners', flush=True)


original = SOURCE.read_bytes()
text = original.decode()
controls = []
for owner in ['AsterBuilderFeeConfig', 'AsterBuilderApprovalCheckResult', 'AsterBuilderApprovalResult']:
    start = text.index(f"impl<'de> serde::Deserialize<'de> for {owner} {{")
    end = text.index('\n}\n', start) + 3
    region = text[start:end]
    needle = 'safe_cex_types::strict_decode::object(d)'
    assert region.count(needle) == 1, owner
    controls.append((f'{owner}-array-widening', owner,
                     text[:start] + region.replace(needle, 'd') + text[end:]))
start = text.index("impl<'de> serde::Deserialize<'de> for AsterBuilderApprovalStatus {")
end = text.index('\n}\n', start) + 3
region = text[start:end]
before = '''        let text = <String as serde::Deserialize>::deserialize(d)?;
        let wire =
            <Wire as serde::Deserialize>::deserialize(serde::de::value::StringDeserializer::<
                D::Error,
            >::new(text))?;'''
assert region.count(before) == 1
controls.append(('status-tagged-map-widening', 'AsterBuilderApprovalStatus',
                 text[:start] + region.replace(before, '        let wire = <Wire as serde::Deserialize>::deserialize(d)?;') + text[end:]))
assert text.count('error: w.error,') == 1
controls.append(('optional-error-loss', 'AsterBuilderApprovalCheckResult',
                 text.replace('error: w.error,', 'error: w.error.filter(|_| false),')))
assert text.count('scaled > cap') == 1
controls.append(('exact-cap-refusal', 'assert_fee_rate', text.replace('scaled > cap', 'scaled >= cap')))

run('baseline')
for label, owner, mutant in controls:
    try:
        SOURCE.write_text(mutant)
        run(label, f'safe_cex_venue_asterdex::builder::{owner}')
    finally:
        SOURCE.write_bytes(original)
    assert SOURCE.read_bytes() == original
    run(f'{label}-restored')
print(f'Six compiled builder faults caught; exact restores pass all twelve owners; logs {LOGS}')
