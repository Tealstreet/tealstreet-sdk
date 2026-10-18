#!/usr/bin/env python3
"""Compile one disabled production binding per required Exchange operation.

Run serially in an otherwise idle worktree. Native/source algorithm controls
remain in the existing per-family runners; these prove the concrete dispatch.
Every mutation restores the exact source, and the final target must pass.
"""
import os
import pathlib
import re
import subprocess
import sys

root = pathlib.Path(__file__).resolve().parents[3]
source = root / 'sdk-rs/safe-cex-venue-weex/src/binding/exchange.rs'
original = source.read_text()
logs = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else '/tmp/weex-binding-controls')
logs.mkdir(parents=True, exist_ok=True)
command = ['cargo', 'test', '--locked', '--manifest-path', 'sdk-rs/Cargo.toml',
           '-p', 'safe-cex-venue-weex', '--test', 'exchange_binding']
cases = [('start', 'start'), ('validate_account', 'validateAccount'),
         ('get_account', 'getAccount'), ('fetch_markets', 'fetchMarkets'),
         ('fetch_tickers', 'fetchTickers'), ('fetch_positions', 'fetchPositions'),
         ('fetch_orders', 'fetchOrders'), ('fetch_symbol_max_leverage', 'fetchSymbolMaxLeverage'),
         ('set_leverage', 'setLeverage'), ('change_position_mode', 'changePositionMode'),
         ('change_margin_type', 'changeMarginType'), ('update_order', 'updateOrder'),
         ('cancel_orders', 'cancelOrders'), ('place_order', 'placeOrder'),
         ('schedule_cancel', None), ('fetch_public_trades', 'fetchPublicTrades')]

def run(name, extra):
    with (logs / f'{name}.log').open('w') as output:
        status = subprocess.run(command + extra, cwd=root, env=os.environ,
                                stdout=output, stderr=subprocess.STDOUT).returncode
    return status, (logs / f'{name}.log').read_text()

try:
    for native, member in cases:
        if member is None:
            before = 'Err(ExchangeError::new("Not implemented"))'
            assert original.count(before) == 1
            changed = original.replace(before, 'Ok(())')
        else:
            pattern = rf'self\.(invoke|unit)\("{member}"'
            assert len(re.findall(pattern, original)) == 1, member
            changed = re.sub(pattern, lambda match: match[0].replace(f'"{member}"', '"__disabled_binding"'), original)
        source.write_text(changed)
        status, output = run(native, [f'{native}_e2e', '--', '--exact'])
        assert status == 101 and f'test {native}_e2e ... FAILED' in output and 'test result: FAILED' in output, output
        assert 'error[E' not in output, output
        print(f'{native}: compiled native witness failed as required', flush=True)
        source.write_text(original)
finally:
    source.write_text(original)
status, output = run('restored', [])
assert status == 0, output
print(f'restored: all concrete binding tests pass; logs {logs}', flush=True)
