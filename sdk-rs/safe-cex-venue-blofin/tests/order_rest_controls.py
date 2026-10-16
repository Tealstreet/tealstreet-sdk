"""Serial runtime mutation proof; always restore bytes, retain every failure/survivor."""
import pathlib,subprocess,os,sys,json,re
root=pathlib.Path(__file__).resolve().parents[3]
out=pathlib.Path(sys.argv[1]);out.mkdir(parents=True,exist_ok=True)
sha=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip()
env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
controls=[
 ('place_order_batch','.chunks(20)','.chunks(21)','native_place_order_batch'),
 ('place_algo_order_batch','Some("102143")','Some("102144")','native_place_algo_order_batch'),
 ('place_order','(host.resolve_caller_client_order_id)(opts)?;','let _ = opts;','native_place_order'),
 ('place_orders','(host.assert_caller_client_order_ids)(opts)?;','let _ = opts;','native_place_orders'),
 ('cancel_normal_orders','cancel_batches(orders, false','cancel_batches(orders, true','native_cancel_normal_orders'),
 ('cancel_algo_orders','cancel_batches(orders, true','cancel_batches(orders, false','native_cancel_algo_orders'),
 ('cancel_trigger_orders','(host.sleep)(350)','(host.sleep)(351)','native_cancel_trigger_orders'),
 ('cancel_orders','replacement.amount = 0.0;','replacement.amount = 1.0;','native_cancel_orders'),
 ('change_margin_type','(host.get_margin_type)(None, true)','(host.get_margin_type)(None, false)','native_change_margin_type'),
 ('set_leverage','market.limits.leverage.max','market.limits.leverage.min','native_set_leverage'),
 ('update_order','js_number(object.get("price"))','js_number(object.get("amount"))','native_update_order'),
 ('pair','Poll::Ready(Err(e)) => error = Some(e),','Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),','native_parallel_leverage_writes_start_before_settlement'),
 ('source_input_freeze',None,None,'native_set_leverage'),
 ('cancel_batches_null','truth(crate::order_services::field(row, "tpslId")?)','truth(row.get("tpslId"))','native_cancel_algo_orders'),
 ('set_leverage_zero','    let existing:', '    let leverage = if leverage == 0.0 { 0.0 } else { leverage };\n    let existing:', 'native_signed_zero_leverage_clamp'),
]
results=[]
for method,before,after,assertion in controls:
 path=root/'sdk-rs/safe-cex-venue-blofin'/(('tests/order_rest_oracle.cjs') if method=='source_input_freeze' else 'src/order_rest.rs')
 original=path.read_bytes();s=original.decode()
 if before:
  start=re.search(r'\b(?:pub )?async fn '+({'set_leverage_zero':'set_leverage','cancel_batches_null':'cancel_batches'}.get(method,method))+r'(?:<|\()',s).start()
  match=re.search(r'\n(?:pub )?async fn ',s[start+1:]);end=start+1+match.start() if match else len(s)
  part=s[start:end];assert before in part,(method,before);mutant=s[:start]+part.replace(before,after,1)+s[end:]
 else:
  assert 'const inputRaw=raw(input);' in s and 'return {inputRaw,name' in s
  mutant=s.replace('const inputRaw=raw(input);','let inputRaw;',1).replace('return {inputRaw,name','return {inputRaw:raw(input),name',1)
 command=['cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-blofin','--test','order_rest','--jobs','2',assertion,'--','--exact','--nocapture']
 try:
  path.write_text(mutant);broken=subprocess.run(command,cwd=root,env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT);(out/(method+'-broken.log')).write_text(broken.stdout)
 finally:path.write_bytes(original)
 restored=subprocess.run(command,cwd=root,env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT);(out/(method+'-restored.log')).write_text(restored.stdout)
 caught=broken.returncode==101 and 'panicked at' in broken.stdout and ('test '+assertion+' ... FAILED') in broken.stdout
 passed=restored.returncode==0 and '1 passed; 0 failed' in restored.stdout
 row=dict(method=method,call_site=str(path.relative_to(root)),assertion=assertion,broken_exit=broken.returncode,restored_exit=restored.returncode,caught=caught,restored=passed)
 results.append(row);(out/'results.json').write_text(json.dumps(dict(sha=sha,expected_controls=15,stopped_after=len(results),results=results),indent=2)+'\n');print(json.dumps(row),flush=True)
 if not caught or not passed:sys.exit('FINDING or harness failure '+method+'; bytes restored; see logs')
