"""One compiled, named native control per ten new HyroTrader capability rows."""
from pathlib import Path
import json,os,re,subprocess
ROOT=Path(__file__).resolve().parents[3];LOGS=ROOT/'.aimux/bybit-capabilities';LOGS.mkdir(parents=True,exist_ok=True);SOURCE=ROOT/'sdk-rs/safe-cex-venue-bybit/src/hyro_capabilities.rs'
COMMAND=['cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-bybit','--test','hyro_capabilities','--jobs','2'];ENV=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
original=SOURCE.read_text();head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip()
def function(name):
 matches=re.findall(r'^pub fn '+name+r'\b.*?^}',original,re.M|re.S);assert len(matches)==1,(name,len(matches));return matches[0]
controls=[]
recipes=[('fail','Err(h.error(message)?)','Ok(h.error_value(h.error(message)?))','fail_native_error'),('fetch_and_cache_api_key_info','Ok(h.undefined())','Ok(h.boolean(true))','cache_native_undefined'),('fetch_api_key_info','HyroTrader does not provide API-key metadata','HyroTrader does not provide API-key metadata changed','api_native_error'),('get_account','HyroTrader does not provide account identity metadata','HyroTrader does not provide account identity metadata changed','account_native_error'),('fetch_fees','h.array(vec![])','h.array(vec![h.boolean(true)])','fees_native_empty'),('fetch_position_mode','h.literal("oneway")','h.literal("hedged")','mode_native_oneway'),('change_position_mode','HyroTrader supports one-way position mode only','HyroTrader supports one-way position mode only changed','change_position_native_error'),('change_margin_type','HyroTrader supports cross margin only','HyroTrader supports cross margin only changed','change_margin_native_error'),('assert_private_ready','HyroTrader account is reconnecting; wait for private state reconciliation before placing orders','HyroTrader account is reconnecting changed','ready_native_error'),('validate_order_capabilities','HyroTrader does not support client-defined slippage limits; protection uses the simulator-configured gate','HyroTrader does not support client-defined slippage limits changed','capabilities_native_slippage')]
for name,needle,replacement,assertion in recipes:
 before=function(name);assert before.count(needle)==1,(name,needle);after=before.replace(needle,replacement);controls.append((name,before,after,assertion))
assert len(controls)==10 and all(original.count(needle)==1 for _,needle,_,_ in controls)
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
