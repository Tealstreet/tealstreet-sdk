#!/usr/bin/env python3
"""Each typed Exchange dispatch must compile and fail its named production E2E.

Only dispatch coverage is claimed. Source bytes are restored in finally; logs
stay outside the repository. Run sequentially, without another Cargo process.
"""
from pathlib import Path
import subprocess
import tempfile

CRATE = Path(__file__).resolve().parents[1]
SOURCE = CRATE / 'src/binding.rs'
ROOT = CRATE.parents[1]
BASE = ['cargo', '+1.99.0', 'test', '--locked', '--manifest-path', 'sdk-rs/Cargo.toml', '-p', 'safe-cex-venue-breakoutprop', '--jobs', '1', '--test', 'binding']
INHERITED = 'inherited_members_reject_exactly_without_a_request'
CONTROLS = [
 ('start','start','start_loads_the_store_and_uses_polling_without_a_socket'),
 ('get_account','getAccount',INHERITED),
 ('validate_account','validateAccount','validate_account_logs_in_through_the_production_api'),
 ('fetch_positions','fetchPositions','fetch_positions_is_a_pure_portfolio_read'),
 ('fetch_orders','fetchOrders','fetch_orders_is_a_pure_portfolio_read'),
 ('fetch_markets','fetchMarkets','fetch_markets_reaches_both_instrument_endpoints'),
 ('fetch_tickers','fetchTickers','fetch_tickers_reads_quotes'),
 ('fetch_symbol_max_leverage','fetchSymbolMaxLeverage',INHERITED),
 ('set_leverage','setLeverage',INHERITED),
 ('change_position_mode','changePositionMode',INHERITED),
 ('change_margin_type','changeMarginType',INHERITED),
 ('place_order','placeOrder','place_order_uses_source_steps_and_returns_the_native_id_list'),
 ('update_order','updateOrder','update_order_cancels_before_replacement'),
 ('cancel_orders','cancelOrders','cancel_orders_sends_the_source_delete'),
 ('schedule_cancel','scheduleCancel',INHERITED),
 ('fetch_public_trades','fetchPublicTrades',INHERITED),
]

def main():
 original = SOURCE.read_bytes()
 text = original.decode()
 logs = Path(tempfile.mkdtemp(prefix='breakout-binding-controls-'))
 print(f'logs: {logs}', flush=True)
 def run(label, test=None):
  result = subprocess.run(BASE + ([test, '--', '--exact'] if test else []), cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
  (logs / f'{label}.log').write_text(result.stdout)
  print(f'{label}: exit {result.returncode}', flush=True)
  return result
 baseline = run('baseline')
 assert baseline.returncode == 0 and '14 passed; 0 failed' in baseline.stdout, baseline.stdout
 try:
  for method, target, test in CONTROLS:
   start = text.index(f'    fn {method}(', text.index('impl Exchange for BreakoutpropExchange'))
   following = text.find('\n    fn ', start + 1)
   end = following if following != -1 else len(text)
   body = text[start:end]
   old = f'"{target}"'
   assert body.count(old) == 1, (method, body)
   SOURCE.write_text(text[:start] + body.replace(old, f'"CONTROL_{target}"') + text[end:])
   result = run(method, test)
   assert result.returncode == 101 and 'error[E' not in result.stdout, (method,result.stdout)
   assert f'test {test} ... FAILED' in result.stdout and '0 passed; 1 failed' in result.stdout, (method,result.stdout)
   SOURCE.write_bytes(original)
 finally:
  SOURCE.write_bytes(original)
 restored = run('restored')
 assert restored.returncode == 0 and '14 passed; 0 failed' in restored.stdout, restored.stdout
 assert SOURCE.read_bytes() == original
 print('16/16 compiling dispatch controls failed named tests; restored 14/14 pass',flush=True)

if __name__ == '__main__':
 main()
