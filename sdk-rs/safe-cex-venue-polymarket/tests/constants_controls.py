"""One production mutation per constant row, sequentially, with named failures."""
from pathlib import Path
import os
import subprocess

crate = Path(__file__).resolve().parents[1]
sdk = crate.parent
source = crate / 'src/constants.rs'
original = source.read_text()
log_directory = Path(os.environ['POLYMARKET_CONTROL_LOG_DIR']) if os.environ.get('POLYMARKET_CONTROL_LOG_DIR') else None
if log_directory:
    log_directory.mkdir(parents=True, exist_ok=True)

def save_log(name, result):
    if log_directory:
        (log_directory / (name + '.log')).write_text(f'exit_code={result.returncode}\n' + result.stdout)

assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH'), 'SAFE_CEX_TYPESCRIPT_PATH is required'
command = ['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--manifest-path', str(sdk / 'Cargo.toml'), '-p', 'safe-cex-venue-polymarket', '--test', 'constants', '--locked', '--jobs', '1']
controls = [
    ('POLYMARKET_CONFIG', 'https://clob.polymarket.com', 'https://mutated.invalid'),
    ('ENDPOINTS', '("MARKETS", "/markets")', '("MARKETS", "/mutated")'),
    ('ORDER_TYPE_MAP', '("GTC", OrderType::Limit)', '("GTC", OrderType::Market)'),
    ('ORDER_SIDE_MAP', '("BUY", OrderSide::Buy)', '("BUY", OrderSide::Sell)'),
    ('ORDER_SIDE_REVERSE', '(OrderSide::Buy, "BUY")', '(OrderSide::Buy, "SELL")'),
    ('ORDER_STATUS_MAP', '("live", "open")', '("live", "closed")'),
    ('SignatureType', 'Poly1271 = 3', 'Poly1271 = 4'),
    ('EIP712_DOMAIN', 'version: "2"', 'version: "3"'),
    ('EIP712_ORDER_TYPES', 'field_type: "uint256"', 'field_type: "uint128"'),
    ('WS_MESSAGE_TYPES', '("USER", "user")', '("USER", "USER")'),
    ('WS_CONFIG', 'ping_interval: 10000.0', 'ping_interval: 9000.0'),
    ('RATE_LIMITS', 'public_rps: 10.0', 'public_rps: 9.0'),
    ('TIMEFRAME_MAP', '("1m", "1m")', '("1m", "60s")'),
]

def run():
    return subprocess.run(command, cwd=sdk.parent, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)

baseline = run()
save_log('constants-baseline', baseline)
assert baseline.returncode == 0, baseline.stdout
for name, before, after in controls:
    marker = ('pub enum ' if name == 'SignatureType' else 'pub const ') + name
    offset = original.index(marker)
    assert before in original[offset:], name + ': mutation target missing'
    changed = original[:offset] + original[offset:].replace(before, after, 1)
    try:
        source.write_text(changed)
        result = run()
        assertion = 'constant:' + name + ':'
        save_log(name + '-mutation', result)
        assert result.returncode == 101 and assertion in result.stdout, name + ': wrong failure\n' + result.stdout
        print(name + ': FAIL ' + assertion, flush=True)
    finally:
        source.write_text(original)
    restored = run()
    save_log(name + '-restored', restored)
    assert restored.returncode == 0, name + ': restore failed\n' + restored.stdout
    print(name + ': RESTORED PASS', flush=True)
print('13 production mutations rejected; 13 restorations passed', flush=True)
