#!/usr/bin/env python3
"""Seven disjoint action defects, eight named native failures, exact restoration."""
import os,subprocess
from pathlib import Path
crate=Path(__file__).resolve().parents[1]
revision=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()
p=crate/'src/trading_actions.rs';original=p.read_bytes();source=original.decode()
controls=[
 ('pub async fn update_algo','h.numeric(get(h, &update, "price")?)?','h.numeric(get(h, &order, "price")?)?',('update_algo_native',),'native bits'),
 ('pub async fn cancel_orders','h.parse_int(get(h, &order, "id")?, 10)?','h.parse_int(get(h, &order, "id")?, 16)?',('cancel_orders_native',),'native UTF-16'),
 ('pub async fn cancel_algo','"stopLoss"','"WRONG"',('cancel_algo_native',),'native object key'),
 ('async fn cancel_both','"CANCEL_ALGO_ORDERS"','"CANCEL_ORDERS"',('cancel_all_native','cancel_symbol_native'),'native UTF-16'),
 ('pub async fn set_leverage','let leverage = h.math_round(clamped)?;','let leverage = clamped;',('leverage_native',),'native bits'),
 ('pub async fn set_all_leverage','text(h, "CROSS")','text(h, "ISOLATED")',('all_leverage_native',),'native UTF-16'),
 ('pub async fn change_margin','("default_margin_mode", mode)','("WRONG", mode)',('margin_native',),'native object key'),
]
for start,old,new,tests,assertion in controls:
 a=source.index(start);b=source.find('\npub async fn ',a+len(start));b=len(source) if b==-1 else b
 region=source[a:b];assert region.count(old)==1,(start,old)
 source=source[:a]+region.replace(old,new)+source[b:]
def run():
 return subprocess.run(['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-orderly','--test','trading_actions','--jobs','2'],env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript'),text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
try:
 p.write_text(source);broken=run()
 assert broken.returncode==101 and 'test result: FAILED' in broken.stdout,broken.stdout
 for start,old,new,tests,assertion in controls:
  for test in tests:
   assert f'test {test} ... FAILED' in broken.stdout,(test,broken.stdout)
   failure=broken.stdout.split(f'---- {test} stdout ----',1)[1].split('----',1)[0]
   assert assertion in failure,(test,failure)
   print(f'{test}: compiled native assertion exit 101 ({assertion})',flush=True)
finally:p.write_bytes(original)
restored=run();assert restored.returncode==0,restored.stdout
for start,old,new,tests,assertion in controls:
 for test in tests:
  assert f'test {test} ... ok' in restored.stdout,(test,restored.stdout)
  print(f'{test}: restored exit 0',flush=True)
assert p.read_bytes()==original
print(f'{revision}: seven compiled disjoint family defects; eight named failures/restored passes; exact bytes restored')
