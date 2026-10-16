"""Serial production mutation proof; each named runtime assertion must fail then pass."""
import pathlib,subprocess,os,sys,json,re
root=pathlib.Path(__file__).resolve().parents[3];out=pathlib.Path(sys.argv[1]);out.mkdir(parents=True,exist_ok=True);sha=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip();env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
controls=[
 ('get','if !yielded {','if false && !yielded {','native_fetch_orders_with_retry','immediate_await_scheduling'),
 ('execution_rows','else if raw_id.is_string()','else if true','native_fetch_historical_fills','strict_numeric_fallback'),
 ('fetch_orders','fetch_orders_with_retry(3.0','fetch_orders_with_retry(0.0','native_fetch_orders'),
 ('fetch_orders_with_retry','1000.0 * attempt','1001.0 * attempt','native_fetch_orders_with_retry'),
 ('fetch_paginated_order_rows','"limit".into(), json!(100)','"limit".into(), json!(101)','native_fetch_paginated_order_rows'),
 ('fetch_normal_orders','"UNFILLED_ORDERS"','"UNFILLED_ALGO_ORDERS"','native_fetch_normal_orders'),
 ('fetch_algo_orders','"tpslId"','"wrongId"','native_fetch_algo_orders'),
 ('fetch_trigger_orders','"orderType":"trigger"','"orderType":"wrong"','native_fetch_trigger_orders'),
 ('fetch_historical_orders','b.created_time - a.created_time','a.created_time - b.created_time','native_fetch_historical_orders'),
 ('execution_rows','multiply(qty, contract_size)','multiply(qty, contract_size * 0.0 + 1.0)','native_fetch_historical_fills'),
 ('fetch_historical_fills','format!("id:{id}")','format!("id:{}",id.len())','native_fetch_historical_fills','per_fill_dedupe'),
 ('fetch_historical_fills','if !limit.is_finite()','if false && !limit.is_finite()','native_fill_boundaries','nan_limit_boundary'),
 ('fetch_historical_fills','from.is_some_and(|v| !v.is_finite())','false && from.is_some_and(|v| !v.is_finite())','native_fill_boundaries','normalized_time_boundary'),
]
results=[]
for control in controls:
 method,before,after,assertion=control[:4];label=control[4]if len(control)>4 else method;path=root/'sdk-rs/safe-cex-venue-blofin/src/order_reads.rs';original=path.read_bytes();s=original.decode();match=re.search(r'\b(?:pub )?(?:async )?fn '+method+r'(?:<|\()',s);assert match,method;start=match.start();match=re.search(r'\n(?:pub )?(?:async )?fn ',s[start+1:]);end=start+1+match.start()if match else len(s);part=s[start:end];assert before in part,(method,before);mutant=s[:start]+part.replace(before,after,1)+s[end:];cmd=['cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-blofin','--test','order_reads','--jobs','2',assertion,'--','--exact','--nocapture']
 try:path.write_text(mutant);broken=subprocess.run(cmd,cwd=root,env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT);(out/(label+'-broken.log')).write_text(broken.stdout)
 finally:path.write_bytes(original)
 restored=subprocess.run(cmd,cwd=root,env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT);(out/(label+'-restored.log')).write_text(restored.stdout)
 caught=broken.returncode==101 and 'panicked at' in broken.stdout and 'test '+assertion+' ... FAILED' in broken.stdout;passed=restored.returncode==0 and '1 passed; 0 failed' in restored.stdout;row=dict(method=method,label=label,assertion=assertion,call_site='sdk-rs/safe-cex-venue-blofin/src/order_reads.rs',broken_exit=broken.returncode,restored_exit=restored.returncode,caught=caught,restored=passed);results.append(row);(out/'results.json').write_text(json.dumps(dict(sha=sha,expected_controls=13,stopped_after=len(results),results=results),indent=2)+'\n');print(json.dumps(row),flush=True)
 if not caught or not passed:sys.exit('FINDING or harness failure '+label+'; bytes restored, see logs')
