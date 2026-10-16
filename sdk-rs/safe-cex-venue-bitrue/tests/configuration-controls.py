"""Sequential, reversible mutations of this venue's private configuration witness."""
import os
from pathlib import Path
import subprocess

root = Path(__file__).resolve().parents[3]
source = root / 'sdk-rs/safe-cex-venue-bitrue/src/types.rs'
original = source.read_text()
command = ['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--manifest-path',
           str(root / 'sdk-rs/Cargo.toml'), '-p', 'safe-cex-venue-bitrue',
           '--test', 'configuration', '--jobs', '1']
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH'), 'SAFE_CEX_TYPESCRIPT_PATH is required'
mutations = [
    ('INTERVAL', '("1m", "1min")', '("1m", "5min")'),
    ('ORDER_TYPE', '("LIMIT", OrderType::Limit)', '("LIMIT", OrderType::Market)'),
    ('ORDER_STATUS', '("INIT", OrderStatus::Open)', '("INIT", OrderStatus::Closed)'),
    ('ORDER_SIDE', '("BUY", OrderSide::Buy)', '("BUY", OrderSide::Sell)'),
    ('RECV_WINDOW', '20_000.0', '20_001.0'),
    ('BROKER_ID', '"Tealstreet"', '"WrongBroker"'),
    ('BASE_URL', '"https://fapi.bitrue.com"', '"https://wrong.invalid"'),
    ('BASE_WS_URL', '"public", "wss://fmarket-ws.bitrue.com/kline-api/ws"',
     '"private", "wss://fmarket-ws.bitrue.com/kline-api/ws"'),
    ('ENDPOINTS', '"ACCOUNT", "/fapi/v2/account"', '"ACCOUNT", "/fapi/v2/openOrders"'),
    ('GRAFANA_ENDPOINTS_EVERY', '[ENDPOINTS[7].1]', '[ENDPOINTS[8].1]'),
    ('GRAFANA_ENDPOINTS_PERIODIC', '[ENDPOINTS[5].1, ENDPOINTS[0].1]',
     '[ENDPOINTS[0].1, ENDPOINTS[5].1]'),
]
for row, before, after in mutations:
    assert original.count(before) == 1, row
    try:
        source.write_text(original.replace(before, after))
        result = subprocess.run(command, capture_output=True, text=True)
        output = result.stdout + result.stderr
        assertion = 'configuration parity: ' + row
        assert result.returncode != 0 and assertion in output, output
        print('FAIL PROVED:', assertion, flush=True)
    finally:
        source.write_text(original)
    result = subprocess.run(command, capture_output=True, text=True)
    assert result.returncode == 0, result.stdout + result.stderr
    print('RESTORED PASS:', row, flush=True)

# A stringified comparison could conceal this Number-to-string source drift.
ts_source = root / 'packages/safe-cex/src/exchanges/bitrue/bitrue.types.ts'
ts_original = ts_source.read_text()
assert ts_original.count('RECV_WINDOW = 20_000') == 1
try:
    ts_source.write_text(ts_original.replace('RECV_WINDOW = 20_000', "RECV_WINDOW = '20000'"))
    result = subprocess.run(command, capture_output=True, text=True)
    output = result.stdout + result.stderr
    assert result.returncode != 0 and 'RECV_WINDOW native typeof' in output, output
    print('FAIL PROVED: RECV_WINDOW native typeof', flush=True)
finally:
    ts_source.write_text(ts_original)
result = subprocess.run(command, capture_output=True, text=True)
assert result.returncode == 0, result.stdout + result.stderr
print('RESTORED PASS: RECV_WINDOW native typeof', flush=True)
