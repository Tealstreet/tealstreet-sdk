"""Per-row and collection-cardinality controls. One Node corpus per target run."""
from pathlib import Path
import re
import subprocess
root=Path(__file__).resolve().parents[3]
crate=root/'sdk-rs/safe-cex-venue-bitunix'
source=crate/'src/row_extraction.rs';test=crate/'tests/row_extraction.rs'
original={p:p.read_text() for p in [source,test]}
logs=root/'sdk-rs/target/bitunix-row-extraction-controls';logs.mkdir(exist_ok=True)
controls=[
 ('first-false-skipped',{source:original[source].replace('!value.is_null()','js_truthy(Some(value))')},'FIRST native selected value'),
 ('account-first-over-preferred',{source:original[source].replace('preferred.or_else(|| rows.first().copied())','rows.first().copied().or(preferred)')},'ACCOUNT native selected row'),
 ('position-symbol-only',{source:original[source].replace('&["symbol", "instId", "contract", "instrument"]','&["symbol"]')},'POSITIONS independent native length'),
]
bypass=original[test]
for label in ['FIRST source total','ROWS source total','ROWS source position count {index}']:
 pattern=r'    +assert_eq!\(\n.*?"'+re.escape(label)+r'"\n    +\);'
 # Bound the match at the nearest assertion; never remove intervening code.
 pattern=r'    +assert_eq!\(\n(?:(?!assert_eq!).)*?"'+re.escape(label)+r'"\n    +\);'
 bypass,n=re.subn(pattern,'',bypass,flags=re.S);assert n==1,label
for name,body in [('empty','return Vec::new();'),('subset','return rows.into_iter().take(1).collect();'),('extra','let mut rows = rows; rows.push(rows[0]); return rows;')]:
 controls.append(('positions-'+name+'-source-bypassed',{source:original[source].replace('return rows;',body),test:bypass},'POSITIONS independent native length'))
for key,label in [('first','FIRST native comparisons'),('rows','ACCOUNT native comparisons')]:
 changed=bypass.replace('let corpus: Value','let mut corpus: Value')
 changed=changed.replace('    let mut first_count', '    corpus["'+key+'"] = json!([]);\n    let mut first_count')
 controls.append(('source-'+key+'-empty',{test:changed},label))
command=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(root/'sdk-rs/Cargo.toml'),'-p','safe-cex-venue-bitunix','--locked','--jobs','1','--test','row_extraction','--','--nocapture']
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
