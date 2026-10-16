#!/usr/bin/env python3.11
"""Compile exact production cross_trade bodies against the measured Cargo dependencies.

The Cargo suite tests the real public library first. This Rustc control harness
includes those same production files directly, allowing bounded re-compilation
without rebuilding the entire BaseExchange for each small mutation. Shared
runtime/types/num and the canonical order constructor are the measured rlibs.
"""
from pathlib import Path
import argparse,subprocess,json,os,hashlib
parser=argparse.ArgumentParser();parser.add_argument('--artifacts',required=True);args=parser.parse_args()
root=Path(__file__).resolve().parents[3];crate=root/'sdk-rs/safe-cex-exchange';sha=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip()
out=root/'sdk-rs/target/cross-trade-controls';out.mkdir(parents=True,exist_ok=True)
artifacts=json.loads(Path(args.artifacts).read_text());assert artifacts['gate_sha']==sha
needed=['safe_cex_exchange','safe_cex_types','safe_cex_num','serde','serde_json'];libraries=artifacts['libraries'];assert all(name in libraries for name in needed)
# Cargo emits host/build dependency artifacts as well as target artifacts. Do
# not select Serde by emission order: prove it implements the actual types ABI.
probe=out/'serde-abi.rs';probe.write_text('fn main(){fn check<T:serde::Serialize+serde::de::DeserializeOwned>(){}check::<safe_cex_types::Market>();let _=serde_json::to_value::<Option<safe_cex_types::Market>>(None).unwrap();}')
serde_candidates=artifacts.get('candidates',{}).get('serde',[libraries['serde']])
abi=[]
for index,candidate in enumerate(serde_candidates):
 cmd=['rustc','--edition=2024',str(probe),'--crate-name','serde_abi','-L','dependency='+str(Path(libraries['safe_cex_exchange']).parent),'-o',str(out/'serde-abi')]
 for name,file in [('serde',candidate),('safe_cex_types',libraries['safe_cex_types']),('serde_json',libraries['serde_json'])]:cmd+=['--extern',name+'='+file]
 result=subprocess.run(cmd,cwd=root,capture_output=True,text=True);(out/('serde-abi-'+str(index)+'.log')).write_text(result.stdout+result.stderr)
 abi.append({'candidate':candidate,'exit':result.returncode})
 if result.returncode==0:libraries['serde']=candidate;break
else:raise SystemExit('No measured Serde artifact matches the shared native types ABI')
(out/'serde-abi.json').write_text(json.dumps({'gate_sha':sha,'probes':abi,'selected':libraries['serde']},indent=2))
source_test=(crate/'tests/cross_trade.rs').read_text();assert source_test.count('use safe_cex_exchange::{')==1
harness='pub use safe_cex_exchange::{runtime,order_operations};\n#[path='+json.dumps(str(crate/'src/cross_trade/mod.rs'))+'] mod cross_trade;\n'+source_test.replace('use safe_cex_exchange::{','use crate::{',1)
(out/'harness.rs').write_text(harness);binary=out/'cross-trade-control'
command=['rustc','--edition=2024','--test',str(out/'harness.rs'),'--crate-name','cross_trade_control','-L','dependency='+str(Path(libraries['safe_cex_exchange']).parent),'-o',str(binary)]
for name in needed:command+=['--extern',name+'='+libraries[name]]
env=dict(os.environ,CARGO_MANIFEST_DIR=str(crate));env.setdefault('SAFE_CEX_TYPESCRIPT_PATH','/Users/sam/cs/tealstreet-next/node_modules/typescript')
results=[]
def compile_and_run(label,test=None):
 compile_result=subprocess.run(command,cwd=root,env=env,capture_output=True,text=True);(out/(label+'-compile.log')).write_text(compile_result.stdout+compile_result.stderr)
 if compile_result.returncode:raise SystemExit(label+' did not compile:\n'+compile_result.stderr)
 run=[str(binary)]+([test,'--exact']if test else[]);result=subprocess.run(run,cwd=root,env=env,capture_output=True,text=True);output=result.stdout+result.stderr;(out/(label+'.log')).write_text(output)
 return result.returncode,output
baseline,output=compile_and_run('baseline');assert baseline==0 and '6 passed; 0 failed' in output
(out/'baseline.json').write_text(json.dumps({'gate_sha':sha,'rustc_command':command,'compile_exit':0,'test_exit':baseline,'native_tests':6,'dependency_sha256':{name:hashlib.sha256(Path(libraries[name]).read_bytes()).hexdigest()for name in needed}},indent=2))
print('Exact-body Rustc baseline: compile0, six native tests0',flush=True)
controls=[
 ('weight-mode','types.rs','    Balance,\n    Absolute,','    #[serde(rename="Absolute")]\n    Balance,\n    #[serde(rename="Balance")]\n    Absolute,','native_defaults_enum_tags_mapping_and_weight_values','CROSS native weight-mode variants'),
 ('execution-mode','types.rs','    Market,\n    Chase,','    #[serde(rename="Chase")]\n    Market,\n    #[serde(rename="Market")]\n    Chase,','native_defaults_enum_tags_mapping_and_weight_values','CROSS native execution-mode variants'),
 ('literal-key','manager.rs','format!("{account}:{symbol}")','format!("{account}|{symbol}")','native_failure_muting_expiry_minimum_and_late_completion','CROSS native literal failure key'),
 ('clear-success','manager.rs','            .remove(&Self::failure_key(account, symbol));','            .get(&Self::failure_key(account, symbol));','native_failure_muting_expiry_minimum_and_late_completion','CROSS native failure entry presence'),
 ('minimum-absolute','manager.rs','amount.abs() >= minimum','amount >= minimum','native_failure_muting_expiry_minimum_and_late_completion','CROSS native minimum amount predicate'),
 ('chase-dedup','place_order.rs','if active.len() > 1','if active.len() >= 1','native_execution_events_prices_errors_and_opaque_identity','CROSS native eager execution events'),
 ('resize-threshold','manager.rs','(difference / position.contracts).abs() > 0.05','(difference / position.contracts).abs() > 0.0','native_position_audits_match_source_requests_before_polling','CROSS native eager audit calls'),
]
for label,file,needle,mutation,test,assertion in controls:
 path=crate/'src/cross_trade'/file;original=path.read_bytes();source=original.decode();assert source.count(needle)==1,(label,source.count(needle));assert original==subprocess.check_output(['git','show',sha+':'+path.relative_to(root).as_posix()],cwd=root)
 try:path.write_text(source.replace(needle,mutation));broken,output=compile_and_run(label+'-broken',test)
 finally:path.write_bytes(original)
 restored,restored_output=compile_and_run(label+'-restored',test)
 row={'gate_sha':sha,'control':label,'call_site':path.relative_to(root).as_posix(),'test':test,'assertion':assertion,'broken_compile_exit':0,'broken_test_exit':broken,'restored_compile_exit':0,'restored_test_exit':restored,'discriminates':broken==101 and assertion in output and 'test '+test+' ... FAILED' in output,'exact_restoration':path.read_bytes()==original,'restored_pass':restored==0 and '1 passed; 0 failed' in restored_output}
 results.append(row);(out/'results.json').write_text(json.dumps(results,indent=2));print(json.dumps(row),flush=True)
 if not row['discriminates']or not row['restored_pass']:raise SystemExit('FINDING: mutation survived or restoration gate failed; '+str(out))
assert len(results)==7,'independent compiled-control floor, iteration 1 stops at seven'
