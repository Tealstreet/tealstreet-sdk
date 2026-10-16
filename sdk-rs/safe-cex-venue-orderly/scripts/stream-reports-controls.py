#!/usr/bin/env python3
"""Eight disjoint body defects; named native failures and exact restoration."""
import os,subprocess
from pathlib import Path
crate=Path(__file__).resolve().parents[1]
revision=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()
p=crate/'src/stream_reports.rs';original=p.read_bytes();source=original.decode()
controls=[
 ('("last", get(h, &row, "close")?)','("last", get(h, &row, "volume")?)','tickers_native','native bits'),
 ('("bid", get(h, &row, "bid")?)','("bid", get(h, &row, "ask")?)','bbo_native','native bits'),
 ('("mark", get(h, &row, "price")?)','("mark", get(h, &row, "volume")?)','mark_native','native bits'),
 ('h.parse_float(h.get_key(row.clone(), "fundingRate")?)?','h.parse_float(h.get_key(row.clone(), "fundingTs")?)?','funding_native','native bits'),
 ('("event", text(h, "pong"))','("event", text(h, "ping"))','private_message_native','native UTF-16'),
 ('if h.truthy(&position) {','if false && h.truthy(&position) {','positions_native','native array cardinality'),
 ('let amount = h.get_key(data.clone(), "executedQuantity")?;','let amount = h.get_key(data.clone(), "executedPrice")?;','execution_native','native UTF-16'),
 ('("amount", get(h, &row, "totalExecutedQuantity")?)','("amount", get(h, &row, "averageExecutedPrice")?)','algo_native','native bits'),
]
for old,new,test,assertion in controls:
 assert source.count(old)==1,old
 source=source.replace(old,new)
def run():
 return subprocess.run(['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-orderly','--test','stream_reports','--jobs','2'],env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript'),text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
try:
 p.write_text(source);broken=run()
 assert broken.returncode==101 and 'test result: FAILED' in broken.stdout,broken.stdout
 for old,new,test,assertion in controls:
  assert f'test {test} ... FAILED' in broken.stdout,(test,broken.stdout)
  failure=broken.stdout.split(f'---- {test} stdout ----',1)[1].split('----',1)[0]
  assert assertion in failure,(test,failure)
  print(f'{test}: compiled native assertion exit 101 ({assertion})',flush=True)
finally:p.write_bytes(original)
restored=run();assert restored.returncode==0,restored.stdout
for old,new,test,assertion in controls:
 assert f'test {test} ... ok' in restored.stdout,(test,restored.stdout)
 print(f'{test}: restored exit 0',flush=True)
assert p.read_bytes()==original
print(f'{revision}: eight disjoint defects compiled, eight named failures/restored passes, byte-exact restoration')
