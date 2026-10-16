#!/usr/bin/env python3
"""One discriminating production mutation per new REST row; serial scoped Cargo."""
from pathlib import Path
import json
import os
import subprocess

crate = Path(__file__).resolve().parents[1]
workspace = crate.parent
revision = subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip()
evidence = {'revision': revision, 'controls': []}
controls = [
    ('Futures.fetchOHLCV', 'futures_candles.rs', 'reads', 'futures_candle_reads::',
     'volume: parse("volume")?', 'volume: parse("open")?', 'FCANDLE native volume'),
    ('Spot.fetchBalance', 'spot_balance.rs', 'spot_balance', 'spot_balance_live_source',
     'let value = info.total * ticker.last;', 'let value = info.total + ticker.last;',
     'SBALANCE direct native balance'),
]
env = dict(os.environ, SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')

for name, filename, target, test, old, new, assertion in controls:
    production = crate / 'src' / filename
    original = production.read_bytes()
    command = ['cargo', 'test', '--manifest-path', str(workspace / 'Cargo.toml'),
               '-p', 'safe-cex-venue-kraken', '--test', target, test, '--jobs', '2']
    def run():
        return subprocess.run(command, env=env, text=True, stdout=subprocess.PIPE,
                              stderr=subprocess.STDOUT)
    baseline = run()
    assert baseline.returncode == 0, baseline.stdout
    broken = None
    try:
        text = original.decode()
        assert text.count(old) == 1, (name, 'production anchor')
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
    evidence['controls'].append({'row': name, 'command': command, 'baseline_exit': baseline.returncode,
                                 'mutation_exit': broken.returncode, 'assertion': assertion,
                                 'restored_exit': restored.returncode, 'mutation_output': broken.stdout,
                                 'restored_output': restored.stdout})
    print(f'{name}: restored exit 0', flush=True)
    (workspace / 'target' / 'kraken-rest-read-evidence.json').write_text(json.dumps(evidence, indent=2) + '\n')
print(f'{len(controls)} rows; {len(controls)} named failures; {len(controls)} restored passes; revision {revision}')
