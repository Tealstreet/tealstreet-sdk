from pathlib import Path
import os, subprocess, json
root=Path(__file__).resolve().parents[3]
venue=root/'sdk-rs/safe-cex-venue-bitunix'
out=root/'sdk-rs/target/bitunix-websocket-evidence'
out.mkdir(parents=True,exist_ok=True)
env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
controls=[
 ('limit','websocket_public.rs','300.0','301.0','WS native subscription limit'),
 ('interval','websocket_public.rs','("1m", "1min")','("1m", "3min")','WS native interval'),
 ('public-ping','websocket_public.rs','"ping".into()','"pong".into()','WS native effect "public.ping"'),
 ('futures-ping','websocket_futures.rs','"ping".into()','"pong".into()','WS native effect "futures.ping"'),
 ('private-ping','websocket_private.rs','JsJsonValue::String("ping".into())','JsJsonValue::String("pong".into())','WS native effect "private.ping"'),
 ('clear-queue','websocket_public.rs','clear_timeout(read_timer()?)?;\n    unset_timer()?;','let timer = read_timer()?;\n    unset_timer()?;\n    clear_timeout(timer)?;','WS native effect "public.clearSubscriptionQueue"'),
 ('drain-queue','websocket_public.rs','"drainSubscriptionQueue", 250.0','"drainSubscriptionQueue", 251.0','WS native effect "public.drainSubscriptionQueue"'),
 ('dispose','websocket_public.rs','"markets:changed", "onMarketsChanged"','"markets:mutated", "onMarketsChanged"','WS native effect "public.dispose"'),
]
for n in ['Balance','Orders','Positions']:
 field=n.lower()
 controls.append(('debounce-'+field,'websocket_private.rs',f'("{field}ThrottleTimeout", "fetchThrottled{n}")',f'("{field}ThrottleTimeouX", "fetchThrottled{n}")',f'WS native effect "private.throttledFetch{n}"'))
 old='[("balance", true), ("positions", true)]' if n=='Balance' else f'("{field}", {field}, loaded, ("{field}", true))'
 new='[("balance", false), ("positions", true)]' if n=='Balance' else f'("{field}", {field}, loaded, ("{field}", false))'
 controls.append(('reconcile-'+field,'websocket_private.rs',old,new,f'WS native effect "private.fetchThrottled{n}"'))
controls.append(('rest-timestamp','rest_reads.rs','timestamp: None','timestamp: Some(0.0)','REST native absent ticker timestamp'))
results=[]
for name,file,old,new,label in controls:
 p=venue/'src'/file; original=p.read_text()
 assert original.count(old)==1,(name,original.count(old))
 target='rest_reads' if name=='rest-timestamp' else 'websocket_lifecycle'
 cmd=['cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-bitunix','--jobs','2','--test',target,'--','--nocapture']
 try:
  p.write_text(original.replace(old,new))
  failed=subprocess.run(cmd,cwd=root,env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
  (out/(name+'-mutated.log')).write_text(failed.stdout)
  assert failed.returncode==101 and 'Finished `test`' in failed.stdout and label in failed.stdout,(name,failed.returncode,failed.stdout[-2500:])
 finally:p.write_text(original)
 passed=subprocess.run(cmd,cwd=root,env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
 (out/(name+'-restored.log')).write_text(passed.stdout)
 assert passed.returncode==0,(name,passed.stdout[-2500:])
 results.append({'row':name,'compiled_mutation_exit':failed.returncode,'native_assertion':label,'restored_exit':passed.returncode})
 (out/'controls.json').write_text(json.dumps(results,indent=2)+'\n')
 print(name+': compiled native failure 101; restored pass 0',flush=True)
