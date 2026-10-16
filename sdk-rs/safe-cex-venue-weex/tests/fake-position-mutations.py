"""Production field/arity and expected-side truncation controls; restore every file."""
from pathlib import Path
import os,subprocess
crate=Path(__file__).resolve().parent.parent
source=crate/'src/utils.rs';original=source.read_text();start=original.index('pub fn fake_positions_for_market(')
prefix,body=original[:start],original[start:]
oracle=crate/'tests/fake-positions-oracle.cjs'; oracle_original=oracle.read_bytes()
command=[os.environ.get('CARGO','/opt/homebrew/opt/rustup/bin/cargo'),'test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-weex','--jobs','1','--test','fake_positions']
controls=[('entry_price: 0.0','entry_price: 1.0','fake_position::0::entryPrice'),('        leverage,','        leverage: leverage.abs(),','fake_position::0::leverage'),('create_fake(PositionSide::Long)','create_fake(PositionSide::Short)','fake_position::0::id')]
controls += [('        create_fake(PositionSide::Short),\n', '', 'fake_position::count')]
try:
 for before,after,assertion in controls:
  assert body.count(before)==1,before
  source.write_text(prefix+body.replace(before,after))
  result=subprocess.run(command,capture_output=True,text=True);output=result.stdout+result.stderr
  assert result.returncode == 101 and assertion in output and 'assertion `left == right` failed' in output,output
  print('FAIL observed: '+assertion,flush=True)
  source.write_text(original)
 # Recovered source-total guard runs before integration arity; keep both.
 for count in (0,1):
  before=b'positions:native.map('
  assert oracle_original.count(before)==1
  oracle.write_bytes(oracle_original.replace(before,f'positions:native.slice(0,{count}).map('.encode()))
  result=subprocess.run(command,capture_output=True,text=True);output=result.stdout+result.stderr
  assert result.returncode==101 and 'cardinality::positions::source_total' in output and 'assertion `left == right` failed' in output,output
  print(f'FAIL observed: expected {count} legs: cardinality::positions::source_total',flush=True)
  oracle.write_bytes(oracle_original)
finally:
 source.write_text(original)
 oracle.write_bytes(oracle_original)
assert source.read_text()==original
assert oracle.read_bytes()==oracle_original
subprocess.run(command,check=True)
print('Restored PASS after 4 production and 2 expected-side mutations',flush=True)
