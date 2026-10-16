"""Serial production controls: named runtime assertions, restoration, no compiler-only proof."""
import json,os,pathlib,subprocess,sys
out=pathlib.Path(sys.argv[1]);out.mkdir(parents=True,exist_ok=True);sha=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip();path=pathlib.Path('sdk-rs/safe-cex-venue-blofin/src/private_stream_control.rs');saved=path.read_bytes();source=saved.decode()
controls=[
 ('endpoint','native_endpoint','if host.is_testnet() {','if !host.is_testnet() {'),
 ('dispose','native_dispose','host.super_dispose()','Ok(())'),
 ('connect','native_connect','if route.tunnelled {','if !route.tunnelled {'),
 ('open','native_open','host.launch_auth(task);','drop(task);'),
 ('ping','native_ping','state.ping_at.set(host.performance_now());','state.ping_at.set(0.0);'),
 ('auth','native_auth','Encoding::Hex','Encoding::Base64'),
 ('subscribe','native_subscribe','["account", "positions", "orders", "orders-algo"]','["account", "positions", "orders-algo", "orders"]'),
 ('message','native_message','host.handle_order_topic(parsed.json())?','host.handle_account_topic(parsed.json())?'),
 ('throttle','native_throttle','host.set_timeout(callback, 2000.0)?','host.set_timeout(callback, 200.0)?'),
 ('fetch_loaded','native_fetch_throttled','loaded.orders = true;','loaded.orders = false;'),
]
assert len(controls)==10
controls.append(('remote_coercion','native_auth','js_string(result.error.as_ref()).map_err(PrivateAuthError::Coercion)?','js_string(result.error.as_ref()).unwrap_or_default()'))
assert len(controls)==11
for label,_,pattern,_ in controls:assert source.count(pattern)==1,label+': unique control target'
if len(sys.argv)>2 and sys.argv[2]=='--preflight':print('11 unique targets; no Cargo or mutation');raise SystemExit(0)
env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript');rows=[]
for index,(label,test,pattern,changed) in enumerate(controls,1):
 command=['cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-blofin','--test','private_stream_control','--jobs','2',test,'--','--exact','--nocapture']
 try:
  path.write_text(source.replace(pattern,changed,1));broken=subprocess.run(command,env=env,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True);(out/(label+'-broken.log')).write_text(broken.stdout)
 finally:path.write_bytes(saved)
 restored=subprocess.run(command,env=env,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True);(out/(label+'-restored.log')).write_text(restored.stdout)
 caught=broken.returncode==101 and 'panicked at' in broken.stdout and f'test {test} ... FAILED' in broken.stdout;passed=restored.returncode==0 and 'test result: ok. 1 passed;' in restored.stdout
 row=dict(sha=sha,label=label,assertion=test,call_site=str(path),command=command,broken_exit=broken.returncode,restored_exit=restored.returncode,caught=caught,restored=passed);rows.append(row);(out/'results.json').write_text(json.dumps(dict(sha=sha,expected_controls=11,stopped_after=index,results=rows),indent=2)+'\n');print(json.dumps(row),flush=True)
 assert path.read_bytes()==saved
 if not caught or not passed:raise SystemExit('Survivor or failure before named runtime assertion; retained '+label)
