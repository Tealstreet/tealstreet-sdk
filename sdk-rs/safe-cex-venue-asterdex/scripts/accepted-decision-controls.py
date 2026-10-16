#!/usr/bin/env python3
"""Require isolated compiled decision regressions and exact passing restores."""
import json
import os
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[3]
RESULTS = ROOT / 'sdk-rs/target/asterdex-decision-contract-results.json'
LOGS = Path(tempfile.mkdtemp(prefix='asterdex-decision-controls-'))
COMMAND = ['node', 'sdk-rs/safe-cex-venue-asterdex/scripts/accepted-decision-contracts.mjs']
assert len(os.sys.argv) == 1, 'decision controls take no scope-changing arguments'
CONTROLS = [
    ('numeric-zero-sign-loss', 'account_helpers.rs',
     'value.is_finite().then_some(value)',
     'value.is_finite().then_some(if value == 0.0 { 0.0 } else { value })',
     'account_helpers::parse_finite_number'),
    ('nonpositive-event-time', 'account_helpers.rs',
     'event_time.is_finite() && event_time > 0.0',
     'event_time.is_finite() && event_time >= 0.0',
     'account_helpers::get_order_update_time'),
    ('zero-position-long', 'account_helpers.rs',
     'position_amount > 0.0', 'position_amount >= 0.0',
     'account_helpers::get_position_side'),
    ('false-string-refusal', 'exchange_helpers.rs',
     'Some(Value::String(s)) if s == "false" => Ok(false),',
     'Some(Value::String(s)) if s == "false-control" => Ok(false),',
     'exchange_helpers::parse_position_mode_response'),
    ('zero-leverage-kept', 'exchange_helpers.rs',
     'Some(value) if *value != 0.0 && !value.is_nan() => *value,',
     'Some(value) if !value.is_nan() => *value,',
     'exchange_helpers::fetch_symbol_max_leverage'),
]


def run(label, owner=None):
    if RESULTS.exists():
        RESULTS.unlink()
    result = subprocess.run(COMMAND, cwd=ROOT, env=os.environ,
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    output = result.stdout.decode(errors='replace')
    (LOGS / f'{label}.log').write_text(output)
    assert 'error[E' not in output, f'{label}: compiler error is not evidence; {LOGS}'
    assert RESULTS.exists(), f'{label}: executed native receipts required; {LOGS}'
    rows = json.loads(RESULTS.read_text())
    assert len(rows) == 8 and all(not row.get('classificationOnly') for row in rows), LOGS
    failures = [row for row in rows if row['failures']]
    if owner is None:
        assert result.returncode == 0 and not failures, f'{label}: baseline failure; {LOGS}'
    else:
        assert result.returncode == 1, f'{label}: native assertion exit required; {LOGS}'
        assert [row['name'] for row in failures] == [f'safe_cex_venue_asterdex::{owner}'], (
            f'{label}: wrong failed owners {failures}; {LOGS}')
        assert any('function output mismatch' in item for item in failures[0]['failures']), (
            f'{label}: behavioral mismatch required; {LOGS}')
    print(f'{label}: exit {result.returncode}; {len(failures)} failed native owners', flush=True)


run('baseline')
for label, filename, before, after, owner in CONTROLS:
    source = ROOT / 'sdk-rs/safe-cex-venue-asterdex/src' / filename
    original = source.read_bytes()
    text = original.decode()
    assert text.count(before) == 1, label
    try:
        source.write_text(text.replace(before, after))
        run(label, owner)
    finally:
        source.write_bytes(original)
    assert source.read_bytes() == original
    run(f'{label}-restored')
print(f'Five compiled decision faults caught; exact restores pass all eight owners; logs {LOGS}')
