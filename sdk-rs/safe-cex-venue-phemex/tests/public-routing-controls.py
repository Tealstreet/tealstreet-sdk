from pathlib import Path
import os,subprocess,json
root=Path(__file__).resolve().parents[3]
source=root/'sdk-rs/safe-cex-venue-phemex/src/public_routing.rs'
out=root/'sdk-rs/target/phemex-public-routing-evidence'
out.mkdir(parents=True,exist_ok=True)
env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
controls=[
 ('connect-network','connect_and_subscribe','if host.testnet()?','if !host.testnet()?','connect','PUBLIC native effect create'),
 ('open-id','on_open','id: 1.0','id: 2.0','open','PUBLIC native effect serialize'),
 ('subscribe-id','subscribe','let mut id = 10.0','let mut id = 11.0','subscribe','PUBLIC native effect serialize'),
 ('warning-threshold','update_subscription_limit_warning','active >= 18','active >= 17','warning','PUBLIC native complete observation'),
 ('ping-id','ping','host.number_string(id)?','host.number_string(id + 1.0)?','ping','PUBLIC native effect send'),
 ('dispatch-stats','dispatch_topic','if !each {','if each {','dispatch','PUBLIC native effect callback:a'),
 ('interval-lookup','get_kline_interval','host.cached_interval(numeric)','host.cached_interval(numeric + 1.0)','interval','PUBLIC native complete observation'),
 ('message-book','on_message','units("orderbook_p.")','units("orderbook_wrong.")','message','PUBLIC native effect dispatch'),
]
results=[]
for name,function,before,after,test,assertion in controls:
 original=source.read_text();start=original.index('pub fn '+function+'<')
 endings=[original.find('\npub ',start+1),original.find('\nfn ',start+1)]
 end=min((e for e in endings if e!=-1),default=len(original))
 block=original[start:end];assert block.count(before)==1,(name,block.count(before))
 command=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-phemex','--jobs','2','--test','public_routing','actual_typescript_'+test,'--','--exact','--nocapture','--test-threads=1']
 try:
  source.write_text(original[:start]+block.replace(before,after)+original[end:])
  failed=subprocess.run(command,cwd=root,env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
  (out/(name+'-mutated.log')).write_text(failed.stdout)
  assert failed.returncode==101 and 'Finished `test`' in failed.stdout and assertion in failed.stdout,(name,failed.stdout[-3000:])
 finally:source.write_text(original)
 passed=subprocess.run(command,cwd=root,env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
 (out/(name+'-restored.log')).write_text(passed.stdout)
 assert passed.returncode==0,(name,passed.stdout[-3000:])
 results.append({'row_family':name,'compiled_exit':failed.returncode,'assertion':assertion,'restored_exit':passed.returncode})
 (out/'controls.json').write_text(json.dumps(results,indent=2)+'\n')
 print(name+': compiled native failure 101; restored pass 0',flush=True)
assert len(results)==8
