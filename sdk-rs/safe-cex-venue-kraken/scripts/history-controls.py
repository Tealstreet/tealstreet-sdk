#!/usr/bin/env python3
"""Serial historical delegation production controls; restore both files byte-exact."""
from pathlib import Path
import os, subprocess, re
crate=Path(__file__).resolve().parents[1]
source=crate/'src/reads.rs'
test=crate/'tests/support/history.rs'
original=source.read_bytes(); witness=test.read_bytes()
anchor='        false,\n    )\n    .await'
controls=[
 ('symbol-scope',anchor,anchor.replace('false','true'),'HISTORY native fold arguments',False),
 ('bounds-swap','from: options.from,\n            to: options.to,','from: options.to,\n            to: options.from,','HISTORY native fold arguments',False),
 ('empty',anchor,anchor+'.map(|mut rows| { rows.clear(); rows })','HISTORY native record count',False),
 ('subset',anchor,anchor+'.map(|mut rows| { rows.truncate(1); rows })','HISTORY native record count',False),
 ('extra',anchor,anchor+'.map(|mut rows| { rows.push(Value::Null); rows })','HISTORY native record count',False),
 ('empty-length-bypassed',anchor,anchor+'.map(|mut rows| { rows.clear(); rows })','HISTORY native records',True),
 ('flatten-error',anchor,anchor+'.map_err(|error| ExchangeError::new(error.message))','HISTORY error allocation retained',False),
]
command=['/opt/homebrew/opt/rustup/bin/cargo','test','--locked','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-kraken','--test','reads','--jobs','1','historical_']
def run():
 return subprocess.run(command,env=os.environ,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
base=run(); assert base.returncode==0,base.stdout
for name,old,new,assertion,bypass in controls:
 try:
  text=original.decode(); assert text.count(old)==1,(name,'anchor')
  source.write_text(text.replace(old,new))
  if bypass:
   text,n=re.subn(r'\s*assert_eq!\(records.len\(\), 2, "HISTORY native record count"\);','',witness.decode())
   assert n==1,('length guard',n)
   test.write_text(text)
  broken=run()
  assert broken.returncode==101 and 'test result: FAILED' in broken.stdout and assertion in broken.stdout,(name,broken.stdout)
  print(f'{name}: compiled; exit 101; {assertion}',flush=True)
 finally:
  source.write_bytes(original); test.write_bytes(witness)
 restored=run(); assert restored.returncode==0,(name,restored.stdout)
 assert source.read_bytes()==original and test.read_bytes()==witness
 print(f'{name}: restored pass',flush=True)
print('7 production mutations failed named assertions; 7 restored passes')
