"""Per-row production and independent per-helper population controls."""
from pathlib import Path
import re
import subprocess
root=Path(__file__).resolve().parents[3];crate=root/'sdk-rs/safe-cex-venue-bitunix'
source=crate/'src/tpsl_helpers.rs';test=crate/'tests/tpsl_helpers.rs'
original={p:p.read_text() for p in [source,test]}
logs=root/'sdk-rs/target/bitunix-tpsl-helper-controls';logs.mkdir(exist_ok=True)
controls=[
 ('leg-separator',{source:original[source].replace('{order_id}__{position_part}__{leg}','{order_id}_{position_part}__{leg}')},'TPSL native leg ID'),
 ('last-as-mark',{source:original[source].replace('if raw == "LAST" || raw == "LAST_PRICE" {\n        return Ok("LAST_PRICE");','if raw == "LAST" || raw == "LAST_PRICE" {\n        return Ok("MARK_PRICE");')},'TPSL native trigger'),
 ('cache-false-lost',{source:original[source].replace('cached.unwrap_or(fallback)','cached.filter(|value| *value).unwrap_or(fallback)')},'TPSL native reduce-only'),
]
bypass=original[test].replace('let corpus: Value','let mut corpus: Value')
for label in ['TPSL ID source total','TPSL trigger source total','TPSL reduce source total']:
 pattern=r'    +assert_eq!\(\n(?:(?!assert_eq!).)*?"'+re.escape(label)+r'"\n    +\);'
 bypass,n=re.subn(pattern,'',bypass,flags=re.S);assert n==1,label
for group in ['ids','triggers','reduce']:
 changed=bypass.replace('    let cache:', '    corpus["'+group+'"] = json!([]);\n    let cache:')
 controls.append(('source-'+group+'-empty',{test:changed},'TPSL independent native counts'))
for name,statement in [('subset','corpus["ids"].as_array_mut().unwrap().remove(0);'),('extra','let extra=corpus["ids"][0].clone(); corpus["ids"].as_array_mut().unwrap().push(extra);')]:
 controls.append(('source-ids-'+name,{test:bypass.replace('    let cache:', '    '+statement+'\n    let cache:')},'TPSL independent native counts'))
command=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(root/'sdk-rs/Cargo.toml'),'-p','safe-cex-venue-bitunix','--locked','--jobs','1','--test','tpsl_helpers','--','--nocapture']
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
