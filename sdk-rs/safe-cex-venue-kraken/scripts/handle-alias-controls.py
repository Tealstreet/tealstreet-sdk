#!/usr/bin/env python3
"""Compiled native consumer controls for Kraken's three read aliases."""
from pathlib import Path
import subprocess
import tempfile
import json
sdk=Path(__file__).resolve().parents[2]
p=sdk/'safe-cex-venue-kraken/src/binding/host.rs'
original=p.read_bytes()
receipts=Path(tempfile.mkdtemp(prefix='kraken-read-alias-controls-'))
cases=[
 ('Handle','handle_alias_retains_constructed_scope_values_identity_and_effects','return Ok(h(value.get(&name)?));','let _ = value.get(&name)?; return Ok(h(Js::Undefined));'),
 ('ReadResult','read_result_alias_keeps_typed_success_and_original_thrown_value','if optional && value.is_nullish() {','if optional || value.is_nullish() {'),
 ('Entries','entries_alias_keeps_own_key_order_utf16_and_original_native_cells','.map(|(k, v)| (k.to_vec(), h(v)))','.map(|(k, v)| (k.to_vec(), h(Js::array(vec![v]))))'),
]
results=[]
def run(name,label,negative):
 r=subprocess.run(['cargo','test','--locked','--jobs','1','-p','safe-cex-venue-kraken','--lib',name,'--','--exact','--nocapture'],cwd=sdk,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
 (receipts/f'{label}.log').write_text(r.stdout)
 if negative:
  assert r.returncode!=0 and f'test {name} ... FAILED' in r.stdout and 'panicked at' in r.stdout and 'could not compile' not in r.stdout,r.stdout
 else: assert r.returncode==0 and f'test {name} ... ok' in r.stdout,r.stdout
 return r.returncode
try:
 for alias,test,needle,fault in cases:
  source=original.decode(); assert source.count(needle)==1,(alias,needle)
  name=f'binding::exchange::accepted_handle_alias::{test}'
  try:
   p.write_text(source.replace(needle,fault))
   red=run(name,f'{alias}-red',True)
  finally: p.write_bytes(original)
  green=run(name,f'{alias}-restored',False)
  results.append({'alias':alias,'test':name,'compiledRed':red,'exactRestoredGreen':green})
  (receipts/'receipts.json').write_text(json.dumps(results,indent=2)+'\n')
  print(f'{alias}: named compiled RED/exact-restored GREEN',flush=True)
finally: p.write_bytes(original)
print(receipts,flush=True)
