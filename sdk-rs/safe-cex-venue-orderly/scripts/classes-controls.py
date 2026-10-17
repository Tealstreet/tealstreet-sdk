#!/usr/bin/env python3
"""Live constructor normalization and family-tag controls, with exact restoration."""
import os,subprocess
from pathlib import Path
crate=Path(__file__).resolve().parents[1]
revision=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()
p=crate/'src/classes.rs';original=p.read_bytes();source=original.decode()
controls=[
 ('units("ed25519:")','units("ed25518:")','constructor_native'),
 ('Self::Orderly => None','Self::Orderly => Some("WRONG_ORDERLY")','orderly_native'),
 ('Some("KODIAKFI")','Some("WRONG_KODIAKFI")','kodiakfi_native'),
 ('Some("MODE")','Some("WRONG_MODE")','mode_native'),
 ('Some("SALSADEX")','Some("WRONG_SALSADEX")','salsadex_native'),
 ('Some("Tealstreet DEX")','Some("WRONG_TEALSTREET_DEX")','tealstreet_dex_native'),
 ('Some("WHAT")','Some("WRONG_WHAT")','what_native'),
 ('Some("WOOFI")','Some("WRONG_WOOFI")','woofi_native'),
]
def run(target="classes", witness=None):
 command=['cargo','test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-orderly','--test',target,'--jobs','1']
 if witness: command.extend([witness,'--','--exact'])
 return subprocess.run(command,env=os.environ.copy(),text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
for group in [controls[:1], controls[1:]]:
 source=original.decode()
 for old,new,test in group:
  assert source.count(old)==1,(old,source.count(old));source=source.replace(old,new)
 try:
  p.write_text(source);broken=run()
  assert broken.returncode==101 and 'test result: FAILED' in broken.stdout,broken.stdout
  for old,new,test in group:
   assert f'test {test} ... FAILED' in broken.stdout,(test,broken.stdout)
   failure=broken.stdout.split(f'---- {test} stdout ----',1)[1].split('----',1)[0]
   marker='native UTF-16'
   assert marker in failure,(test,failure)
   print(f'{test}: compiled named constructor assertion exit101',flush=True)
  binding=run('exchange_trait','each_family_constructor_preserves_its_source_order_tag_and_normalized_key')
  assert binding.returncode==101 and 'test each_family_constructor_preserves_its_source_order_tag_and_normalized_key ... FAILED' in binding.stdout,binding.stdout
  print('real Exchange constructor/signing binding: compiled runtime failure exit101',flush=True)
 finally:p.write_bytes(original)
 restored=run();assert restored.returncode==0,restored.stdout
 for old,new,test in group:
  assert f'test {test} ... ok' in restored.stdout,(test,restored.stdout)
  print(f'{test}: restored exit0',flush=True)
 binding=run('exchange_trait','each_family_constructor_preserves_its_source_order_tag_and_normalized_key')
 assert binding.returncode==0,binding.stdout
 assert p.read_bytes()==original
print(f'{revision}: eight constructor defects, real binding failures and restored passes; exact source bytes restored')
