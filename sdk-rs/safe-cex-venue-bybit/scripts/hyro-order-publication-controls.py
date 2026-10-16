"""One compiled, named native control per eight new HyroTrader order-publication rows."""
from pathlib import Path
import json,os,re,subprocess
ROOT=Path(__file__).resolve().parents[3];LOGS=ROOT/'.aimux/bybit-order-publication';LOGS.mkdir(parents=True,exist_ok=True);SOURCE=ROOT/'sdk-rs/safe-cex-venue-bybit/src/hyro_order_publication.rs'
COMMAND=['cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-bybit','--test','hyro_order_publication','--jobs','2'];ENV=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
original=SOURCE.read_text();head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip()
def function(name):
 matches=re.findall(r'^pub fn '+name+r'\b.*?^}',original,re.M|re.S);assert len(matches)==1,(name,len(matches));return matches[0]
controls=[]
recipes=[('observe_orders','if h.truthy(&h.apply(some, vec![callback])?)','if !h.truthy(&h.apply(some, vec![callback])?)','observe_native_linkset'),('dispose','"clear"','"values"','dispose_native_pending_cleared'),('unknown','HyroTrader order outcome is unknown (orderLinkId ','HyroTrader changed outcome (orderLinkId ','unknown_native_message'),('run_group','let seen = h.apply(observed, vec![link])?;','let seen = { h.apply(observed, vec![link])?; h.boolean(false) };','group_native_authoritative_seen'),('find_order','for page in 0..100','for page in 0..99','find_native_request_floor'),('observe_private_orders','if string(h, h.get(&row, "orderId")?)?','if !string(h, h.get(&row, "orderId")?)?','private_observe_native_ids'),('submit_create_orders','"accepted-order"','"existing-order"','publication_native_recorded'),('cancel_orders_helper','h.array(retiring)','h.array(vec![retiring[0].clone()])','cancel_native_original_aliases')]
for name,needle,replacement,assertion in recipes:
 before=function(name);assert before.count(needle)==1,(name,needle);after=before.replace(needle,replacement);
 if name=='find_order':assert before.count('page == 99')==1;after=after.replace('page == 99','page == 98')
 controls.append((name,before,after,assertion))
assert len(controls)==8 and all(original.count(needle)==1 for _,needle,_,_ in controls)
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
