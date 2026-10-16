#!/usr/bin/env python3
"""Eight disjoint class-factory defects, named native failures and restoration."""
import os,subprocess
from pathlib import Path
crate=Path(__file__).resolve().parents[1]
revision=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()
p=crate/'src/classes.rs';original=p.read_bytes();source=original.decode()
controls=[
 ('units("ed25519:")','units("ed25518:")','constructor_native'),
 ('construct(h, Kind::Orderly, args)','{ let r=construct(h, Kind::Orderly, args)?; h.define(r.clone(), "orderlyExchangeName", text(h, "WRONG_ORDERLY"))?; Ok(r) }','orderly_native'),
 ('Some("KODIAKFI")','Some("WRONG_KODIAKFI")','kodiakfi_native'),
 ('Some("MODE")','Some("WRONG_MODE")','mode_native'),
 ('Some("SALSADEX")','Some("WRONG_SALSADEX")','salsadex_native'),
 ('Some("Tealstreet DEX")','Some("WRONG_TEALSTREET_DEX")','tealstreet_dex_native'),
 ('Some("WHAT")','Some("WRONG_WHAT")','what_native'),
 ('Some("WOOFI")','Some("WRONG_WOOFI")','woofi_native'),
]
for old,new,test in controls:
 assert source.count(old)==1,(old,source.count(old));source=source.replace(old,new)
def run():
 return subprocess.run(['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-orderly','--test','classes','--jobs','2'],env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript'),text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
try:
 p.write_text(source);broken=run()
 assert broken.returncode==101 and 'test result: FAILED' in broken.stdout,broken.stdout
 for old,new,test in controls:
  assert f'test {test} ... FAILED' in broken.stdout,(test,broken.stdout)
  failure=broken.stdout.split(f'---- {test} stdout ----',1)[1].split('----',1)[0]
  assert 'native UTF-16' in failure,(test,failure)
  print(f'{test}: compiled native UTF-16 assertion exit101',flush=True)
finally:p.write_bytes(original)
restored=run();assert restored.returncode==0,restored.stdout
for old,new,test in controls:
 assert f'test {test} ... ok' in restored.stdout,(test,restored.stdout)
 print(f'{test}: restored exit0',flush=True)
assert p.read_bytes()==original
print(f'{revision}: eight compiled family defects; eight named failures/restored passes; exact source bytes restored')
