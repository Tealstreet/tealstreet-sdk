#!/usr/bin/env python3
"""Sequential compiled radix faults, named oracle failure, exact restoration."""
from pathlib import Path
import os
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[3]
SOURCE = ROOT / 'sdk-rs/safe-cex-exchange/src/js_runtime/builtins.rs'
COMMAND = [os.environ.get('CARGO', 'cargo'), 'test', '--manifest-path', str(ROOT / 'sdk-rs/Cargo.toml'),
           '-p', 'safe-cex-exchange', '--test', 'js_bigint_radix', '--locked', '--jobs', '1']
LOGS = Path(os.environ.get('BIGINT_CONTROL_LOG_DIR') or tempfile.mkdtemp(prefix='bigint-radix-controls-'))
LOGS.mkdir(parents=True, exist_ok=True)
original = SOURCE.read_bytes()
FAULTS = [
    ('decimal-only', 'n.to_str_radix(radix as u32)', 'n.to_str_radix((radix * 0. + 10.) as u32)'),
    ('round-radix', 'requested.to_number()?.trunc()', 'requested.to_number()?.round()'),
    ('reject-base-two', '!(2.0..=36.0).contains(&radix)', '!(3.0..=36.0).contains(&radix)'),
]

def run(label, expected):
    result = subprocess.run(COMMAND, cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    output = result.stdout.decode(errors='replace')
    (LOGS / f'{label}.log').write_text(output)
    assert result.returncode == expected, f'{label}: exit {result.returncode}; expected {expected}; {LOGS}'
    if expected:
        assert 'Finished `test`' in output and 'error[E' not in output, f'{label}: must compile'
        assert 'BigInt radix source case' in output, f'{label}: must fail behavioral source assertion'
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
print(f'3 compiled faults rejected; all exact restorations pass; logs {LOGS}')
