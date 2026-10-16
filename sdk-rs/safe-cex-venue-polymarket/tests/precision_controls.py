"""Precision-row runtime controls; one actual-TS corpus and serial Cargo."""
from pathlib import Path
import json, os, subprocess
crate=Path(__file__).resolve().parents[1]
rust=crate/'src/utilities.rs';test=crate/'tests/utilities.rs';oracle=crate/'tests/utilities_oracle.cjs'
logs=Path(os.environ['POLYMARKET_CONTROL_LOG_DIR']);logs.mkdir(parents=True,exist_ok=True)
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH')
def replace(old,new):
 def change(s):
  assert s.count(old)==1,old
  return s.replace(old,new)
 return change
controls=[
 ('precision-fallback','precision_native_value:6',[(rust,replace('return 2.0;', 'return 3.0;'))]),
 ('precision-seven-digits','precision_native_value:',[(rust,replace('{tick:.8}','{tick:.7}'))]),
 ('precision-exponent-branch','precision_native_value:18',[(rust,replace('tick >= 1e21','tick >= f64::MAX'))]),
 ('precision-no-zero-trim','precision_native_value:0',[(rust,replace("text.trim_end_matches('0')",'text.as_str()'))]),
 ('precision-source-subset','precision_source_case_count',[(oracle,replace('process.stdout.write(', 'precisionCases.pop();\nprocess.stdout.write('))]),
 ('precision-source-bypass','precision_total_native_comparisons',[
  (oracle,replace('process.stdout.write(', 'precisionCases.splice(0, precisionCases.length, precisionCases[38], precisionCases[39]);\nprocess.stdout.write(')),
  (test,replace('    assert_eq!(precision_cases.len(), 3565, "precision_source_case_count");\n','')),
 ]),
]
command=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-polymarket','--test','utilities','--locked','--jobs','1','--','--nocapture']
def run(name):
 r=subprocess.run(command,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
 (logs/(name+'.log')).write_text(f'exit_code={r.returncode}\n'+r.stdout)
 assert r.stdout.count('polymarket utilities oracle: one Node invocation for the entire corpus')==1,r.stdout
 return r
assert run('precision-baseline').returncode==0
summary={'sha':subprocess.check_output(['git','rev-parse','HEAD'],cwd=crate,text=True).strip(),'controls':[]}
for name,assertion,changes in controls:
 originals={p:p.read_text() for p,_ in changes}
 try:
  for p,change in changes:p.write_text(change(originals[p]))
  r=run(name+'-mutation');assert r.returncode==101 and assertion in r.stdout,r.stdout
 finally:
  for p,s in originals.items():p.write_text(s)
 restored=run(name+'-restored');assert restored.returncode==0,restored.stdout
 summary['controls'].append({'name':name,'mutation_exit':r.returncode,'assertion':assertion,'restored_exit':restored.returncode})
 (logs/'summary.json').write_text(json.dumps(summary,indent=2)+'\n')
 print(f'{name}: 101 {assertion}; restored 0',flush=True)
