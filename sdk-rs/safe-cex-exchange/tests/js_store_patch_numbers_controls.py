#!/usr/bin/env python3
"""Compiled numeric projection faults with named failures and exact restoration."""
from pathlib import Path
import os
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[3]
SOURCE = ROOT / 'sdk-rs/safe-cex-exchange/src/js_runtime/store.rs'
COMMAND = [os.environ.get('CARGO', 'cargo'), 'test', '--manifest-path', str(ROOT / 'sdk-rs/Cargo.toml'),
           '-p', 'safe-cex-exchange', '--test', 'js_store_patch_numbers', '--locked', '--jobs', '1']
LOGS = Path(os.environ.get('STORE_PATCH_CONTROL_LOG_DIR') or tempfile.mkdtemp(prefix='store-patch-controls-'))
LOGS.mkdir(parents=True, exist_ok=True)
original = SOURCE.read_bytes()
FAULTS = [
    ('json-projection', 'changes.set_number(key, value);',
     'changes.set_json(key, serde_json::json!(value));'),
    ('erase-negative-zero', 'changes.set_number(key, value);',
     'changes.set_number(key, if value == 0.0 { 0.0 } else { value });'),
]

def run(label, expected):
    result = subprocess.run(COMMAND, cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    output = result.stdout.decode(errors='replace')
    (LOGS / f'{label}.log').write_text(output)
    assert result.returncode == expected, f'{label}: exit {result.returncode}; expected {expected}; {LOGS}'
    if expected:
        assert 'Finished `test`' in output and 'error[E' not in output, f'{label}: must compile'
        for name in ('update_positions_keeps_native_leverage_bits', 'all_model_patch_methods_keep_every_numeric_field'):
            assert f'test {name} ... FAILED' in output, f'{label}: named runtime assertion required: {name}'
    else:
        assert '3 passed; 0 failed' in output, f'{label}: expected all three tests'
    print(f'{label}: exit {result.returncode}', flush=True)

try:
    run('baseline', 0)
    for name, old, new in FAULTS:
        text = original.decode()
        assert text.count(old) == 1, name
        SOURCE.write_text(text.replace(old, new))
        try:
            run(name, 101)
        finally:
            SOURCE.write_bytes(original)
        run(f'{name}-restored', 0)
finally:
    SOURCE.write_bytes(original)
print(f'2 compiled faults rejected; exact restorations pass; logs {LOGS}')
