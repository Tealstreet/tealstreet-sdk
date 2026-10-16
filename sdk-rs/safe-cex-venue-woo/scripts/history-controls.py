"""Reject compile-only failures while falsifying history projection invariants."""
from pathlib import Path
import os,subprocess,sys
p=Path('sdk-rs/safe-cex-venue-woo/src/order_history.rs');source=p.read_text()
cases=[
 ('asset-overflow','safe_cex_num::js_truthy(Some(v))','if v.is_number() { v.as_f64().is_some_and(|n| n != 0.0 && !n.is_nan()) } else { safe_cex_num::js_truthy(Some(v)) }'),
 ('seconds-scale','(n * 1000.0).floor()','n.floor()'),
 ('fraction-floor','(n * 1000.0).floor()','n * 1000.0'),
 ('parse-float','safe_cex_num::parse_float','safe_cex_num::number_from_string'),
 ('optional-zero','n.is_finite() && n != 0.0','n.is_finite()'),
 ('pnl-presence','realized_pnl: optional_present("realized_pnl")?,','realized_pnl: to_optional_number(row.get("realized_pnl"))?,'),
 ('fee-sign','fees: optional_present("total_fee")?,','fees: optional_present("total_fee")?.map(|v|-v),'),
 ('average-price','average_price: to_optional_number(row.get("average_executed_price"))?,','average_price: to_optional_number(row.get("price"))?,'),
 ('filled-quantity','filled: to_number(row.get("executed"))?,','filled: to_number(row.get("quantity"))?,'),
 ('updated-zero','Some(to_milliseconds(row.get("updated_time"))?).filter(|v| *v != 0.0)','Some(to_milliseconds(row.get("updated_time"))?)'),
 ('unknown-type','.unwrap_or(OrderType::Limit)','.unwrap_or(OrderType::Market)'),
 ('open-status','("NEW", OrderStatus::Open)','("NEW", OrderStatus::Closed)'),
 ('string-error','safe_cex_num::js_string(value).map_err(|e| e.message.to_owned())','Ok(safe_cex_num::js_string(value).unwrap_or_else(|_|"[object Object]".into()))'),
 ('resolver-error','resolve_symbol(&market_id)?','resolve_symbol(&market_id).unwrap_or(None)'),
]
env=dict(os.environ);env['PATH']='/opt/homebrew/opt/rustup/bin:'+env['PATH']
for name,old,new in cases:
 if sys.argv[1:] and name not in sys.argv[1:]:continue
 assert old in source,(name,old)
 try:
  p.write_text(source.replace(old,new,1));r=subprocess.run(['cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-woo','--test','order_history','--locked','--offline','--jobs','2'],env=env,capture_output=True,text=True)
  Path('/tmp/woo-history-'+name+'.log').write_text(r.stdout+r.stderr)
  assert r.returncode==101 and 'test result: FAILED' in r.stdout,(name,r.stdout,r.stderr)
  print(name,'assertion-red 101',flush=True)
 finally:p.write_text(source)
