"""Utility-row field, branch and consuming-cardinality controls; serial --jobs 1."""
from pathlib import Path
import json, os, re, subprocess
crate=Path(__file__).resolve().parents[1]
rust=crate/'src/utilities.rs'; harness=crate/'tests/utilities.rs'; oracle=crate/'tests/utilities_oracle.cjs'
logs=Path(os.environ['POLYMARKET_CONTROL_LOG_DIR']);logs.mkdir(parents=True,exist_ok=True)
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH')
def replace(a,b):
 def change(s):
  assert s.count(a)==1,a
  return s.replace(a,b)
 return change

def swallow(s):
 for a,b in [('Some(value.to_owned())','Some(value.to_owned()).filter(|_| false)'),('Some(value)))','Some(value).filter(|_| false)))')]:
  assert s.count(a)==1,a
  s=s.replace(a,b)
 return s

def bypass(s):
 a='        assert_eq!(native.len(), expected.len(), "tokens_native_length:{index}");\n'
 assert s.count(a)==1
 s=s.replace(a,'')
 s,n=re.subn(r'        assert_eq!\(\n            expected.len\(\),\n            expected_counts\[index\],\n            "tokens_source_case_outputs:\{index\}"\n        \);\n','',s)
 assert n==1
 return s
controls=[
 ('tick-supported-membership','tick_native_success:3',[(rust,replace('"0.0025"','"0.025"'))]),
 ('tick-string-trim','tick_native_expected_error',[(rust,replace('value.clone()','value.trim().to_owned()'))]),
 ('tick-number-sign','tick_native_error_message:9',[(rust,replace('js_number_string(*value)','js_number_string(value.abs())'))]),
 ('token-pair-swap','tokens_native_token_id:3',[(rust,replace('=> Some((token_id, outcome)),','=> Some((outcome, token_id)),'))]),
 ('token-length-mismatch','tokens_native_length:6',[(rust,replace('token_ids.len() != outcomes.len()','token_ids.len() == usize::MAX'))]),
 ('token-consuming-string-loss','tokens_native_length:3',[(rust,swallow)]),
 ('token-output-subset','tokens_native_length:4',[(rust,replace('        .collect()','        .take(1).collect()'))]),
 ('token-consuming-bypass-lengths','tokens_case_comparisons:3',[(rust,swallow),(harness,bypass)]),
 ('token-source-subset','tokens_source_case_count',[(oracle,replace('process.stdout.write(', 'tokenCases.pop();\nprocess.stdout.write('))]),
]
command=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-polymarket','--test','utilities','--locked','--jobs','1','--','--nocapture']
def run(name):
 r=subprocess.run(command,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
 (logs/(name+'.log')).write_text(f'exit_code={r.returncode}\n'+r.stdout)
 assert r.stdout.count('polymarket utilities oracle: one Node invocation for the entire corpus')==1,r.stdout
 return r
assert run('utilities-baseline').returncode==0
summary={'sha':subprocess.check_output(['git','rev-parse','HEAD'],cwd=crate,text=True).strip(),'controls':[]}
for name,assertion,changes in controls:
 originals={p:p.read_text() for p,_ in changes}
 try:
  for p,change in changes:
   updated=change(originals[p]);assert updated!=originals[p],name;p.write_text(updated)
  r=run(name+'-mutation');assert r.returncode==101 and assertion in r.stdout,r.stdout
 finally:
  for p,s in originals.items():p.write_text(s)
 restored=run(name+'-restored');assert restored.returncode==0,restored.stdout
 summary['controls'].append({'name':name,'mutation_exit':r.returncode,'assertion':assertion,'restored_exit':restored.returncode})
 (logs/'summary.json').write_text(json.dumps(summary,indent=2)+'\n')
 print(f'{name}: 101 {assertion}; restored 0',flush=True)
