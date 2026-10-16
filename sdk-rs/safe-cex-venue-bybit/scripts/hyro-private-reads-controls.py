"""One compiled native value/effect control per eight new private-read rows."""
from pathlib import Path
import json,os,re,subprocess
ROOT=Path(__file__).resolve().parents[3];LOGS=ROOT/'.aimux/bybit-private-reads';LOGS.mkdir(parents=True,exist_ok=True);SOURCE=ROOT/'sdk-rs/safe-cex-venue-bybit/src/hyro_private_reads.rs'
COMMAND=['cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-bybit','--test','hyro_private_reads','--jobs','2'];ENV=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
original=SOURCE.read_text();head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip()
def function(name):
 matches=re.findall(r'^pub fn '+name+r'\b.*?^}',original,re.M|re.S);assert len(matches)==1,(name,len(matches));return matches[0]
recipes=[('fetch_raw_balance','h.wait(h.get(&h.get(&data, "result")?, "list")?).await','h.wait(h.array(vec![])).await','wallet_native_list_identity'),('assert_private_list_response','Ok(h.undefined())','Ok(h.boolean(false))','list_native_undefined'),('map_position','Ok(mapped)','Ok(h.spread(mapped, vec![])?)','position_native_identity'),('fetch_orders','h.literal("linear")','h.literal("inverse")','orders_native_category'),('fetch_private_rows','for page in 0..100 {','for page in 0..99 {','pages_native_request_floor'),('fetch_orders_helper','let result = call!(h, owner, "mapValidatedOrders", vec![rows]);','let _retained_owner = owner; let result = rows;','native_mapped_order_aliases'),('map_validated_orders','Ok(orders)','Ok(h.array(vec![]))','fetchOrders_native_order_count'),('fetch_positions_helper','let result = h.apply(dedupe, vec![positions])?;','let _retained_dedupe = dedupe; let result = positions;','positions_helper_native_dedupe_call')]
controls=[]
for name,needle,replacement,assertion in recipes:
 before=function(name);assert before.count(needle)==1,(name,needle);after=before.replace(needle,replacement)
 if name=='fetch_private_rows':
  assert before.count('if page != 99 {')==1;after=after.replace('if page != 99 {','if page != 98 {')
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
