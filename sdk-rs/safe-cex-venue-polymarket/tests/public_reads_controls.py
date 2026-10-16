"""Public-read row runtime controls; one actual-TS corpus and serial Cargo."""
from pathlib import Path
import json, os, subprocess
crate=Path(__file__).resolve().parents[1]
rust=crate/'src/public_reads.rs';test=crate/'tests/public_reads.rs';oracle=crate/'tests/public_reads_oracle.cjs'
logs=Path(os.environ['POLYMARKET_CONTROL_LOG_DIR']);logs.mkdir(parents=True,exist_ok=True)
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH')
def replace(old,new):
 def change(s):
  assert s.count(old)==1,old
  return s.replace(old,new)
 return change
controls=[
 ('time-endpoint','time_native_url:0',[(rust,replace('*key == "TIME"','*key == "MARKETS"'))]),
 ('time-method','time_native_method:0',[(rust,replace('method: "get".into(),\n        url: crate::constants', 'method: "post".into(),\n        url: crate::constants'))]),
 ('time-zero-sign','time_native_bits:1',[(rust,replace('    read(client, request).await', '    read(client, request).await.map(f64::abs)'))]),
 ('time-failure-context','time_failure_metadata:0',[(rust,replace('client.execute(request.clone()).await?', 'client.execute(request.clone()).await.map_err(|mut error| { error.context = None; error })?'))]),
 ('time-source-empty','time_source_case_count',[(oracle,replace(' process.stdout.write(', ' cases.splice(0);\n process.stdout.write('))]),
 ('time-count-bypass','time_source_case_present',[
  (oracle,replace(' process.stdout.write(', ' cases.splice(0);\n process.stdout.write(')),
  (test,replace('    assert_eq!(cases.len(), 20, "time_source_case_count");\n','')),
 ]),
]
command=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-polymarket','--test','public_reads','--locked','--jobs','1','--','--nocapture']
def run(name):
 r=subprocess.run(command,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
 (logs/(name+'.log')).write_text(f'exit_code={r.returncode}\n'+r.stdout)
 assert r.stdout.count('polymarket public reads oracle: one Node invocation for the entire corpus')==1,r.stdout
 return r
assert run('time-baseline').returncode==0
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
