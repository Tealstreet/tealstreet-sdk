#!/usr/bin/env python3
"""One compiled defect and restored pass per new subscription/dispatch row."""
from pathlib import Path
import os
import subprocess
crate = Path(__file__).resolve().parents[1]
revision = subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip()
environment = dict(os.environ, SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
controls = [
    ('public_subscriptions', 'futures_on_open', 'feed: "ticker"', 'feed: "book"', 'OPEN native feed'),
    ('public_subscriptions', 'spot_on_open', 'depth: 100.0', 'depth: 101.0', 'OPEN native book depth'),
    ('spot_dispatch', 'handle_book', 'if host.is_snapshot(message)?', 'if !host.is_snapshot(message)?', 'DISPATCH native method'),
    ('spot_dispatch', 'handle_balances', '.is_none()', '.is_some()', 'DISPATCH native event count'),
]
for module, function, old, new, assertion in controls:
    production = crate / 'src' / (module + '.rs')
    original = production.read_bytes()
    source = original.decode()
    start = source.index('pub fn ' + function + '<')
    end = source.find('\npub fn ', start + 1)
    if end < 0:
        end = len(source)
    body = source[start:end]
    assert body.count(old) == 1, (function, 'unique production mutation')
    command = ['cargo', 'test', '--manifest-path', str(crate.parent / 'Cargo.toml'),
               '-p', 'safe-cex-venue-kraken', '--test', module, function + '_live_source', '--jobs', '2']
    def run():
        return subprocess.run(command, env=environment, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    baseline = run()
    assert baseline.returncode == 0, baseline.stdout
    try:
        production.write_text(source[:start] + body.replace(old, new) + source[end:])
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
print(f'{revision}: 4 rows, 4 named compiled failures, 4 restored passes')
