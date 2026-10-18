#!/usr/bin/env python3
"""Compile faults in live metadata alias consumers, then restore exact source."""
from pathlib import Path
import subprocess
import argparse
workspace=Path(__file__).resolve().parents[2]
source=workspace/'safe-cex-venue-woo/src/legacy_metadata.rs'
original=source.read_bytes()
test='binding::exchange::structural_alias_tests::metadata_aliases_keep_native_units_cache_identity_and_signed_fields'
controls=[
 ('units', b'symbol: symbol.clone(),', b'symbol: "corrupted".encode_utf16().collect(),'),
 ('rows', b'if now - c.updated_at < 30000.0 {\n                return Ok(Err(c.metadata.clone()));', b'if now - c.updated_at < 30000.0 {\n                state.cache.borrow_mut().delete(&key);\n                return Ok(Err(c.metadata.clone()));'),
 ('fields', b'("position_mode", Field::Text(u("ONE_WAY"))),',b'("position_mode", Field::Text(u("HEDGE_MODE"))),'),
]
parser=argparse.ArgumentParser()
parser.add_argument('--control',action='append',choices=[row[0] for row in controls])
selected=parser.parse_args().control
controls=[row for row in controls if selected is None or row[0] in selected]
logs=workspace/'target/woo-structural-alias-controls';logs.mkdir(parents=True,exist_ok=True)
def run(label):
 r=subprocess.run(['cargo','test','--locked','--jobs','1','-p','safe-cex-venue-woo','--lib',test,'--','--exact','--nocapture'],cwd=workspace,capture_output=True,text=True,timeout=900)
 output=r.stdout+r.stderr;(logs/(label+'.log')).write_text(output);return r.returncode,output
try:
 for name,before,after in controls:
  assert source.read_bytes()==original
  assert original.count(before)==1,(name,original.count(before))
  source.write_bytes(original.replace(before,after))
  code,output=run(name+'-fault')
  assert code==101 and f'test {test} ... FAILED' in output and 'test result: FAILED' in output and 'could not compile' not in output,output[-8000:]
  print(f'CONTROL PASS {name}: compiled real class assertion failure',flush=True)
  source.write_bytes(original)
  code,output=run(name+'-restored')
  assert code==0 and f'test {test} ... ok' in output and '1 passed; 0 failed' in output,output[-8000:]
  print(f'RESTORED PASS {name}: exact source and real class witness',flush=True)
finally:
 source.write_bytes(original)
