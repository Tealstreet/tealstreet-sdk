"""Income-record native field and corpus controls; not a paging implementation."""
from pathlib import Path
import os,subprocess
crate=Path(__file__).resolve().parent.parent
source=crate/'src/income.rs';original=source.read_bytes()
oracle=crate/'tests/income-oracle.cjs';oracle_original=oracle.read_bytes()
command=[os.environ.get('CARGO','/opt/homebrew/opt/rustup/bin/cargo'),'test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-weex','--jobs','1','--test','income']
controls=[
 (b'truthy_string(&["symbol"], "")',b'truthy_string(&["asset"], "")','income::distinct::route_0::symbol'),
 (b'truthy_string(&["incomeType"], "")',b'truthy_string(&["symbol"], "")','income::distinct::route_0::incomeType'),
 (b'raw.pick_retained(&["income"])',b'raw.pick_retained(&["time"])','income::distinct::route_0::income'),
 (b'truthy_string(&["asset", "coin"], "USDT")',b'truthy_string(&["symbol"], "USDT")','income::distinct::route_0::asset'),
 (b'raw.pick_retained(&["time", "createdTime"])',b'raw.pick_retained(&["income"])','income::distinct::route_0::timestamp'),
 (b'raw.pick_retained(&["billId", "tranId", "id"])',b'raw.pick_retained(&["tradeId"])','income::distinct::route_0::tranId'),
 (b'raw.pick_retained(&["tradeId"])',b'raw.pick_retained(&["billId"])','income::distinct::route_0::tradeId'),
 (b'value.get("incomeType")',b'None','income::distinct::route_0::category'),
]
def fail(label):
 r=subprocess.run(command,capture_output=True,text=True);out=r.stdout+r.stderr
 assert r.returncode==101 and label in out and 'assertion `left == right` failed' in out,out
 print('COMPILED; exit101; '+label,flush=True)
try:
 for before,after,label in controls:
  assert original.count(before)==1,before
  source.write_bytes(original.replace(before,after));fail(label);source.write_bytes(original)
 for mutation,label in [('cases.splice(0);','income::source_cases'),('cases.splice(1);','income::source_cases'),('cases.push(cases[0]);','income::source_cases'),('cases[1]=cases[0];','income::source_identity'),('refusals[0]=refusals[1];','income::refusal_identity')]:
  assert oracle_original.count(b'process.stdout.write(')==1
  oracle.write_bytes(oracle_original.replace(b'process.stdout.write(',mutation.encode()+b'process.stdout.write('));fail(label);oracle.write_bytes(oracle_original)
finally:
 source.write_bytes(original);oracle.write_bytes(oracle_original)
subprocess.run(command,check=True)
print('RESTORED PASS; 8 production + 5 census controls',flush=True)
