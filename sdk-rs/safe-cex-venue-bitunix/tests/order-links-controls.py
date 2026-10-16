"""Production helper mutations and explicit post-decode parent-input controls."""
from pathlib import Path
import re
import subprocess
root=Path(__file__).resolve().parents[3];crate=root/'sdk-rs/safe-cex-venue-bitunix'
source=crate/'src/order_links.rs';test=crate/'tests/order_links.rs'
original={p:p.read_text() for p in [source,test]}
logs=root/'sdk-rs/target/bitunix-order-link-controls';logs.mkdir(exist_ok=True)
controls=[
 ('raw-empty-nullish',{source:original[source].replace('.filter(|v| !v.is_null())','.filter(|v| !v.is_null() && v.as_str() != Some(""))')},'ORDER LINK native raw ID'),
 ('attached-last-parent',{source:original[source].replace('orders.iter().find','orders.iter().rev().find')},'ORDER LINK native attached'),
 ('standalone-inverted',{source:original[source].replace('get_attached_virtual_tpsl_leg(order, orders).is_none()','get_attached_virtual_tpsl_leg(order, orders).is_some()')},'ORDER LINK native standalone'),
 ('position-amount-filled-swap',{source:original[source].replace('order.amount == 0.0','order.filled == 0.0')},'ORDER LINK native position'),
]
bypass=original[test].replace('    assert_eq!(rows.len(), 250, "ORDER LINK source total");','')
bypass,n=re.subn(r'        assert_eq!\(\n            row\["orders"\].*?"ORDER LINK source parent count \{index\}"\n        \);','',bypass,flags=re.S);assert n==1
for mode,operation in [('empty','parents.clear();'),('subset','parents.truncate(1);'),('extra','if let Some(first)=parents.first().cloned() {parents.push(first);}')]:
 # Input is fully decoded before removal; these are TEST input controls, not
 # a claim that the shared production decoder was mutated.
 changed=bypass.replace('let parents: Vec<Order>', 'let mut parents: Vec<Order>').replace('        assert_eq!(\n            parents.len(),','        '+operation+'\n        assert_eq!(\n            parents.len(),')
 controls.append(('post-decode-parent-'+mode,{test:changed},'ORDER LINK independent native parent length'))
changed=bypass.replace('let rows: Vec<Value>', 'let mut rows: Vec<Value>').replace('    let mut compared', '    rows.clear();\n    let mut compared')
controls.append(('source-empty-count-bypassed',{test:changed},'ORDER LINK independent native comparisons'))
command=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(root/'sdk-rs/Cargo.toml'),'-p','safe-cex-venue-bitunix','--locked','--jobs','1','--test','order_links','--','--nocapture']
for name,changes,assertion in controls:
 try:
  for p,changed in changes.items():assert changed!=original[p],name;p.write_text(changed)
  r=subprocess.run(command,text=True,capture_output=True);(logs/(name+'-fail.log')).write_text(r.stdout+r.stderr)
  assert r.returncode==101 and 'Finished `test`' in r.stderr and assertion in r.stdout+r.stderr,(name,r.returncode,r.stdout+r.stderr)
  print('FAIL compiled 101: '+name+' / '+assertion,flush=True)
 finally:
  for p in changes:p.write_text(original[p])
 r=subprocess.run(command,text=True,capture_output=True);(logs/(name+'-pass.log')).write_text(r.stdout+r.stderr)
 assert r.returncode==0,(name,r.stdout+r.stderr)
 print('PASS restored: '+name,flush=True)
print('controls proved:',len(controls))
