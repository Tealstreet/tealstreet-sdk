#!/usr/bin/env python3
"""One compiled field defect and restored pass per new stream handler."""
from pathlib import Path
import os
import subprocess
crate = Path(__file__).resolve().parents[1]
production = crate / 'src/stream_updates.rs'
original = production.read_bytes()
revision = subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip()
environment = dict(os.environ, SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
controls = [
    ('futures_handle_ticker', '("bid", bid)', '("ask", bid)', 'STREAM native patch key'),
    ('spot_handle_ticker', '("mark", value)', '("mark", value + 1.0)', 'STREAM native patch mark'),
    ('spot_handle_ohlc', '/ 1000.0', '/ 100.0', 'STREAM native candle timestamp'),
]
for function, old, new, assertion in controls:
    source = original.decode()
    start = source.index('pub fn ' + function + '<')
    end = source.find('\npub fn ', start + 1)
    if end < 0:
        end = len(source)
    body = source[start:end]
    assert body.count(old) == 1, (function, 'unique production mutation')
    command = ['cargo', 'test', '--manifest-path', str(crate.parent / 'Cargo.toml'),
               '-p', 'safe-cex-venue-kraken', '--test', 'stream_updates', function + '_live_source', '--jobs', '2']
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
print(f'{revision}: 3 rows, 3 named compiled failures, 3 restored passes')
