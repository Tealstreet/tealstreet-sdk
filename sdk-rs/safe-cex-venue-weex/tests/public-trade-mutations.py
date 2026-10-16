"""Public-trade field and source-census controls; one Node corpus per target."""
from pathlib import Path
import os,subprocess
crate=Path(__file__).resolve().parent.parent
source=crate/'src/public_trade.rs'; original=source.read_bytes()
oracle=crate/'tests/public-trade-oracle.cjs'; oracle_original=oracle.read_bytes()
command=[os.environ.get('CARGO','/opt/homebrew/opt/rustup/bin/cargo'),'test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-weex','--jobs','1','--test','public_trade']
controls=[
 (b'        price,',b'        price: qty,','public_trade::distinct::route_0::price'),
 (b'        qty,',b'        qty: price,','public_trade::distinct::route_0::qty'),
 (b'quote_qty: Some(quote_qty)',b'quote_qty: Some(quote_qty + 1.0)','public_trade::distinct::route_0::quoteQty'),
 (b'        timestamp,',b'        timestamp: qty,','public_trade::distinct::route_0::timestamp'),
 (b'        id,',b'        id: id + "changed",','public_trade::distinct::route_0::id'),
 (b'symbol: symbol.to_owned()',b'symbol: symbol.to_owned() + "changed"','public_trade::distinct::route_0::symbol'),
 (b'value.get("buyer") == Some(&Value::Bool(false))',b'value.get("buyer") == Some(&Value::Bool(true))','public_trade::flags_5::route_0::side'),
 (b'value.get("m") == Some(&Value::Bool(true))',b'value.get("m") == Some(&Value::Bool(false))','public_trade::flags_1::route_0::side'),
 (b'value.get("isBuyerMaker") == Some(&Value::Bool(true))',b'value.get("isBuyerMaker") == Some(&Value::Bool(false))','public_trade::flags_4::route_0::side'),
]
def fail(label):
 r=subprocess.run(command,capture_output=True,text=True);out=r.stdout+r.stderr
 assert r.returncode==101 and label in out and 'assertion `left == right` failed' in out,out
 print('COMPILED; exit101; '+label,flush=True)
try:
 for before,after,label in controls:
  assert original.count(before)==1,before
  source.write_bytes(original.replace(before,after));fail(label);source.write_bytes(original)
 for mutation,label in [('cases.splice(0);','public_trade::source_cases'),('cases.splice(1);','public_trade::source_cases'),('cases.push(cases[0]);','public_trade::source_cases'),('cases[1]=cases[0];','public_trade::source_identity'),('refusals[0]=refusals[1];','public_trade::refusal_identity')]:
  assert oracle_original.count(b'process.stdout.write(')==1
  oracle.write_bytes(oracle_original.replace(b'process.stdout.write(',mutation.encode()+b'process.stdout.write('));fail(label);oracle.write_bytes(oracle_original)
finally:
 source.write_bytes(original);oracle.write_bytes(oracle_original)
subprocess.run(command,check=True)
print('RESTORED PASS; 9 production + 5 census controls',flush=True)
