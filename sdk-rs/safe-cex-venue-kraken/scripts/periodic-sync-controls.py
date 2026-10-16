#!/usr/bin/env python3
"""One compiled production defect and restored pass per new periodic/REST row."""
from pathlib import Path
import os
import subprocess
crate = Path(__file__).resolve().parents[1]
revision = subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip()
environment = dict(os.environ, SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
controls = [
    ('periodic_sync', 'futures_tick', '500.0', '501.0', 'TICK native loop delay'),
    ('periodic_sync', 'spot_tick', '500.0', '501.0', 'TICK native loop delay'),
    ('spot_cancel', 'cancel_orders', '"txid"', '"order_id"', 'CANCEL native ID field'),
]
for module, function, old, new, assertion in controls:
    production = crate / 'src' / (module + '.rs')
    original = production.read_bytes()
    source = original.decode()
    start = source.index('pub async fn ' + function + '<')
    end = source.find('\npub async fn ', start + 1)
    if end < 0:
        end = len(source)
    body = source[start:end]
    if function == 'cancel_orders':
        assert body.count(old) == 2, 'single-order and live-iterator POST fields'
        body = body.replace(old, new, 1)
    else:
        assert body.count(old) == 1, (function, 'unique mutation')
        body = body.replace(old, new)
    command = ['cargo', 'test', '--manifest-path', str(crate.parent / 'Cargo.toml'),
               '-p', 'safe-cex-venue-kraken', '--test', module, function + '_live_source', '--jobs', '2']
    def run():
        return subprocess.run(command, env=environment, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    baseline = run()
    assert baseline.returncode == 0, baseline.stdout
    try:
        production.write_text(source[:start] + body + source[end:])
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
print(f'{revision}: 3 rows, 3 named compiled failures, 3 restored passes')
