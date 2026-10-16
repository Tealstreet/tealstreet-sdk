"""Sequential compiling dispatch controls; never run a compiler concurrently."""
from pathlib import Path
import re
import subprocess

root = Path(__file__).resolve().parents[3]
source = root / 'sdk-rs/safe-cex-venue-bitrue/src/binding/exchange.rs'
original = source.read_text()
command = ['cargo', 'test', '--manifest-path', str(root / 'sdk-rs/Cargo.toml'), '--locked', '-p', 'safe-cex-venue-bitrue', '--test', 'exchange_binding', 'sixteen_trait_methods_use_the_constructed_bitrue_and_scripted_transports', '--', '--exact']
methods = {
    'start':'start', 'get_account':'getAccount', 'validate_account':'validateAccount',
    'fetch_positions':'fetchPositions', 'fetch_orders':'fetchOrders', 'fetch_markets':'fetchMarkets',
    'fetch_tickers':'fetchTickers', 'fetch_symbol_max_leverage':'fetchSymbolMaxLeverage',
    'set_leverage':'setLeverage', 'change_position_mode':'changePositionMode',
    'change_margin_type':'changeMarginType', 'place_order':'placeOrder',
    'update_order':'updateOrder', 'cancel_orders':'cancelOrders',
    'schedule_cancel':'scheduleCancel', 'fetch_public_trades':'fetchPublicTrades',
}
assert len(methods) == 16
for method, target in methods.items():
    pattern = re.compile(r'(    fn ' + method + r'\b[\s\S]*?self\.(?:invoke|unit)\(")' + target + r'("[,\)])')
    changed, count = pattern.subn(lambda m: m[1] + '__missingBinding' + m[2], original)
    assert count == 1, (method, count)
    try:
        source.write_text(changed)
        result = subprocess.run(command, capture_output=True, text=True)
        output = result.stdout + result.stderr
        (Path('/tmp') / ('bitrue-binding-control-' + method + '.log')).write_text(output)
        assert result.returncode == 101 and 'BIND ' + target in output and 'could not compile' not in output, output
        print('COMPILED EXIT101 ' + method + ' -> BIND ' + target, flush=True)
    finally:
        source.write_text(original)
    result = subprocess.run(command, capture_output=True, text=True)
    assert result.returncode == 0, result.stdout + result.stderr
    print('RESTORED PASS ' + method, flush=True)
print('16/16 compiling controls rejected; 16/16 restored passes', flush=True)
