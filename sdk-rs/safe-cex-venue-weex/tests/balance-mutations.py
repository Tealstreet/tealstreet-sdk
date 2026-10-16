"""Named production/census controls. Run serially; restore all injected changes."""
from pathlib import Path
import os,subprocess
crate=Path(__file__).resolve().parent.parent
source=crate/'src/utils.rs'; original=source.read_bytes()
oracle=crate/'tests/balance-oracle.cjs'; oracle_original=oracle.read_bytes()
command=[os.environ.get('CARGO','/opt/homebrew/opt/rustup/bin/cargo'),'test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-weex','--jobs','1','--test','balance']
start=original.index(b'pub fn normalize_balance(');prefix,body=original[:start],original[start:]
controls=[
 (b'        total,',b'        total: free,','balance::distinct::route_0::total'),
 (b'        free,',b'        free: total,','balance::distinct::route_0::free'),
 (b'        upnl,',b'        upnl: upnl + 1.0,','balance::distinct::route_0::upnl'),
 (b'            frozen\n',b'            upnl\n','balance::distinct::route_0::used'),
 (b'selected\n        .or_else(|| values.first())',b'values.first().or(selected)','balance::second::route_0::total'),
 (b'safe_cex_num::math_max(total - free, 0.0)',b'safe_cex_num::math_max(free - total, 0.0)','balance::second::route_0::used'),
 (b'info: Some(rows.value.clone()),',b'info: Some(serde_json::json!([])),','balance::distinct::route_0::native_info_count'),
 (b'info: Some(rows.value.clone()),',b'info: Some(Value::Array(values.iter().take(1).cloned().collect())),','balance::second::route_0::native_info_count'),
 (b'info: Some(rows.value.clone()),',b'info: Some(Value::Array(values.iter().chain(values.iter()).cloned().collect())),','balance::distinct::route_0::native_info_count'),
 (b'info: Some(rows.value.clone()),',b'info: Some(Value::Array(values.iter().map(|row| match row { Value::Object(map) => Value::Array(map.iter().map(|(key,value)| Value::Array(vec![Value::String(key.clone()),value.clone()])).collect()), value => value.clone() }).collect())),','balance::distinct::route_0::retained_info_identity'),
]
def fail(label):
 r=subprocess.run(command,capture_output=True,text=True);out=r.stdout+r.stderr
 assert r.returncode==101 and label in out and 'assertion `left == right` failed' in out,out
 print('COMPILED; exit101; '+label,flush=True)
try:
 for before,after,label in controls:
  assert body.count(before)==1,before
  source.write_bytes(prefix+body.replace(before,after));fail(label);source.write_bytes(original)
 for mutation,label in [
  ('cases.splice(0);','balance::source_cases'),('cases.splice(1);','balance::source_cases'),
  ('cases.push(cases[0]);','balance::source_cases'),('cases[1]=cases[0];','balance::source_identity'),
  ('refusals[0]=refusals[1];','balance::refusal_identity')]:
  assert oracle_original.count(b'process.stdout.write(')==1
  oracle.write_bytes(oracle_original.replace(b'process.stdout.write(',mutation.encode()+b'process.stdout.write('));fail(label);oracle.write_bytes(oracle_original)
finally:
 source.write_bytes(original);oracle.write_bytes(oracle_original)
subprocess.run(command,check=True)
print('RESTORED PASS; 10 production + 5 census controls',flush=True)
