"""Sequential runtime controls; restore each source and reject compiler failures."""
from pathlib import Path
import subprocess,os,re
p=Path('sdk-rs/safe-cex-venue-woo/src/v3_public.rs');original=p.read_text()
cases=[]
def add(name,old,new):
 assert old in original,name
 cases.append((name,original.replace(old,new,1)))
add('number-radix','safe_cex_num::number_from_string(s).is_finite()', 's.parse::<f64>().unwrap_or(f64::NAN).is_finite()')
add('feff-trim','safe_cex_num::js_trim(s).is_empty()','s.trim().is_empty()')
add('smallest-positive','f64::from_bits(1)','f64::MIN_POSITIVE')
add('book-sort','safe_cex_store::orderbook::sort_order_book(&mut book);','')
add('book-total','safe_cex_store::orderbook::calc_order_book_total(&mut book);','')
add('minimum','result < minimum','result < f64::NEG_INFINITY')
add('kline-order','end_timestamp < start_timestamp','false')
pattern=r'let map = serde_json::Map::<String, serde_json::Value>::deserialize\(deserializer\)\?;\s*(\w+)Wire::deserialize\(serde_json::Value::Object\(map\)\)'
for m in re.finditer(pattern,original):
 # Flatten's private derived visitor already rejects sequences; don't claim a redundant control.
 if m[1]=='WooV3MappedOrderBookSnapshot':continue
 cases.append((m[1]+'-map',original[:m.start()]+m[1]+'Wire::deserialize(deserializer)'+original[m.end():]))
cases.append(('public-inherent',original+"\nimpl WooV3OrderBookLevel { pub fn deserialize<'de,D:serde::Deserializer<'de>>(d:D)->Result<Self,D::Error> {WooV3OrderBookLevelWire::deserialize(d)} }\n"))
env=dict(os.environ);env['PATH']='/opt/homebrew/opt/rustup/bin:'+env['PATH']
for name,mutated in cases:
 try:
  p.write_text(mutated)
  r=subprocess.run(['cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-woo','--test','v3_public','--jobs','2'],env=env,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
  Path('/tmp/woo-public-'+name+'.log').write_text(r.stdout)
  ok=r.returncode==101 and 'test result: FAILED' in r.stdout
  print(name,r.returncode,'assertion-red' if ok else 'BAD',flush=True)
  assert ok,r.stdout
 finally:p.write_text(original)

source=Path('packages/safe-cex/src/exchanges/woo/woo.v3.public.ts')
saved=source.read_text()
try:
 old='.sort((a, b) => a.timestamp - b.timestamp);'
 assert saved.count(old)==1
 source.write_text(saved.replace(old,'.sort((a, b) => (Number.isNaN(a.timestamp) ? Infinity : a.timestamp) - (Number.isNaN(b.timestamp) ? Infinity : b.timestamp));'))
 run=subprocess.run(['node','sdk-rs/safe-cex-venue-woo/scripts/public-oracle.cjs','--check'],env=env,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
 Path('/tmp/woo-public-ts-nan-control.log').write_text(run.stdout)
 assert run.returncode!=0 and 'source NaN sort changed; revisit chosen divergence' in run.stdout,run.stdout
 print('source-nan-correction',run.returncode,'assertion-red',flush=True)
finally:source.write_text(saved)
