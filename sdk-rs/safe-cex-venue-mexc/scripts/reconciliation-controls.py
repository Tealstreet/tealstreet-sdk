"""Substitute plausible dispatch, snapshot, and request defects independently."""
from pathlib import Path
import os
import subprocess
crate = Path(__file__).resolve().parents[1]
source = crate / 'src/exchange.rs'
original = source.read_text()
prefix = ['cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '--locked', '-p', 'safe-cex-venue-mexc', '--lib', '--jobs', '2', '--', '--exact']
controls = [
 ('error message coercion swallowed', 'Some(message) => safe_cex_num::js_string(Some(&message)).map_err(|e| {\n            let mut error = ExchangeError::new(e.message);\n            error.name = e.name.into();\n            error\n        })', 'Some(message) => Ok(safe_cex_num::js_string(Some(&message)).unwrap_or_else(|_| error.message.clone()))', 'mexc_fetch_orders_live_requests_and_results'),
 ('override base bypass', 'self.reconcile_orders(symbols.clone(), self.fetch_orders(symbols), false)', 'Box::pin(async move { self.base.fetch_and_set_orders(self, symbols).await })', 'mexc_orders_override_reaches_concrete_reconciliation'),
 ('stale disposal capture', 'let orders = fetch.await?;\n            if self.base.is_disposed()', 'let disposed = self.base.is_disposed();\n            let orders = fetch.await?;\n            if disposed', 'mexc_reconciliation_live_store_transitions'),
 ('regular snapshot fetched-first', 'unique_orders(stops.into_iter().chain(preserved).chain(fetched))', 'unique_orders(fetched.into_iter().chain(stops).chain(preserved))', 'mexc_reconciliation_live_store_transitions'),
 ('placement release on wrong snapshot', 'if !regular {', 'if regular {', 'mexc_reconciliation_live_store_transitions'),
 ('lookback ninety days', 'now - 89.0*24.0*60.0*60.0*1000.0', 'now - 90.0*24.0*60.0*60.0*1000.0', 'mexc_fetch_orders_live_requests_and_results'),
 ('dedupe by symbol', 'seen.insert(o.id.clone())', 'seen.insert(o.symbol.clone())', 'mexc_fetch_orders_live_requests_and_results'),
]
try:
 for name,before,after,test in controls:
  assert original.count(before)==1,(name,original.count(before))
  source.write_text(original.replace(before,after))
  qualified='exchange::tests::'+test
  result=subprocess.run(prefix+[qualified,'--nocapture'],cwd=crate.parents[1],text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
  assert result.returncode==101,result.stdout
  assert qualified+' ... FAILED' in result.stdout,result.stdout
  assert 'assertion' in result.stdout or 'concrete MEXC placement shield override must run' in result.stdout,result.stdout
  print(name+': compiled and named assertion failed at101',flush=True)
finally:source.write_text(original)
subprocess.run(prefix[:-2],cwd=crate.parents[1],check=True)
env=os.environ.copy();env['MEXC_TS_CONTROL']='order-snapshot-write'
qualified='exchange::tests::mexc_reconciliation_live_store_transitions'
result=subprocess.run(prefix+[qualified,'--nocapture'],cwd=crate.parents[1],env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
assert result.returncode==101,result.stdout
assert qualified+' ... FAILED' in result.stdout,result.stdout
assert 'source subscriber throw rejects after snapshot commit' in result.stdout,result.stdout
print('TypeScript order snapshot write mutation: named subscriber assertion failed at 101',flush=True)
