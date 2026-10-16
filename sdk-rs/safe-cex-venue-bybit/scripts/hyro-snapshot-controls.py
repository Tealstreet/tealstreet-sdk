"""One compiled, named native control per eight new HyroTrader snapshot rows."""
from pathlib import Path
import json,os,re,subprocess
ROOT=Path(__file__).resolve().parents[3];LOGS=ROOT/'.aimux/bybit-snapshot';LOGS.mkdir(parents=True,exist_ok=True);SOURCE=ROOT/'sdk-rs/safe-cex-venue-bybit/src/hyro_snapshot.rs'
COMMAND=['cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-bybit','--test','hyro_snapshot','--jobs','2'];ENV=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
original=SOURCE.read_text();head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip()
def function(name):
 matches=re.findall(r'^pub fn '+name+r'\b.*?^}',original,re.M|re.S);assert len(matches)==1,(name,len(matches));return matches[0]
controls=[]
recipes=[('reconcile_private_state','Ok(h.boolean(true))','Ok(h.boolean(false))','snapshot_native_publication_result'),('fetch_and_set_private_snapshot','snapshot(h, owner)','fetch_markets(h, owner)','snapshot_wrapper_native_call'),('fetch_orders_positions_and_balance','snapshot(h, owner)','fetch_markets(h, owner)','account_wrapper_native_call'),('fetch_and_set_private_snapshot_helper','!h.truthy(&h.get(&h.get(&owner, "privateWebsocket")?, "ready")?)','h.truthy(&h.get(&h.get(&owner, "privateWebsocket")?, "ready")?)','snapshot_helper_native_call'),('fetch_markets','"market-usdt"','"market-entry"','markets_native_count'),('get_bybit_rest_base_url','"HYROTRADER_SIMULATOR_URL"','"HYROTRADER_SIMULATOR_WS_URL"','rest_url_native_value'),('get_bybit_public_ws_url','h.literal("linear")','h.literal("inverse")','public_url_native_value'),('get_bybit_private_ws_url','"private"','"public"','private_url_native_value')]
for name,needle,replacement,assertion in recipes:
 before=function(name);assert before.count(needle)==1,(name,needle);after=before.replace(needle,replacement);controls.append((name,before,after,assertion))
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
