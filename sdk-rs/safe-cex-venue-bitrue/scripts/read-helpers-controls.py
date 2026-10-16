"""Sequential production mutations: one corpus process per scoped test run."""
from pathlib import Path
import os
import subprocess
import sys

root = Path(__file__).resolve().parents[3]
source = root / 'sdk-rs/safe-cex-venue-bitrue/src/read_helpers.rs'
original = source.read_text()
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH')
command = ['/opt/homebrew/opt/rustup/bin/cargo', 'test', '--manifest-path', str(root / 'sdk-rs/Cargo.toml'), '-p', 'safe-cex-venue-bitrue', '--test', 'read_helpers', '--locked', '--jobs', '1']
controls = [
    ('isSuccessCode', 'code == 200.0', 'code == 201.0'),
    ('normalizeTimestampMs', 'ts > 1_000_000_000_000.0', 'ts >= 1_000_000_000_000.0'),
    ('normalizeTradeSymbol', "value.contains('_')", "value.contains('-')"),
    ('pickTradeOrderId', 'if side == "BUY" { "bidId" } else { "askId" }', 'if side == "BUY" { "askId" } else { "bidId" }'),
    ('resolveTradeQty', '.get(symbol)', '.get(if symbol.is_empty() { symbol } else { "E-BTC-USDT" })'),
    ('resolveTradeQty', 'let scaled_qty = raw_qty * multiplier;', 'let scaled_qty = raw_qty / multiplier;'),
]
for name, old, new in controls:
    if len(sys.argv) > 1 and name not in sys.argv[1:]:
        continue
    assert original.count(old) == 1, old
    try:
        source.write_text(original.replace(old, new))
        result = subprocess.run(command, capture_output=True, text=True)
        output = result.stdout + result.stderr
        assertion = 'read helper native parity: ' + name
        assert result.returncode not in (0,126) and assertion in output, output
        print('FAIL PROVED: ' + assertion, flush=True)
    finally:
        source.write_text(original)
    result = subprocess.run(command, capture_output=True, text=True)
    assert result.returncode == 0, result.stdout + result.stderr
    print('RESTORED PASS: ' + name, flush=True)
