#!/usr/bin/env python3
"""Sequential compiled faults of the shared throttle, with exact restoration."""
from pathlib import Path
import os
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[3]
SOURCE = ROOT / 'sdk-rs/safe-cex-exchange/src/js_runtime/throttle.rs'
CARGO = os.environ.get('CARGO', 'cargo')
COMMAND = [CARGO, 'test', '--manifest-path', str(ROOT / 'sdk-rs/Cargo.toml'),
           '-p', 'safe-cex-exchange', '--test', 'js_throttle', '--locked', '--jobs', '1']
log_dir = os.environ.get('THROTTLE_CONTROL_LOG_DIR')
LOGS = Path(log_dir) if log_dir else Path(tempfile.mkdtemp(prefix='throttle-controls-'))
LOGS.mkdir(parents=True, exist_ok=True)
original = SOURCE.read_text()
FAULTS = [
    ('leading', '        leading,\n', '        leading: !leading,\n'),
    ('trailing', '        trailing,\n', '        trailing: !trailing,\n'),
    ('max-wait', 'max_wait: wait.max(0.),', 'max_wait: wait.max(0.) * 2.,'),
]

def run(label, expected, command=COMMAND, witness='throttle source case'):
    result = subprocess.run(command, cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    output = result.stdout.decode(errors='replace')
    (LOGS / f'{label}.log').write_text(output)
    assert result.returncode == expected, f'{label}: exit {result.returncode}, expected {expected}; {LOGS}'
    if expected:
        assert 'Finished `test`' in output and 'error[E' not in output, f'{label}: must compile'
        assert witness in output, f'{label}: must fail the named behavioral witness'
    print(f'{label}: exit {result.returncode}', flush=True)

try:
    run('baseline', 0)
    for name, old, new in FAULTS:
        assert original.count(old) == 1, name
        SOURCE.write_text(original.replace(old, new))
        try:
            run(name, 101)
        finally:
            SOURCE.write_text(original)
        run(f'{name}-restored', 0)
    library = SOURCE.with_name('library.rs')
    installed = library.read_text()
    old = '        "throttle",\n        scoped("throttle",'
    assert installed.count(old) == 1
    consumer = [CARGO, 'test', '--manifest-path', str(ROOT / 'sdk-rs/Cargo.toml'),
                '-p', 'safe-cex-venue-polymarket', '--test', 'exchange_binding',
                '--locked', '--jobs', '1']
    try:
        library.write_text(installed.replace(old, '        "missingThrottle",\n        scoped("throttle",'))
        run('consumer-wiring', 101, consumer, 'throttle is not defined')
    finally:
        library.write_text(installed)
    run('consumer-wiring-restored', 0, consumer)
finally:
    SOURCE.write_text(original)
print(f'3 scheduler faults + consumer wiring fault rejected; exact restorations pass; logs {LOGS}')
