#!/usr/bin/env python3
"""One compiling wrong-result mutation per newly ported mapping row."""
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[3]
SOURCE = ROOT / 'sdk-rs/safe-cex-venue-cryptocom/src/order_mappings.rs'
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH'), 'compiler_path_required'
COMMAND = ['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--manifest-path', str(ROOT / 'sdk-rs/Cargo.toml'),
           '-p', 'safe-cex-venue-cryptocom', '--test', 'order_mappings', '--jobs', '2', '--locked']


def run():
    result = subprocess.run(COMMAND, cwd=ROOT, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    print(result.stdout, flush=True)
    return result


def passed(result):
    assert result.returncode == 0 and '1 passed; 0 failed' in result.stdout, result.stdout


passed(run())
for name, before, after, assertion in [
    ('status', 'return OrderStatus::Closed;', 'return OrderStatus::Open;', 'mapping_native:mapOrderStatus:2'),
    ('order_type', 'return CryptocomOrderType::Market;', 'return CryptocomOrderType::Limit;', 'mapping_native:mapOrderType:0'),
    ('cryptocom_order_type', 'return OrderType::Market;', 'return OrderType::Limit;', 'mapping_native:mapCryptocomOrderType:0'),
    ('time_in_force', 'return CryptocomTimeInForce::FillOrKill;', 'return CryptocomTimeInForce::GoodTillCancel;', 'mapping_native:mapTimeInForce:2'),
    ('stop_trigger', 'return CryptocomRefPriceType::Index;', 'return CryptocomRefPriceType::Mark;', 'mapping_native:mapStopTriggerType:2'),
    ('cryptocom_time_in_force', 'return OrderTimeInForce::FillOrKill;', 'return OrderTimeInForce::GoodTillCancel;', 'mapping_native:mapCryptocomTimeInForce:2'),
]:
    original = SOURCE.read_bytes()
    try:
        text = original.decode()
        assert text.count(before) == 1, 'mapping_mutation_site'
        SOURCE.write_text(text.replace(before, after))
        result = run()
        assert result.returncode == 101 and 'test result: FAILED.' in result.stdout and assertion in result.stdout, (name, result.stdout)
        print(f'FAIL observed {name}: {assertion}; exit=101', flush=True)
    finally:
        SOURCE.write_bytes(original)
    passed(run())
    print(f'PASS restored {name}', flush=True)
print('controls: 6 failures and restored passes', flush=True)
