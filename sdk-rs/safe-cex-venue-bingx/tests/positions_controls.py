#!/usr/bin/env python3
"""One compiled mutation per new BingX account REST family."""
from pathlib import Path
import os
import subprocess

crate = Path(__file__).resolve().parents[1]
sdk = crate.parent
source = crate / 'src/data_reads.rs'
original = source.read_text()
env = os.environ.copy()
env['SAFE_CEX_TYPESCRIPT_PATH'] = '/Users/sam/cs/tealstreet-next/node_modules/typescript'
command = ['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--manifest-path', str(sdk / 'Cargo.toml'),
           '-p', 'safe-cex-venue-bingx', '--test', 'positions', '--jobs', '1', '--locked',
           'positions_match_actual_typescript', '--', '--exact']
controls = [
    ('positions', 'let notional = contracts.abs() * entry_price + upnl;', 'let notional = contracts.abs() * entry_price - upnl;', 'BINGX-POS native fetchOpenPositions variant 0'),
    ('metadata', 'leverage: leverage.sell_leverage,', 'leverage: leverage.buy_leverage,', 'BINGX-POS native fetchPositions variant 0'),
    ('configuration', 'market.limits.leverage.max,', 'market.limits.leverage.min,', 'BINGX-POS native request order/config setLeverage variant 0'),
    ('cancellation', 'code == Some(0.0) || code == Some(80016.0)', 'code == Some(0.0)', 'BINGX-POS native Store effects cancelOrders variant 0'),
]
for name, before, after, assertion in controls:
    assert original.count(before) == 1, f'{name}: unique production operation required'
    try:
        source.write_text(original.replace(before, after))
        broken = subprocess.run(command, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        (sdk / 'target' / f'bingx-positions-{name}-broken.log').write_text(broken.stdout)
        assert broken.returncode == 101 and assertion in broken.stdout, f'FINDING {name}: exit {broken.returncode}, expected named assertion missing\n{broken.stdout}'
        print(f'{name}: compiled mutation exit 101 at {assertion}', flush=True)
    finally:
        source.write_text(original)
    restored = subprocess.run(command, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    (sdk / 'target' / f'bingx-positions-{name}-restored.log').write_text(restored.stdout)
    assert restored.returncode == 0, f'{name}: restoration exit {restored.returncode}\n{restored.stdout}'
    print(f'{name}: restoration exit 0', flush=True)
assert source.read_text() == original, 'production bytes restored'
