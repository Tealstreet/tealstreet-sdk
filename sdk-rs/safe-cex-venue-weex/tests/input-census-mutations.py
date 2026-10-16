"""Keep row IDs and recompute actual-source results after duplicating raw inputs."""
from pathlib import Path
import os,subprocess,sys
crate=Path(__file__).resolve().parent.parent
for target in sys.argv[1:]:
 assert target in ['balance','public_trade','income'],target
 name=target.replace('_','-'); oracle=crate/'tests'/(name+'-oracle.cjs');original=oracle.read_bytes()
 command=[os.environ.get('CARGO','/opt/homebrew/opt/rustup/bin/cargo'),'test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-weex','--jobs','1','--test',target]
 index=3 if target=='balance' else 1
 mutations=[(f'inputs[{index}][1]=inputs[0][1];',target+'::input_identity')]
 if target=='public_trade':mutations.append(('for(let i=16;i<inputs.length;i++)inputs[i][1]=inputs[15][1];','public_trade::flag_inputs'))
 try:
  before=b'const cases=inputs.map('
  assert original.count(before)==1
  for mutation,label in mutations:
   oracle.write_bytes(original.replace(before,mutation.encode()+before))
   result=subprocess.run(command,capture_output=True,text=True);out=result.stdout+result.stderr
   assert result.returncode==101 and label in out and 'assertion `left == right` failed' in out,out
   print('COMPILED; exit101; '+label,flush=True)
   oracle.write_bytes(original)
 finally:oracle.write_bytes(original)
 subprocess.run(command,check=True)
 print('RESTORED PASS '+target,flush=True)
