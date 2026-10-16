"""Compile/run one complete corpus per invocation; never shell out per case."""
from pathlib import Path
import os, re, subprocess, sys
crate=Path(__file__).resolve().parents[1]
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH')
paths=['src/types.rs','src/constants.rs','src/utilities.rs','src/ticker_reads.rs','tests/cardinality_oracle.cjs','tests/types.rs','tests/support/type_fields.rs']
original={p:(crate/p).read_text() for p in paths}
command=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-phemex','--locked','--jobs','1']
def replace(path,before,after):
 assert original[path].count(before)==1,(path,before)
 return {path:original[path].replace(before,after)}
controls=[]
for count,label in [(0,'empty'),(1,'subset')]:
 controls.append((f'positions {label}','types',replace('src/types.rs','positions: wire.positions,',f'positions: wire.positions.into_iter().take({count}).collect(),'),'PHEMEX-CARDINALITY baseline native positions' if count==0 else 'PHEMEX-CARDINALITY expected/native positions'))
 block=re.search(r'pub const GRAFANA_ENDPOINTS_EVERY: \[&str; 6\] = \[.*?\n\];',original['src/constants.rs'],re.S).group()
 entries=block.splitlines()[1:-1][:count]
 new=f'pub const GRAFANA_ENDPOINTS_EVERY: [&str; {count}] = [\n'+ '\n'.join(entries)+'\n];'
 controls.append((f'constant {label}','constants',replace('src/constants.rs',block,new),'ordered constant GRAFANA_ENDPOINTS_EVERY'))
 old='found.expect("truthy value exists").clone()'
 new=f'if let Some(rows) = found.and_then(serde_json::Value::as_array) {{ serde_json::Value::Array(rows.iter().take({count}).cloned().collect()) }} else {{ {old} }}'
 controls.append((f'utility array {label}','utilities',replace('src/utilities.rs',old,new),'native symbol output first match'))
 old='Ok(data.expect("an explicit null error field requires a present response"))'
 new=f'Ok(Value::Object(data.unwrap().as_object().unwrap().iter().take({count}).map(|(k,v)|(k.clone(),v.clone())).collect()))'
 controls.append((f'ticker object {label}','ticker_reads',replace('src/ticker_reads.rs',old,new),'native ticker result'))
old='process.stdout.write(JSON.stringify({...baseline,supplement}));'
new='supplement[2].expected.pop();\n'+old
mut=replace('tests/cardinality_oracle.cjs',old,new)
controls.append(('source subset count','types',mut,'PHEMEX-CARDINALITY expected position count'))
mut=dict(mut)
mut.update(replace('tests/types.rs','expected.len(),\n            count,\n            "PHEMEX-CARDINALITY expected position count"','count,\n            count,\n            "PHEMEX-CARDINALITY expected position count"'))
controls.append(('source subset bypasses first count','types',mut,'PHEMEX-CARDINALITY expected/native positions'))
# Additions must fail too: an index walk can otherwise ignore native tails.
controls.append(('positions extra','types',replace('src/types.rs','positions: wire.positions,','positions: { let mut rows = wire.positions; if let Some(row) = rows.first().cloned() { rows.push(row); } rows },'),'PHEMEX-CARDINALITY baseline native positions'))
block=re.search(r'pub const GRAFANA_ENDPOINTS_EVERY: \[&str; 6\] = \[.*?\n\];',original['src/constants.rs'],re.S).group()
new=block.replace('[&str; 6]', '[&str; 7]').replace('\n];','\n    "extra",\n];')
controls.append(('constant extra','constants',replace('src/constants.rs',block,new),'ordered constant GRAFANA_ENDPOINTS_EVERY'))
old='found.expect("truthy value exists").clone()'
new=f'if let Some(rows) = found.and_then(serde_json::Value::as_array) {{ let mut rows = rows.clone(); rows.push(serde_json::Value::Null); serde_json::Value::Array(rows) }} else {{ {old} }}'
controls.append(('utility array extra','utilities',replace('src/utilities.rs',old,new),'native symbol output first match'))
old='Ok(data.expect("an explicit null error field requires a present response"))'
new='Ok({ let mut value = data.unwrap(); value.as_object_mut().unwrap().insert("extra".into(), Value::Null); value })'
controls.append(('ticker object extra','ticker_reads',replace('src/ticker_reads.rs',old,new),'native ticker result'))
# Bypass both source-only guards, keeping the independent native guard live.
for method in ['splice(0)', 'pop()']:
 old='process.stdout.write(JSON.stringify({...baseline,supplement}));'
 mut=replace('tests/cardinality_oracle.cjs',old,'supplement[2].expected.'+method+';\n'+old)
 before='expected.len(),\n            count,\n            "PHEMEX-CARDINALITY expected position count"'
 after='count,\n            count,\n            "PHEMEX-CARDINALITY expected position count"'
 mut.update(replace('tests/types.rs',before,after))
 controls.append(('source '+method+' bypass count','types',mut,'PHEMEX-CARDINALITY expected/native positions'))
for count,label in [(0,'empty'),(1,'subset')]:
 mut=replace('src/types.rs','positions: wire.positions,',f'positions: wire.positions.into_iter().take({count}).collect(),')
 for path in ['tests/types.rs','tests/support/type_fields.rs']:
  mut[path]=re.sub(r'assert_eq!\([\s\S]*?\);',lambda m:'if false { '+m[0]+' }' if any(label in m[0] for label in ['baseline native positions','expected/native positions']) else m[0],original[path])
 controls.append((f'consuming {label} bypass native lengths','types',mut,'PHEMEX-CARDINALITY fixed native baseline position' if count==0 else 'PHEMEX-CARDINALITY fixed native supplemental position'))
assert len(controls)==18, 'PHEMEX-CONTROLS full inventory'
expected_count=2 if '--consuming-only' in sys.argv else 18
selected=[row for row in controls if '--consuming-only' not in sys.argv or row[0].startswith('consuming ')]
assert len(selected)==expected_count, 'PHEMEX-CONTROLS selected inventory'
completed=0
for name,target,mut,assertion in selected:
 try:
  for path,text in mut.items():(crate/path).write_text(text)
  result=subprocess.run(command+['--test',target],capture_output=True,text=True)
  evidence=result.stdout+result.stderr
  assert result.returncode==101 and 'running 1 test' in evidence and 'test result: FAILED' in evidence and assertion in evidence,(name,evidence)
  print(f'COMPILED FAIL 101: {name}: {assertion}',flush=True)
 finally:
  for path,text in original.items():(crate/path).write_text(text)
 restored=subprocess.run(command+['--test',target],capture_output=True,text=True)
 assert restored.returncode==0,(name,restored.stdout+restored.stderr)
 print(f'PASS restored: {name}',flush=True)

 completed+=1
assert completed==expected_count, 'PHEMEX-CONTROLS executed inventory'
