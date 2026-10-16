"""Run the two literal controls of stored bound-arrow values, serially."""
from pathlib import Path
import os,subprocess,time,json,hashlib,argparse
parser=argparse.ArgumentParser();parser.add_argument('--out-dir',required=True,type=Path);args=parser.parse_args()
args.out_dir=args.out_dir.resolve();args.out_dir.mkdir(parents=True,exist_ok=True)
# Repository-local when installed in the package scripts directory.
if Path(__file__).parent.name=='scripts':os.chdir(Path(__file__).resolve().parents[3])
revision=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()
assert not subprocess.check_output(['git','diff','--name-only'],text=True).strip()
path=Path('sdk-rs/safe-cex-venue-coinbase/src/class_methods.rs');original=path.read_bytes()
controls=[
 ('foreign-origin-ignored','receiver: self.origin.clone().unwrap_or(owner),','receiver: { let _ = &self.origin; owner },','arrow_value_',[
  ('arrow_value_transplanted_supported_arrow_retains_origin','native transplanted supported arrow retains original receiver'),
  ('arrow_value_transplanted_finder_retains_origin','native transplanted finder reads original receiver HTTP leaves'),
  ('arrow_value_transplanted_arrow_retains_live_origin_option_slot','native transplanted arrow rereads restored origin options'),
  ('arrow_value_reassigned_arrow_keeps_old_code_and_origin','native reassigned arrow retains original code and receiver')]),
 ('redundant-self-edge-retained','let origin = (!Rc::ptr_eq(&value.receiver, owner)).then_some(value.receiver);','let _ = owner; let origin = Some(value.receiver);','arrow_value_transplanted_supported_arrow_retains_origin',[
  ('arrow_value_transplanted_supported_arrow_retains_origin','native self assignment releases target without a redundant receiver edge')]),
]
assert [r[0]for r in controls]==['foreign-origin-ignored','redundant-self-edge-retained']
rows=[];env=dict(os.environ,CARGO_INCREMENTAL='0',CARGO_BUILD_JOBS='2')
sha=lambda value:hashlib.sha256(value).hexdigest()
for name,old,new,selector,assertions in controls:
 assert subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()==revision
 assert original.decode().count(old)==1,(name,'anchor count')
 mutant=original.decode().replace(old,new).encode()
 command=['cargo','test','--manifest-path','sdk-rs/Cargo.toml','--locked','--jobs','2','-p','safe-cex-venue-coinbase','--test','completion',selector,'--','--nocapture','--test-threads=1']
 def run(label):
  start=time.time();log=args.out_dir/(name+'-'+label+'.log')
  with log.open('wb')as output:p=subprocess.run(command,env=env,stdout=output,stderr=subprocess.STDOUT)
  return {'exit':p.returncode,'elapsed_seconds':round(time.time()-start,2)},log.read_text(errors='replace')
 try:
  path.write_bytes(mutant);negative,text=run('negative')
 finally:path.write_bytes(original)
 restored,restored_text=run('restored')
 named={test:assertion in text and test in text.split('failures:')[-1] for test,assertion in assertions}
 n=len(assertions)
 caught=negative['exit']==101 and all(named.values()) and ('test result: FAILED. 0 passed; '+str(n)+' failed;')in text
 passed=restored['exit']==0 and ('test result: ok. '+str(n)+' passed;')in restored_text
 rows.append({'name':name,'code_revision':revision,'call_site':str(path),'command':command,'native_assertions':[{'test':t,'assertion':a}for t,a in assertions],'named_failures':named,'negative':negative,'restored':restored,'caught':caught,'restored_named_floor':passed,'original_sha256':sha(original),'mutant_sha256':sha(mutant),'restored_sha256':sha(path.read_bytes())})
 receipt={'code_revision':revision,'independent_control_ids':['foreign-origin-ignored','redundant-self-edge-retained'],'records':rows,'survivors':[r['name']for r in rows if not r['caught']],'iteration':'Stopped at two literal controls: foreign receiver rebinding and redundant self edge. No exhaustive claim.'}
 (args.out_dir/'controls.json').write_text(json.dumps(receipt,indent=2)+'\n')
 print(name,negative['exit'],restored['exit'],named,flush=True)
 assert path.read_bytes()==original
 if not caught or not passed:raise SystemExit(1)
