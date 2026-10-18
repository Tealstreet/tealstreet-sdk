#!/usr/bin/env python3
"""Execute fixed production faults, require named native failures, restore exactly."""
import json
import os
from pathlib import Path
import subprocess
import tempfile

WORKSPACE = Path(__file__).resolve().parents[2]
SOURCE = WORKSPACE / 'safe-cex-venue-asterdex/src'
RESULTS = WORKSPACE / 'target/asterdex-alternate-contract-results.json'
LOGS = Path(tempfile.mkdtemp(prefix='asterdex-alternate-controls-'))
ENV = {**os.environ, 'RUSTC_BOOTSTRAP': '1', 'CARGO_BUILD_JOBS': '1'}
assert len(os.sys.argv) == 1, 'alternate controls have a fixed scope'


def execute(command, label):
    with (LOGS / f'{label}.log').open('w') as log:
        result = subprocess.run(command, cwd=WORKSPACE, env=ENV,
                                stdout=log, stderr=subprocess.STDOUT)
    output = (LOGS / f'{label}.log').read_text()
    assert 'error[E' not in output, f'compiler failure is not a behavioral RED: {LOGS}'
    return result.returncode, output


def native(label, expected):
    RESULTS.unlink(missing_ok=True)
    code, output = execute(
        ['node', 'safe-cex-venue-asterdex/scripts/accepted-alternate-contracts.mjs'], label)
    assert RESULTS.exists(), (label, LOGS)
    rows = json.loads(RESULTS.read_text())
    (LOGS / f'{label}.json').write_text(json.dumps(rows, indent=2) + '\n')
    assert len(rows) == 33, (label, LOGS)
    assert sum(bool(r['classificationOnly']) for r in rows) == 8, (label, LOGS)
    failed = [r for r in rows if r['failures']]
    assert sorted(r['name'] for r in failed) == sorted(expected), (label, failed, LOGS)
    assert code == (1 if expected else 0), (label, LOGS)
    if expected:
        assert all(any('observation mismatch' in f or 'output mismatch' in f
                       for f in r['failures']) for r in failed), (label, failed, LOGS)
    print(f'{label}: {len(failed)} named native failures; {LOGS}', flush=True)


def conversion(label, red):
    code, output = execute([
        'cargo', 'test', '--locked', '-p', 'safe-cex-venue-asterdex', '--jobs', '1',
        '--test', 'exchange_trait',
        'alternate_typed_preparation_keeps_all_native_fields_and_presence', '--', '--exact',
    ], label)
    assert code == (101 if red else 0), (label, LOGS)
    assert ('1 failed' if red else '1 passed') in output, (label, LOGS)
    assert 'alternate_typed_preparation_keeps_all_native_fields_and_presence' in output
    print(f'{label}: {"RED" if red else "GREEN"}; {LOGS}', flush=True)


faults = [
    ('types.rs', 'price_precision: w.price_precision,',
     'price_precision: w.price_precision + 1.0,'),
    ('public_stream.rs', 'patch.set_number("last", price);',
     'patch.set_number("last", price + 1.0);'),
    ('order_execution.rs', 'post_only: input.post_only,', 'post_only: None,'),
]
originals = {SOURCE / file: (SOURCE / file).read_bytes() for file, _, _ in faults}
native('baseline', [])
try:
    for file, before, after in faults:
        source = SOURCE / file
        text = originals[source].decode()
        assert text.count(before) == 1, (file, before)
        source.write_text(text.replace(before, after))
    native('compiled-dto-and-stream-faults', [
        'safe_cex_venue_asterdex::types::AsterDEXMarketResponse',
        'safe_cex_venue_asterdex::public_stream::PublicStream',
    ])
    conversion('compiled-preparation-fault', True)
finally:
    for source, original in originals.items():
        source.write_bytes(original)
        assert source.read_bytes() == original
native('exact-restoration', [])
conversion('preparation-restoration', False)
print(f'Three compiled production faults detected; exact restored native/typed gates pass. {LOGS}')
