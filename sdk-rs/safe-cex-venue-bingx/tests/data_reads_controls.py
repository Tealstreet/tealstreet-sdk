#!/usr/bin/env python3
"""One compiled production mutation per new BingX read family."""
from pathlib import Path
import os
import subprocess

crate = Path(__file__).resolve().parents[1]
source = crate / 'src/data_reads.rs'
sdk = crate.parent
original = source.read_text()
env = os.environ.copy()
env['SAFE_CEX_TYPESCRIPT_PATH'] = '/Users/sam/cs/tealstreet-next/node_modules/typescript'
command = ['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--manifest-path', str(sdk / 'Cargo.toml'),
           '-p', 'safe-cex-venue-bingx', '--test', 'data_reads', '--jobs', '1', '--locked',
           'data_reads_match_actual_typescript', '--', '--exact']
controls = [
    ('balance', 'free: float(data.get("availableMargin"))?', 'free: float(data.get("usedMargin"))?', 'BINGX-DATA native fetchBalance variant 0'),
    ('markets', 'let price = precision("pricePrecision")?', 'let price = precision("quantityPrecision")?', 'BINGX-DATA native fetchMarkets variant 0'),
    ('tickers', 'mark: float(alias(price, "markPrice")?)?', 'mark: float(alias(price, "indexPrice")?)?', 'BINGX-DATA native formatTickers variant 0'),
    ('leverage-margin', 'buy_leverage: strict_number(field(data, "longLeverage")?, "buy leverage")?', 'buy_leverage: strict_number(field(data, "shortLeverage")?, "buy leverage")?', 'BINGX-DATA native fetchLeverageForSymbol variant 0'),
    ('position-mode', 'self.account_position_mode.set(Some(mode));', 'self.account_position_mode.set(Some(PositionMode::OneWay));', 'BINGX-DATA native getOrFetchPositionMode variant 0'),
]
for name, before, after, assertion in controls:
    assert original.count(before) == 1, f'{name}: expected exactly one production operand'
    try:
        source.write_text(original.replace(before, after))
        result = subprocess.run(command, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        (sdk / 'target' / f'bingx-data-{name}-broken.log').write_text(result.stdout)
        assert result.returncode == 101 and assertion in result.stdout, f'FINDING {name}: exit {result.returncode}, named assertion missing\n{result.stdout}'
        print(f'{name}: compiled mutation exit 101 at {assertion}', flush=True)
    finally:
        source.write_text(original)
    restored = subprocess.run(command, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    (sdk / 'target' / f'bingx-data-{name}-restored.log').write_text(restored.stdout)
    assert restored.returncode == 0, f'{name}: restoration exit {restored.returncode}\n{restored.stdout}'
    print(f'{name}: restoration exit 0', flush=True)
assert source.read_text() == original, 'production bytes restored'
