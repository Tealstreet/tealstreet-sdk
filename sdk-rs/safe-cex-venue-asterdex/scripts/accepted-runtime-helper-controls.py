#!/usr/bin/env python3
"""Compile six real-body faults; require named native mismatches and exact restores."""
import json
import os
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[3]
RESULTS = ROOT / 'sdk-rs/target/asterdex-runtime-helper-contract-results.json'
LOGS = Path(tempfile.mkdtemp(prefix='asterdex-runtime-helper-controls-'))
COMMAND = ['node', 'sdk-rs/safe-cex-venue-asterdex/scripts/accepted-runtime-helper-contracts.mjs']
assert len(os.sys.argv) == 1, 'controls take no scope-changing arguments'
CONTROLS = [
    ('option-errors-swallowed', 'api_runtime.rs', 'h.get(options, key)',
     'h.get(options, key).or_else(|_| Ok(h.undefined()))', 'api_runtime::option'),
    ('clock-wrong-method', 'api_runtime.rs',
     'invoke!(h, h.constant("asterDEXVirtualClock"), key, args)',
     'invoke!(h, h.constant("asterDEXVirtualClock"), if key == "needsSync" { "getCurrentTimeMs" } else { key }, args)',
     'api_runtime::clock'),
    ('detail-wrong-field', 'api_runtime.rs', 'vec![h.get(data, field)?, opt(h, data.clone(), "code")?]',
     'vec![h.get(data, if field == "msg" { "other" } else { field })?, opt(h, data.clone(), "code")?]',
     'api_runtime::detail'),
    ('due-interval-edge', 'exchange_runtime.rs', 'h.greater_equal(elapsed, h.number(interval))',
     'h.greater_equal(elapsed, h.number(interval + 1.0))', 'exchange_runtime::due'),
    ('update-call-lost', 'exchange_runtime.rs',
     'h.call(\n        function,\n        store,\n        vec![h.object(vec![(kind, rows), ("loaded", loaded)])],\n    )?;',
     'let _ = (function, store, rows, loaded);', 'exchange_runtime::update'),
    ('optional-nullish-bypass-lost', 'runtime_host.rs', 'if h.nullish(&value) {',
     'if false && h.nullish(&value) {', 'runtime_host::opt'),
]


def run(label, owner=None):
    if RESULTS.exists():
        RESULTS.unlink()
    result = subprocess.run(COMMAND, cwd=ROOT, env=os.environ,
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    output = result.stdout.decode(errors='replace')
    (LOGS / (label + '.log')).write_text(output)
    assert 'error[E' not in output, 'compiler error is not evidence: ' + str(LOGS)
    assert RESULTS.exists(), 'actual native receipts required: ' + str(LOGS)
    rows = json.loads(RESULTS.read_text())
    assert len(rows) == 6 and all(not r.get('classificationOnly') for r in rows), LOGS
    failures = [r for r in rows if r['failures']]
    if owner is None:
        assert result.returncode == 0 and not failures, (label, LOGS)
    else:
        assert result.returncode == 1, (label, LOGS)
        assert [r['name'] for r in failures] == ['safe_cex_venue_asterdex::' + owner], (label, failures, LOGS)
        assert any('function output mismatch' in failure for failure in failures[0]['failures']), LOGS
    print(label + ': exit ' + str(result.returncode) + '; ' + str(len(failures)) + ' failed native owners', flush=True)


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
    run(label + '-restored')
print('Six compiled production faults caught; exact restores pass all six owners. Logs: ' + str(LOGS))
