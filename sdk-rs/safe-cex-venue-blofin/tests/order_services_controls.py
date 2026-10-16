"""One serial, byte-restored production mutation for each new method, no CI."""
import pathlib,json,subprocess,os,sys
root=pathlib.Path(__file__).resolve().parents[3];out=pathlib.Path(sys.argv[1]);out.mkdir(parents=True,exist_ok=True)
sha=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip();env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
controls=[
 ('order_formatters.rs','format_trigger_order','market.precision.price','market.precision.amount'),
 ('order_formatters.rs','format_normal_order','let amount = adjust(opts.amount, lot);','let amount = adjust(opts.amount, market.precision.amount);'),
 ('order_formatters.rs','format_algo_order','if amount == 0.0 { -1.0 } else { amount }','if amount == 0.0 { 0.0 } else { amount }'),
 ('order_services.rs','get_or_fetch_position_mode','Some("net_mode")','Some("wrong_mode")'),
 ('order_services.rs','get_or_fetch_margin_type','Some("isolated")','Some("wrong_margin")'),
 ('order_services.rs','fetch_leverage_for_symbol','buy_leverage:buy,sell_leverage:sell','buy_leverage:sell,sell_leverage:buy'),
 ('order_services.rs','change_position_mode','Some("net_mode")','Some("wrong_mode")'),
 ('order_services.rs','place_trigger_order','js_string(id).map_err(ServiceError::Coercion)','Ok("BROKEN".into())'),
 ('order_services.rs','cancel_symbol_orders','.filter(|o| o.symbol == symbol)','.filter(|o| o.symbol != symbol)'),
 ('order_formatters.rs','format_normal_order','if total_count > 10_000','if total_count > 10_001','native_large_split_profile_is_explicit_refusal'),
 ('order_services.rs','fetch_leverage_for_symbol','rows.get("length").and_then(Value::as_f64)==Some(0.0)','rows.get("length").and_then(Value::as_f64)==Some(1.0)','native_fetch_leverage_for_symbol'),
 ('order_services.rs','error_message','and_then(|r| body::<E>(&r.body).ok())','map(|r| body::<E>(&r.body)).transpose()?','native_fetch_leverage_for_symbol'),
]
results=[]
for control in controls:
 file,method,before,after=control[:4]
 path=root/'sdk-rs/safe-cex-venue-blofin/src'/file;original=path.read_bytes();text=original.decode();prefix='fn ' if method=='error_message' else 'pub async fn ';start=text.index(prefix+method);end=text.find('\npub async fn ',start+1);end=len(text) if end<0 else end;scope=text[start:end];assert before in scope,(method,before)
 # Price-axis control changes only the first consumer in this function, not other helpers.
 mutant=text[:start]+scope.replace(before,after,1)+text[end:]
 assertion=control[4] if len(control)>4 else 'native_'+method;label=method+('-review-regression' if len(control)>4 else '');command=['cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-blofin','--test','order_services','--jobs','2',assertion,'--','--exact','--nocapture']
 try:
  path.write_text(mutant);broken=subprocess.run(command,cwd=root,env=env,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True);(out/(label+'-broken.log')).write_text(broken.stdout)
 finally:path.write_bytes(original)
 restored=subprocess.run(command,cwd=root,env=env,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True);(out/(label+'-restored.log')).write_text(restored.stdout)
 caught=broken.returncode==101 and 'panicked at' in broken.stdout and ('test '+assertion+' ... FAILED') in broken.stdout
 passed=restored.returncode==0 and '1 passed; 0 failed' in restored.stdout
 row=dict(method=method,call_site='safe-cex-venue-blofin/src/'+file,assertion=assertion,broken_exit=broken.returncode,restored_exit=restored.returncode,caught=caught,restored=passed);results.append(row);(out/'results.json').write_text(json.dumps(dict(sha=sha,expected_controls=12,stopped_after=len(results),results=results),indent=2)+'\n');print(json.dumps(row),flush=True)
 if not caught or not passed:sys.exit('FINDING or harness failure '+method+'; source byte-restored; see logs')
