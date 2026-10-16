"""One control per exchange body; separate callers from the helpers they invoke."""
from pathlib import Path
import subprocess,re
root=Path(__file__).resolve().parents[3];file=root/'sdk-rs/safe-cex-venue-apexomni/src/exchange.rs';helper=root/'sdk-rs/safe-cex-venue-apexomni/src/executions.rs';originals={file:file.read_text(),helper:helper.read_text()};logs=root/'sdk-rs/target/apexomni-controls'
leaf=[
('new','Cell::new(1.0)','Cell::new(3.0)','constructor_native','ApexOmniExchange constructor native margin'),
('dispose','self.base.dispose()?;','if false { self.base.dispose()?; }','dispose_native','dispose result'),
('account_type','== 1.0','== 2.0','account_type_native','accountType result'),
('get_account','"id"','"wrongId"','account_native','getAccount result'),
('fetch_raw_balance','"maintenanceMargin"','"initialMargin"','raw_balance_native','fetchRawBalance result'),
('map_position','let entry_price = float(&entry)?;','let entry_price = float(&entry)? * 2.0;','position_native','mapPosition result'),
('map_order','price: float(price)?,','price: float(price)? * 2.0,','order_native','mapOrder result'),
('fetch_markets_helper','price: float(get(row, "tickSize"))?,','price: float(get(row, "tickSize"))? * 2.0,','markets_helper_native','fetchMarketsHelper result'),
('cancel_orders_helper','self.base.store.remove_orders(','if false { self.base.store.remove_orders(','cancel_helper_native','cancelOrdersHelper'),
('record_ws_execution','executions.insert(0, execution);','let mut execution=execution;execution.qty+=1.0;executions.insert(0, execution);','record_execution_native','recordWsExecution independent native qty'),
]
# Cancellation uses a request-id control, rather than hiding a Store call behind a guard.
leaf[8]=('cancel_orders_helper','("id", J::String(id.clone()))','("id", J::String(format!("MUTATED-{id}")))','cancel_helper_native','cancelOrdersHelper operations')
parents=[
('fetch_balance','if self.base.is_disposed()','if !self.base.is_disposed()','balance_native','fetchBalance result'),
('fetch_orders','<= 10_000.0','<= 9_000.0','orders_native','fetchOrders'),
('fetch_positions','let mut positions = Vec::new();','let mut positions = Vec::new();','positions_native','fetchPositions result'),
('fetch_markets','self.fetch_markets_helper().await','{ let mut markets=self.fetch_markets_helper().await?; for market in &mut markets { market.precision.amount *= 2.0; } Ok(markets) }','markets_native','fetchMarkets result'),
('cancel_orders','self.cancel_orders_helper(orders, "0").await','self.cancel_orders_helper(&orders[..0], "0").await','cancel_native','cancelOrders operations'),
('get_ws_execution_fallback','result.truncate(keep);','result.truncate(keep.min(0));','execution_fallback_native','getWsExecutionFallback result'),
]
parents[2]=('fetch_positions','leverage,','leverage: leverage * 2.0,','positions_native','fetchPositions result')

def mutate_method(text,name,before,after):
 match=re.search(r'    pub (?:async )?fn '+name+r'\b',text);assert match,name
 start=match.start();following=re.search(r'\n    (?:pub |fn )',text[match.end():]);end=match.end()+following.start() if following else len(text)
 part=text[start:end]
 count=part.count(before)
 if name=='new' and before=='Cell::new(1.0)':assert count==2;part=part.replace(before,after,1)
 else:assert count==1,(name,before,count);part=part.replace(before,after)
 return text[:start]+part+text[end:]
cmd=['cargo','test','--manifest-path',str(root/'sdk-rs/Cargo.toml'),'-p','safe-cex-venue-apexomni','--test','exchange','--jobs','2','--','--test-threads=1']
try:
 for name,controls in [('leaf',leaf),('caller',parents)]:
  mutant=originals[file]
  for method,before,after,_,_ in controls:mutant=mutate_method(mutant,method,before,after)
  helper_mutant=originals[helper]
  if name=='leaf':
   helper_mutant=helper_mutant.replace('strip_prefix("PERP")','strip_prefix("PERPX")').replace('(parsed / 1000.0).floor()','(parsed / 100.0).floor()')
  file.write_text(mutant);helper.write_text(helper_mutant)
  result=subprocess.run(cmd,cwd=root,capture_output=True,text=True);output=result.stdout+result.stderr;(logs/f'exchange-{name}-mutation.log').write_text(output)
  assert result.returncode==101 and 'test result: FAILED.' in output,output
  for _,_,_,test,label in controls:assert f'test {test} ... FAILED' in output and label in output,(test,label,output)
  if name=='leaf':
   for test,label in [('symbol_native','normalizeApexSymbolKey result'),('timestamp_native','normalizeExecutionTimestampToSeconds result')]:assert f'test {test} ... FAILED' in output and label in output,(test,label,output)
  print(f'{name}: compiling controls failed101 at named native assertions',flush=True)
finally:
 for path,text in originals.items():path.write_text(text)
result=subprocess.run(cmd,cwd=root,capture_output=True,text=True);output=result.stdout+result.stderr;(logs/'exchange-restored.log').write_text(output);assert result.returncode==0,output
print('18 exchange controls restored: exit 0',flush=True)
