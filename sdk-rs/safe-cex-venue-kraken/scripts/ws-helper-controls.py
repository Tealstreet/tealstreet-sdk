#!/usr/bin/env python3
"""One production mutation per newly ported public helper; restore original bytes."""
from pathlib import Path
import os
import subprocess

crate = Path(__file__).resolve().parents[1]
production = crate / 'src/ws_helpers.rs'
original = production.read_bytes()
revision = subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip()
environment = dict(os.environ, SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
controls = [
    ('product_ids_live_source', '.filter(|symbol| active_symbols.contains(symbol))',
     '.filter(|symbol| !active_symbols.contains(symbol))', 'PUBLIC-HELPER independent native product count'),
    ('timeframe_live_source', '("12h", 720.0)', '("12h", 240.0)', 'PUBLIC-HELPER native timeframe bits'),
]
for test, old, new, assertion in controls:
    command = ['cargo', 'test', '--manifest-path', str(crate.parent / 'Cargo.toml'),
               '-p', 'safe-cex-venue-kraken', '--test', 'ws_helpers', test, '--jobs', '2']
    def run():
        return subprocess.run(command, env=environment, text=True, stdout=subprocess.PIPE,
                              stderr=subprocess.STDOUT)
    baseline = run()
    assert baseline.returncode == 0, baseline.stdout
    try:
        text = original.decode()
        assert text.count(old) == 1, (test, 'production anchor')
        production.write_text(text.replace(old, new))
        broken = run()
        assert broken.returncode == 101 and 'test result: FAILED' in broken.stdout and assertion in broken.stdout, (test, broken.stdout)
        print(f'{test}: exit 101; {assertion}', flush=True)
    finally:
        if production.read_bytes() != original:
            production.write_bytes(original)
    restored = run()
    assert restored.returncode == 0, restored.stdout
    assert production.read_bytes() == original
    print(f'{test}: restored exit 0', flush=True)
print(f'{revision}: 2 rows; 2 named failures; 2 restored passes')
