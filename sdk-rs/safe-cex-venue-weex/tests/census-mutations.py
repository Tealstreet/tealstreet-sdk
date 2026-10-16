"""Expected-side drops and duplicate controls; oracle inputs remain untouched."""
from pathlib import Path
import os,re,subprocess
crate=Path(__file__).resolve().parent.parent
specs=[('response_types','response','expected','rows'),('utils_oracle','utils','corpus','outcomes'),('numeric_utils','numeric','corpus','cases'),('timestamps','timestamps','corpus','cases'),('orderbook','book','corpus','cases'),('fake_positions','positions','corpus','cases'),('candles','candles','corpus','cases')]
originals={crate/'tests'/f'{target}.rs':(crate/'tests'/f'{target}.rs').read_text() for target,_,_,_ in specs}
controls=[]
for target,name,var,key in specs:
 file=crate/'tests'/f'{target}.rs';s=originals[file]
 prefix=''
 if target=='response_types':
  cut=s.index('fn response_types_match_actual_typescript()');prefix,s=s[:cut],s[cut:]
 anchor=f'    cardinality::check(&{var}, "{name}");'
 assert s.count(anchor)==1
 for n in [0,1]:
  mutation=f'    {var}["{key}"].as_array_mut().unwrap().truncate({n});\n'
  controls.append((file,prefix+s.replace('let '+var+': Value','let mut '+var+': Value').replace(anchor,mutation+anchor),target,f'cardinality::{name}::{key}::source_count'))
 mutation=f'    {var}["{key}"][1] = {var}["{key}"][0].clone();\n'
 controls.append((file,prefix+s.replace('let '+var+': Value','let mut '+var+': Value').replace(anchor,mutation+anchor),target,f'cardinality::{name}::unique_identities'))
file=crate/'tests/utils_oracle.rs';s=originals[file]
for key in ['numbers','refusals','depth','callable']:
 mutation=f'    corpus["{key}"][1] = corpus["{key}"][0].clone();\n'
 mutated=s.replace('let corpus: Value','let mut corpus: Value').replace('    cardinality::check(&corpus, "utils");',mutation+'    cardinality::check(&corpus, "utils");')
 controls.append((file,mutated,'utils_oracle',f'cardinality::utils::{key}::unique_identities'))
file=crate/'tests/fake_positions.rs';s=originals[file]
for n in [0,1]:
 mutated=s.replace('let corpus: Value','let mut corpus: Value').replace('    cardinality::check(&corpus, "positions");',f'    for case in corpus["cases"].as_array_mut().unwrap() {{ case["positions"].as_array_mut().unwrap().truncate({n}); }}\n    cardinality::check(&corpus, "positions");')
 controls.append((file,mutated,'fake_positions','cardinality::positions::source_total'))
 for marker in ['source_total','source_per_case']:
  mutated,count=re.subn(r'assert_eq!\([^;]*?"cardinality::positions::'+marker+r'"\s*\);','let _ = expected_total;' if marker=='source_total' else '',mutated)
  assert count==1,marker
 controls.append((file,mutated,'fake_positions','cardinality::positions::native_length'))
try:
 for file,mutated,target,assertion in controls:
  file.write_text(mutated)
  run=subprocess.run([os.environ.get('CARGO','/opt/homebrew/opt/rustup/bin/cargo'),'test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-weex','--jobs','1','--test',target],capture_output=True,text=True)
  output=run.stdout+run.stderr
  assert run.returncode==101 and assertion in output and 'assertion `left == right` failed' in output,output
  print('COMPILED exit101 '+assertion,flush=True)
  file.write_text(originals[file])
finally:
 for file,original in originals.items():file.write_text(original)
for target,_,_,_ in specs:
 subprocess.run([os.environ.get('CARGO','/opt/homebrew/opt/rustup/bin/cargo'),'test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-weex','--jobs','1','--test',target],check=True)
print(f'Restored PASS after {len(controls)} expected-side cardinality controls',flush=True)
