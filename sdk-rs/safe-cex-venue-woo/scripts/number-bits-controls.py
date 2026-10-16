"""Compiling capture mutations must retain zero-sign and finite-payload checks."""
from pathlib import Path
import os
import subprocess

path = Path('sdk-rs/safe-cex-venue-woo/tests/support/number_bits.rs')
original = path.read_text()
marker = '    if value.is_nan() {'
cases = [
    ('zero-sign', '    if value == 0.0 { return "0000000000000000".to_owned(); }\n', 'signed zero remains bit exact'),
    ('finite-payload', '    if value.is_finite() && value != 0.0 { return "0000000000000000".to_owned(); }\n', 'non-NaN payload remains bit exact'),
]
env = dict(os.environ)
env['PATH'] = '/opt/homebrew/opt/rustup/bin:' + env['PATH']
for name, mutation, assertion in cases:
    assert original.count(marker) == 1
    try:
        path.write_text(original.replace(marker, mutation + marker))
        run = subprocess.run(['cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '--locked', '--offline', '--jobs', '1', '-p', 'safe-cex-venue-woo', '--test', 'number_bits', 'capture_canonicalizes_only_nan'], env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        Path('/tmp/woo-number-bits-control-' + name + '.log').write_text(run.stdout)
        ok = run.returncode == 101 and 'test result: FAILED' in run.stdout and assertion in run.stdout
        print(name, run.returncode, 'assertion-red' if ok else 'BAD', flush=True)
        assert ok, run.stdout
    finally:
        path.write_text(original)
