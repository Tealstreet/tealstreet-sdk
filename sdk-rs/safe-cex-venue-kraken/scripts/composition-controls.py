#!/usr/bin/env python3
"""Two family defects, seven named native failures, exact restoration."""
import os
import subprocess
from pathlib import Path
crate=Path(__file__).resolve().parents[1]
revision=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()
controls=[
 ('classes.rs','for definition in definitions(kind) {','for definition in definitions(kind).iter().rev() {',2,('futures_exchange_class_native','spot_exchange_class_native','futures_public_class_native','spot_public_class_native','futures_private_class_native','spot_private_class_native'),'native own-key order'),
 ('ohlcv_polling.rs','h.set_timeout(poll, 5000.0)?','h.set_timeout(poll, 5001.0)?',1,('polling_native',),'native bits'),
]
originals={name:(crate/'src'/name).read_bytes()for name,*_ in controls}
mutants={name:data.decode()for name,data in originals.items()}
for name,old,new,count,tests,assertion in controls:
 assert mutants[name].count(old)==count,(name,old,count)
 mutants[name]=mutants[name].replace(old,new)
def run():
 return subprocess.run(['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-kraken','--test','composition','--jobs','2'],env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript'),text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
try:
 for name,source in mutants.items():(crate/'src'/name).write_text(source)
 broken=run()
 assert broken.returncode==101 and 'test result: FAILED' in broken.stdout,broken.stdout
 for name,old,new,count,tests,assertion in controls:
  for test in tests:
   assert f'test {test} ... FAILED' in broken.stdout,(test,broken.stdout)
   failure=broken.stdout.split(f'---- {test} stdout ----',1)[1].split('----',1)[0]
   assert assertion in failure,(test,failure)
   print(f'{name} / {test}: compiled native assertion exit 101; {assertion}',flush=True)
finally:
 for name,original in originals.items():(crate/'src'/name).write_bytes(original)
restored=run()
assert restored.returncode==0,restored.stdout
for name,old,new,count,tests,assertion in controls:
 for test in tests:
  assert f'test {test} ... ok' in restored.stdout,(test,restored.stdout)
  print(f'{name} / {test}: restored exit 0',flush=True)
for name,original in originals.items():assert (crate/'src'/name).read_bytes()==original
print(f'{revision}: three family defects compiled together; nine named failures and restored passes; exact byte restoration')
