#!/usr/bin/env python3
"""Eight body controls; named native failures followed by exact restoration."""
import os,subprocess
from pathlib import Path
crate=Path(__file__).resolve().parents[1]
revision=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()
p=crate/'src/account_order_reads.rs';original=p.read_bytes();source=original.decode()
controls=[
 ('pub async fn position_mode','text(h, "oneway")','text(h, "hedged")','position_mode_native','native UTF-16'),
 ('pub async fn account','if h.truthy(&affiliate)','if !h.truthy(&affiliate)','account_native','native null'),
 ('pub async fn validate','optional(h, &data, "msg")?','optional(h, &data, "message")?','validation_native','native UTF-16'),
 ('pub async fn leverage','h.number(20.0)','h.number(21.0)','leverage_native','native bits'),
 ('pub async fn margin_types','!h.text_eq(&margin, "ISOLATED")','!h.text_eq(&margin, "WRONG")','margin_types_native','native array cardinality'),
 ('pub async fn limit_orders','if h.is_null(&order)','if !h.truthy(&order)','limit_orders_native','native array cardinality'),
 ('pub async fn algo_orders','next.extend(h.spread(children)?);','next.extend(h.spread(children)?.into_iter().take(0));','algo_orders_native','native array cardinality'),
 ('pub async fn orders','let mut all = h.spread(limits)?;\n    all.extend(h.spread(algos)?);','let mut all = h.spread(algos)?;\n    all.extend(h.spread(limits)?);','orders_native','native object cardinality'),
]
for start,old,new,test,assertion in controls:
 a=source.index(start);b=source.find('pub async fn ',a+len(start));b=len(source) if b==-1 else b
 region=source[a:b];assert region.count(old)==1,(start,old)
 source=source[:a]+region.replace(old,new)+source[b:]
def run():
 return subprocess.run(['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-orderly','--test','account_order_reads','--jobs','2'],env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript'),text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
try:
 p.write_text(source);broken=run()
 assert broken.returncode==101 and 'test result: FAILED' in broken.stdout,broken.stdout
 for start,old,new,test,assertion in controls:
  assert f'test {test} ... FAILED' in broken.stdout,(test,broken.stdout)
  failure=broken.stdout.split(f'---- {test} stdout ----',1)[1].split('----',1)[0]
  assert assertion in failure,(test,failure)
  print(f'{test}: compiled native assertion exit 101 ({assertion})',flush=True)
finally:p.write_bytes(original)
restored=run();assert restored.returncode==0,restored.stdout
for start,old,new,test,assertion in controls:
 assert f'test {test} ... ok' in restored.stdout,(test,restored.stdout)
 print(f'{test}: restored exit 0',flush=True)
assert p.read_bytes()==original
print(f'{revision}: eight disjoint body defects compiled; eight named failures/restored passes; exact byte restoration')
