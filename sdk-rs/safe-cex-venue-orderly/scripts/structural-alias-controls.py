#!/usr/bin/env python3
"""Compile three native host faults; require named failures and exact restores."""
from pathlib import Path
import subprocess

workspace=Path(__file__).resolve().parents[2]
source=workspace/'safe-cex-venue-orderly/src/binding/host.rs'
original=source.read_bytes()
test='native_exchange::structural_alias_tests::native_host_aliases_preserve_values_entries_and_original_callback_failures'
controls=[
 ('value','get',b'\n        let next=value.get_key(key)?;\n        let next=match next { Js::Number(n) if n.to_bits()==(-0.0f64).to_bits()=>Js::Number(0.0), other=>other };\n        Ok(v(next))\n    '),
 ('entries','entries',b'\n        let _=value;\n        Ok(Vec::new())\n    '),
 ('result','call',b'\n        let result=function.call(receiver.map_or(Js::Undefined,|x|(*x).clone()),args.into_iter().map(|x|(*x).clone()).collect());\n        Ok(v(result.map_err(|_|Thrown::error("Error","replacement failure"))?))\n    '),
]
logs=workspace/'target/orderly-structural-alias-controls';logs.mkdir(parents=True,exist_ok=True)
def run(label):
 r=subprocess.run(['cargo','test','--locked','--jobs','1','-p','safe-cex-venue-orderly','--lib',test,'--','--exact','--nocapture'],cwd=workspace,capture_output=True,text=True,timeout=900)
 output=r.stdout+r.stderr;(logs/(label+'.log')).write_text(output);return r.returncode,output
try:
 for name,method,replacement in controls:
  assert source.read_bytes()==original
  begin=original.index(('    fn '+method+'(').encode(),original.index(b'impl WsHost for Host {'))
  start=original.index(b'{',begin);end=start+1;depth=1
  while depth:
   depth+=(original[end]==123)-(original[end]==125);end+=1
  source.write_bytes(original[:start+1]+replacement+original[end-1:])
  code,output=run(name+'-fault')
  assert code==101 and f'test {test} ... FAILED' in output and 'test result: FAILED' in output and 'could not compile' not in output,output[-8000:]
  print(f'CONTROL PASS {name}: compiled actual host/class failure',flush=True)
  source.write_bytes(original)
  code,output=run(name+'-restored')
  assert code==0 and f'test {test} ... ok' in output and '1 passed; 0 failed' in output,output[-8000:]
  print(f'RESTORED PASS {name}: exact source and six constructed native classes',flush=True)
finally:
 source.write_bytes(original)
