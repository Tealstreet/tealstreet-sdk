#!/usr/bin/env python3
"""Eight compiled runtime family defects and exact restoration."""
import os, subprocess
from pathlib import Path
crate=Path(__file__).resolve().parents[1]
revision=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()
p=crate/'src/runtime_actions.rs';original=p.read_bytes();source=original.decode()
controls=[
 ('pub fn transient','>= 500.0','>= 600.0','transient_native','native boolean'),
 ('pub fn dispose','h.boolean(true)','h.boolean(false)','dispose_native','native boolean'),
 ('pub async fn start','("markets", h.boolean(true))','("markets", h.boolean(false))','start_native','native boolean'),
 ('pub async fn tick','h.number(100.0)','h.number(10.0)','tick_native','native bits'),
 ('pub async fn update_order','"order_price"','"WRONG_PRICE"','update_native','native object key'),
 ('pub async fn place_order','text(h, tag)','text(h, &format!("WRONG{tag}"))','place_native','native UTF-16'),
 ('pub async fn place_algo','&u("stopLoss")','&u("WRONG_STOP")','algo_native','native object key'),
 ('pub async fn place_positional','text(h, "POSITIONAL_TP_SL")','text(h, "WRONG_TP_SL")','positional_native','native UTF-16'),
]
for start,old,new,test,assertion in controls:
 a=source.index(start);b=source.find('\npub ',a+len(start));b=len(source) if b==-1 else b
 region=source[a:b];expected=2 if start=='pub async fn tick' else 1
 assert region.count(old)==expected,(start,old,region.count(old))
 if expected==2:
  c=region.rindex(old);region=region[:c]+new+region[c+len(old):]
 else:region=region.replace(old,new)
 source=source[:a]+region+source[b:]
def run():
 return subprocess.run(['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-orderly','--test','runtime_actions','--jobs','2'],env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript'),text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
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
print(f'{revision}: eight compiled disjoint family defects; eight named failures/restored passes; exact bytes restored')
