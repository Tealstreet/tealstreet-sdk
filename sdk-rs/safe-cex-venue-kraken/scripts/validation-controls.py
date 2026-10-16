#!/usr/bin/env python3
"""Prove one named native assertion discriminates for each account-validation row."""
from pathlib import Path
import os
import subprocess

crate = Path(__file__).resolve().parents[1]
production = crate / 'src/validation.rs'
original = production.read_bytes()
revision = subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip()
environment = dict(os.environ, SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
controls = [
    ('futures', 'futures_validation_live_source', 'Account validation failed',
     'Mutated account validation failure', 'VALIDATION futures native error'),
    ('spot', 'spot_validation_live_source',
     'Invalid nonce. Please ensure your system time is synchronized.',
     'Mutated nonce message', 'VALIDATION spot native error'),
]

for name, test, old, new, assertion in controls:
    command = ['cargo', 'test', '--manifest-path', str(crate.parent / 'Cargo.toml'),
               '-p', 'safe-cex-venue-kraken', '--test', 'validation', test, '--jobs', '2']
    def run():
        return subprocess.run(command, env=environment, text=True,
                              stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    baseline = run()
    assert baseline.returncode == 0, baseline.stdout
    try:
        text = original.decode()
        assert text.count(old) == 1, (name, 'production mutation anchor')
        production.write_text(text.replace(old, new))
        broken = run()
        assert broken.returncode == 101 and 'test result: FAILED' in broken.stdout and assertion in broken.stdout, (name, broken.stdout)
        print(f'{name}: exit 101; {assertion}', flush=True)
    finally:
        if production.read_bytes() != original:
            production.write_bytes(original)
    restored = run()
    assert restored.returncode == 0, restored.stdout
    assert production.read_bytes() == original
    print(f'{name}: restored exit 0', flush=True)
print(f'{revision}: 2 rows; 2 named failures; 2 restored passes')
