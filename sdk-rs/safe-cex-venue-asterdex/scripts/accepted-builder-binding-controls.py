#!/usr/bin/env python3
"""Require actual native behavioral failures and exact passing restorations."""
import json
import os
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[3]
RESULTS = ROOT / 'sdk-rs/target/asterdex-builder-binding-results.json'
LOGS = Path(tempfile.mkdtemp(prefix='asterdex-builder-binding-controls-'))
COMMAND = ['node', 'sdk-rs/safe-cex-venue-asterdex/scripts/accepted-builder-binding-contracts.mjs']
assert len(os.sys.argv) == 1, 'builder/binding controls take no scope-changing arguments'
CONTROLS = [
    ('optional-json-loss', 'binding/members.rs',
     'to_typed(&value).map(Some)',
     'to_typed(&from_json(&to_json(&value)?)).map(Some)',
     ['binding::members::optional']),
    ('no-wallet-success', 'builder_approval.rs',
     '''fn no_wallet() -> AsterBuilderApprovalResult {
    result(
        false,''',
     '''fn no_wallet() -> AsterBuilderApprovalResult {
    result(
        true,''',
     ['builder_approval::switch_to_aster_signature_chain',
      'builder_approval::approve_aster_builder_fee_with_browser_wallet']),
]


def run(label, expected=None):
    RESULTS.unlink(missing_ok=True)
    result = subprocess.run(COMMAND, cwd=ROOT, env=os.environ,
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    output = result.stdout.decode(errors='replace')
    (LOGS / f'{label}.log').write_text(output)
    assert 'error[E' not in output, f'{label}: compiler error is not evidence; {LOGS}'
    assert RESULTS.exists(), f'{label}: actual native receipts required; {LOGS}'
    rows = json.loads(RESULTS.read_text())
    (LOGS / f'{label}.json').write_text(json.dumps(rows, indent=2) + '\n')
    assert len(rows) == 19, LOGS
    assert [r['name'] for r in rows if r['classificationOnly']] == [
        'safe_cex_venue_asterdex::builder_approval::no_wallet'], LOGS
    failures = [r for r in rows if r['failures']]
    if expected is None:
        assert result.returncode == 0 and not failures, f'{label}: restore must pass; {LOGS}'
    else:
        assert result.returncode == 1, f'{label}: native assertion exit required; {LOGS}'
        assert sorted(r['name'] for r in failures) == sorted(
            f'safe_cex_venue_asterdex::{owner}' for owner in expected), (label, failures, LOGS)
        assert all(any('function output mismatch' in f for f in r['failures'])
                   for r in failures), (label, failures, LOGS)
    print(f'{label}: {len(failures)} failed native owners; receipts {LOGS}', flush=True)


for label, filename, before, after, expected in CONTROLS:
    source = ROOT / 'sdk-rs/safe-cex-venue-asterdex/src' / filename
    original = source.read_bytes()
    text = original.decode()
    assert text.count(before) == 1, label
    try:
        source.write_text(text.replace(before, after))
        run(label, expected)
    finally:
        source.write_bytes(original)
    assert source.read_bytes() == original
    run(f'{label}-restored')
print(f'Two isolated compiled faults caught; exact restores pass all nineteen owners; {LOGS}')
