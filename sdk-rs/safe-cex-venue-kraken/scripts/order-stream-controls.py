#!/usr/bin/env python3
"""Retained order mapper control; duplicate typed stream controls were retired."""
from pathlib import Path
import os
import subprocess
import sys

crate = Path(__file__).resolve().parents[1]
revision = subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip()
environment = dict(os.environ, CARGO_INCREMENTAL='0')
environment.setdefault('SAFE_CEX_TYPESCRIPT_PATH', str(crate.parents[1] / 'node_modules/typescript'))
controls = [
    ('orders.rs', 'fetch_orders', 'orders', 'futures_orders_live_source',
     '                filled,', '                filled: filled + 1.0,', 'ORDERS native numeric filled'),
]
originals = {name: (crate / 'src' / name).read_bytes() for name, *_ in controls}
selected = set(sys.argv[1:])
assert selected <= {row[1] for row in controls}, 'unknown control function'
controls = [row for row in controls if not selected or row[1] in selected]

for name, function, target, test, old, new, assertion in controls:
    production = crate / 'src' / name
    original = originals[name]
    text = original.decode()
    prefix = 'pub async fn ' if function == 'fetch_orders' else 'pub fn '
    start = text.index(prefix + function + '<')
    end = text.find('\npub fn ', start + 1)
    if end < 0:
        end = len(text)
    body = text[start:end]
    assert body.count(old) == 1, (function, 'unique production mutation anchor')
    mutant = text[:start] + body.replace(old, new) + text[end:]
    command = ['cargo', 'test', '--manifest-path', str(crate.parent / 'Cargo.toml'),
               '-p', 'safe-cex-venue-kraken', '--test', target, test, '--jobs', '1']
    def run():
        return subprocess.run(command, cwd=crate.parent, env=environment, text=True,
                              stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    baseline = run()
    assert baseline.returncode == 0, baseline.stdout
    try:
        production.write_text(mutant)
        broken = run()
        assert broken.returncode == 101 and 'test result: FAILED' in broken.stdout and assertion in broken.stdout, (function, broken.stdout)
        print(f'{function}: compiled exit 101; {assertion}', flush=True)
    finally:
        if production.read_bytes() != original:
            production.write_bytes(original)
    restored = run()
    assert restored.returncode == 0, restored.stdout
    assert production.read_bytes() == original
    print(f'{function}: restored exit 0', flush=True)
print(f'{revision}: {len(controls)} rows; {len(controls)} named failures; {len(controls)} restored passes')
