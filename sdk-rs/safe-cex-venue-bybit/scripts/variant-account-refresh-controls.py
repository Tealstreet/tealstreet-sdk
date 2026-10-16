"""One compiled, named native control per nine new account-refresh rows."""
from pathlib import Path
import json,os,re,subprocess
ROOT=Path(__file__).resolve().parents[3];LOGS=ROOT/'.aimux/bybit-account';LOGS.mkdir(parents=True,exist_ok=True);SOURCE=ROOT/'sdk-rs/safe-cex-venue-bybit/src/variant_account_refresh.rs'
COMMAND=['cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-bybit','--test','variant_account_refresh','--jobs','2'];ENV=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
original=SOURCE.read_text();head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip()
def function(name):
 pattern=re.compile(r'^pub (?:async )?fn '+name+r'\b.*?^}',re.M|re.S);matches=pattern.findall(original);assert len(matches)==1,(name,len(matches));return matches[0]
controls=[]
for variant in ['inverse','spot']:
 before=function(variant+'_account_refresh');needle='refresh(h, owner, '+('true'if variant=='inverse'else'false')+')';after=before.replace(needle,'refresh(h, owner, '+('false'if variant=='inverse'else'true')+')');assert after!=before;controls.append((variant+'-parallel-positions',before,after,variant+'_refresh_native_merge_window_read'))
 before=function(variant+'_private_snapshot');after=before.replace('snapshot(h, owner)','Box::pin(async move { let _retained_owner = owner; undefined(h) })');assert after!=before;controls.append((variant+'-snapshot-wrapper',before,after,variant+'_wrapper_native_call'))
 before=function(variant+'_snapshot_helper');needle='fetch_orders, '+('true'if variant=='inverse'else'false');after=before.replace(needle,'fetch_orders, '+('false'if variant=='inverse'else'true'));assert after!=before;controls.append((variant+'-sequential-positions',before,after,variant+'_helper_native_merge_window_read'))
 before=function(variant+'_tick');after=before.replace('15_000.0','15_001.0');assert after!=before;controls.append((variant+'-tick-reset',before,after,variant+'_tick_native_reset'))
before=function('spot_positions');after=before.replace('h.array(vec![])','h.array(vec![h.literal("unexpected")])');assert after!=before;controls.append(('spot-empty-positions',before,after,'spot_positions_native_empty'))
assert len(controls)==9 and all(original.count(needle)==1 for _,needle,_,_ in controls)
def run(label):
 r=subprocess.run(COMMAND,cwd=ROOT,env=ENV,capture_output=True,text=True);out=r.stdout+r.stderr;(LOGS/f'control-{label}.log').write_text(out);return r.returncode,out
baseline,out=run('baseline');assert baseline==0,out;report=dict(head=head,baseline_exit=baseline,controls=[],survivors=[])
for name,needle,replacement,assertion in controls:
 try:SOURCE.write_text(original.replace(needle,replacement));mutated,out=run(name)
 finally:SOURCE.write_text(original)
 record=dict(control=name,assertion=assertion,mutation_exit=mutated,compiled_runtime_failure=mutated==101 and 'test result: FAILED' in out);report['controls'].append(record)
 if mutated==0:report['survivors'].append(name)
 (LOGS/'controls.json').write_text(json.dumps(report,indent=2)+'\n');assert record['compiled_runtime_failure'] and assertion in out,(name,out)
 restored,out=run(name+'-restored');record['restored_exit']=restored;(LOGS/'controls.json').write_text(json.dumps(report,indent=2)+'\n');assert restored==0,(name,out);assert SOURCE.read_text()==original;print(json.dumps(record),flush=True)
assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip()==head
