#!/usr/bin/env python3
"""One compiled production mutation and restored pass per new startup/helper row."""
from pathlib import Path
import os
import subprocess

crate = Path(__file__).resolve().parents[1]
revision = subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip()
environment = dict(os.environ, SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
controls = [
    ('startup.rs', 'start_futures', 'startup', 'futures_start_live_source',
     'SocketKind::FuturesPublic', 'SocketKind::SpotPublic', 'START native constructor kind'),
    ('startup.rs', 'start_spot', 'startup', 'spot_start_live_source',
     'SocketKind::SpotPrivate', 'SocketKind::FuturesPrivate', 'START native constructor kind'),
    ('startup.rs', 'fetch_and_set_orders', 'startup', 'fetch_and_set_orders_live_source',
     'flags.orders = true;', 'flags.orders = false;', 'SNAPSHOT native orders-loaded flag'),
    ('feed_stats.rs', 'futures_clear_feed_stats_if_idle', 'feed_stats', 'futures_feed_stats_live_source',
     'length > 0', 'length > 1', 'FEED native cleanup count'),
    ('feed_stats.rs', 'spot_clear_feed_stats_if_idle', 'feed_stats', 'spot_feed_stats_live_source',
     'length > 0', 'length > 1', 'FEED native cleanup count'),
]
originals = {name: (crate / 'src' / name).read_bytes() for name, *_ in controls}
for name, function, target, test, old, new, assertion in controls:
    production = crate / 'src' / name
    original = originals[name]
    text = original.decode()
    prefix = 'pub async fn ' if name == 'startup.rs' else 'pub fn '
    start = text.index(prefix + function + '<')
    candidates = [end for marker in ('\npub fn ', '\npub async fn ')
                  if (end := text.find(marker, start + 1)) >= 0]
    end = min(candidates) if candidates else len(text)
    body = text[start:end]
    assert body.count(old) == 1, (function, 'unique production mutation anchor')
    command = ['cargo', 'test', '--manifest-path', str(crate.parent / 'Cargo.toml'),
               '-p', 'safe-cex-venue-kraken', '--test', target, test, '--jobs', '2']
    def run():
        return subprocess.run(command, env=environment, text=True,
                              stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    baseline = run()
    assert baseline.returncode == 0, baseline.stdout
    try:
        production.write_text(text[:start] + body.replace(old, new) + text[end:])
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
print(f'{revision}: 5 rows; 5 named failures; 5 restored passes')
