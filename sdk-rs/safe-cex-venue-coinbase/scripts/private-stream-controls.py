"""Scoped sequential compiling controls, with exact source restoration after each run."""
import pathlib,subprocess,os,json,sys
root=pathlib.Path(__file__).resolve().parents[3];source=root/'sdk-rs/safe-cex-venue-coinbase/src/private_stream_dispatch.rs'
sha=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip();out=pathlib.Path(sys.argv[1] if len(sys.argv)>1 else '/tmp/coinbase-private-stream-controls')/sha;out.mkdir(parents=True,exist_ok=True);original=source.read_text()
controls=[
 ('private-endpoint','private_stream_live_native_corpus','wss://advanced-trade-ws-user.coinbase.com','wss://native-broken.invalid','connect-local: private endpoint: native UTF16 string',None),
 ('signer-to-sign','private_stream_live_native_corpus','coinbase-websocket-jwt.{channel}','coinbase-BROKEN-jwt.{channel}','open-remote: native to-sign text',None),
 ('epoch-guard','pending_local_signatures_cancel_and_rejections_close_current_socket','if r.host.is_disposed() || attempt != r.subscription_attempt.get() {','if { let _ = attempt; r.host.is_disposed() } {','stale/disposed local signature suppresses writes',('pub fn subscribe<','// Promise-return adoption')),
 ('local-adoption','local_signature_promise_adoption_matches_source_checkpoints','queue_job(spawn, move || {\n        child.observe(&jobs, move |value| resolve.resolve(value))\n    });','queue_job(spawn, move || {\n        child.observe(&jobs, move |value| resolve.resolve(value))\n    });','local signing adoption source checkpoint write counts','adopt'),
 ('auth-classification','private_stream_live_native_corpus','if auth_failure(&frame_string(message.as_ref())?) {','if !auth_failure(&frame_string(message.as_ref())?) {','message-auth-error: normal error classification',None),
 ('health-before-events','private_stream_live_native_corpus','r.host.mark_private_user_stream_message_received()?;','// mutation: dropped health mark','message-user: independent source invocation count',None),
]
if len(sys.argv)>2:
 start=next(i for i,c in enumerate(controls) if c[0]==sys.argv[2])
 controls=controls[start:]
# Source helper replacement models return-await instead of return-Promise adoption.
adopt_mutation='''fn adopt<T: Clone + 'static>(child: Promise<T>, spawn: &CoinbaseStarbasePromiseSpawner) -> Promise<T> {
    Promise::from_future(Box::pin(async move { child.await }),spawn)
}
'''
env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript');results=[]
try:
 for name,test,old,new,assertion,scope in controls:
  if scope=='adopt':
   begin=original.index('fn adopt<');end=original.index('// ts: CoinbasePrivateWebsocket.buildSubscriptionJwt',begin);mutated=original[:begin]+adopt_mutation+original[end:]
   # Removing the helper's only queue_job use also removes its import, keeping the control compilable.
   mutated=mutated.replace('CoinbaseStarbasePromiseSpawner, queue_job','CoinbaseStarbasePromiseSpawner')
  elif scope:
   begin=original.index(scope[0]);end=original.index(scope[1],begin);part=original[begin:end];assert part.count(old)==1,(name,'selector must identify one call site');mutated=original[:begin]+part.replace(old,new)+original[end:]
  else:
   assert original.count(old)==1,(name,'selector must identify one call site');mutated=original.replace(old,new)
  cmd=['cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-coinbase','--test','private_stream_dispatch',test,'--jobs','2','--','--exact']
  def run(stage):
   with (out/(name+'-'+stage+'.log')).open('w') as f:r=subprocess.run(cmd,cwd=root,env=env,stdout=f,stderr=subprocess.STDOUT)
   return r.returncode,(out/(name+'-'+stage+'.log')).read_text()
  source.write_text(mutated);broken,text=run('broken');source.write_text(original);restored,restored_text=run('restored')
  result=dict(name=name,sha=sha,command=cmd,call_site=old,named_assertion=assertion,broken_exit=broken,restored_exit=restored,compiled='Finished `test`' in text and 'error[E' not in text,named_failure=assertion in text,broken_output=text,restored_output=restored_text);results.append(result)
  (out/'results.json').write_text(json.dumps(dict(sha=sha,enumeration='stopped after '+str(len(results))+' selected controls; six family controls declared',controls=results),indent=2)+'\n');print(json.dumps({k:v for k,v in result.items() if not k.endswith('output')}),flush=True)
  if not(broken==101 and restored==0 and result['compiled'] and result['named_failure']):raise SystemExit('Control failed or survived; finding retained: '+name)
finally:source.write_text(original)
assert source.read_text()==original
