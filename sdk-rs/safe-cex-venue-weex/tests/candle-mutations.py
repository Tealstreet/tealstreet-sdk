"""Per-field production tuple and object mapping controls; run serially."""
from pathlib import Path
import os, subprocess, re
crate=Path(__file__).resolve().parent.parent
source=crate/'src/utils.rs';original=source.read_text();start=original.index('pub fn normalize_candle(')
prefix,body=original[:start],original[start:]
oracle=crate/'tests/candle-oracle.cjs'; oracle_original=oracle.read_bytes()
command=[os.environ.get('CARGO','/opt/homebrew/opt/rustup/bin/cargo'),'test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-weex','--jobs','1','--test','candles']
controls=[]
fields=['timestamp','open','high','low','close','volume']
for index,field in enumerate(fields):
 controls.append((f'pick({index},',f'pick({(index+1)%6},',f'candle::tuple::route_0::{field}'))
for index,field in enumerate(fields):
 other=fields[(index+1)%6]
 controls.append((f'&["{field}",',f'&["{other}",',f'candle::object::route_0::{field}'))
order=re.search(r'(        timestamp: normalize_timestamp_to_seconds\([\s\S]*?\)\?,\n)(        open: [^\n]+\n)',body)
assert order, 'timestamp/open initializer inventory'
controls.append((order.group(0),order.group(2)+order.group(1),'candle::error_order::date_before_open'))
try:
 for before,after,assertion in controls:
  assert body.count(before)==1,before
  source.write_text(prefix+body.replace(before,after))
  result=subprocess.run(command,capture_output=True,text=True);output=result.stdout+result.stderr
  assert result.returncode==101 and assertion in output and 'assertion `left == right` failed' in output,output
  print('COMPILED; exit101; FAIL observed: '+assertion,flush=True)
  source.write_text(original)
 # The recovered census runs before the integration guards; retain the guards
 # and require the earlier named census assertions after merge-down.
 for mutation,assertion in [
  ('cases.splice(0);','cardinality::candles::cases::source_count'),
  ('cases.splice(1);','cardinality::candles::cases::source_count'),
  ('cases.push(cases[0]);','cardinality::candles::cases::source_count'),
  ('cases[1]=cases[0];','cardinality::candles::unique_identities'),
  ('refusals.splice(0);','cardinality::candles::refusals::source_count'),
  ('refusals[0]=refusals[1];','cardinality::candles::refusals::unique_identities'),
 ]:
  before=b'process.stdout.write('
  assert oracle_original.count(before)==1
  oracle.write_bytes(oracle_original.replace(before,mutation.encode()+before))
  result=subprocess.run(command,capture_output=True,text=True);output=result.stdout+result.stderr
  assert result.returncode==101 and assertion in output and 'assertion `left == right` failed' in output,output
  print('COMPILED; exit101; FAIL observed: '+assertion,flush=True)
  oracle.write_bytes(oracle_original)
finally:
 source.write_text(original)
 oracle.write_bytes(oracle_original)
assert oracle.read_bytes()==oracle_original
subprocess.run(command,check=True)
print('Restored PASS after 12 per-field mapping controls and 1 evaluation-order control and 6 source-census controls',flush=True)
