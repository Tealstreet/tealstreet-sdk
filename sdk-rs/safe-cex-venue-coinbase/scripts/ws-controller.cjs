const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH??'typescript');
const dir=path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/coinbase');const modules={};
for(const name of ['coinbase.starbase','coinbase.starbase-session','coinbase.starbase-ws-session']){
 const output={};vm.runInNewContext(ts.transpileModule(fs.readFileSync(path.join(dir,name+'.ts'),'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports:output,require:name=>modules[name.replace('./','')]});modules[name]=output;
}
const {CoinbaseStarbaseAuthSession:Auth,CoinbaseStarbaseJsonRpcClient:Client,CoinbaseStarbaseHeartbeatController:Heartbeat}=modules['coinbase.starbase'];
const {CoinbaseStarbaseWebsocketSessionController:Controller}=modules['coinbase.starbase-ws-session'];
const authResult={access_token:'native-token',expires_in:60,scope:'native-scope',token_type:'bearer'};
const copy=x=>JSON.parse(JSON.stringify(x));const error=e=>({name:e.name,message:e.message});
const capture=async fn=>{try{const result=await fn();return result===undefined?{undefined:true}:{ok:copy(result)}}catch(e){return{error:error(e)}}};
const idle=async()=>{for(let i=0;i<32;i++)await Promise.resolve()};
function fixture(spec={}){
 let jwtCalls=0,replays=0;const calls=[],events=[];let release;
 const gate=spec.gate?new Promise(resolve=>release=resolve):undefined;
 let controller;
 const client=new Client(async(request,context)=>{
  calls.push(copy({request,context}));events.push(request.method);
  if(spec.disconnectAt===request.method)controller.recordDisconnect(spec.reason??'transport');
  if(spec.gate===request.method)await gate;
  if(spec.rejectAt===request.method)throw new Error('wire identity');
  let result;
  if(request.method==='public/auth')result=spec.authResult??authResult;
  else if(request.method==='public/set_heartbeat')result=spec.heartbeatResult??'ok';
  else if(request.method.includes('enable_cancel')||request.method.includes('disable_cancel'))result=spec.mutationResult??'ok';
  else if(request.method==='private/get_cancel_on_disconnect')result=Object.hasOwn(spec,'readback')?spec.readback:{enabled:spec.policy.enabled,scope:spec.policy.scope??'connection'};
  else if(request.method==='public/test')result=Object.hasOwn(spec,'testResult')?spec.testResult:{version:'native-version'};
  return{jsonrpc:'2.0',id:request.id,result};
 },{kind:spec.kind??'websocket',initialRequestId:7});
 const auth=new Auth(client,()=>{jwtCalls++;events.push('jwt');if(spec.jwtError)return Promise.reject(new Error('jwt identity'));return Promise.resolve('jwt-'+jwtCalls)}, {clock:()=>1000});
 const heartbeat=new Heartbeat(client.kind==='websocket'?client:new Client(async()=>null,{kind:'websocket'}),10.5);
 controller=new Controller({client,authSession:auth,heartbeat,cancelOnDisconnect:spec.policy,replaySubscriptions:()=>{replays++;events.push('replay');if(spec.disconnectAt==='replay')controller.recordDisconnect(spec.reason??'transport');return spec.replayError?Promise.reject(new Error('replay identity')):Promise.resolve()}});
 return{controller,auth,release:()=>release(),counts:()=>({jwtCalls,replays,calls:copy(calls),events:[...events],state:controller.state})};
}
(async()=>{
 const constructors=[];for(const kind of ['http','websocket']){constructors.push({kind,result:await capture(()=>fixture({kind}).controller.state)})}
 const specs=[
  {name:'basic'}, {name:'enabled-default',policy:{enabled:true}}, {name:'disabled-account',policy:{enabled:false,scope:'account'}},
  {name:'enabled-account',policy:{enabled:true,scope:'account'}}, {name:'disabled-connection',policy:{enabled:false,scope:'connection'}},
  {name:'jwt-error',jwtError:true}, {name:'auth-invalid',authResult:{}}, {name:'heartbeat-invalid',heartbeatResult:'unexpected'},
  {name:'mutation-invalid',policy:{enabled:true},mutationResult:'unexpected'},
  ...[null,{}, {enabled:'true',scope:'connection'}, {enabled:true,scope:'invalid'}, {enabled:true,scope:'connection',extra:0}, {enabled:false,scope:'connection'}, {enabled:true,scope:'account'}].map(readback=>({name:'readback-'+JSON.stringify(readback),policy:{enabled:true},readback})),
  {name:'replay-error',replayError:true},
  ...['public/auth','public/set_heartbeat','private/enable_cancel_on_disconnect','private/get_cancel_on_disconnect','replay'].map(disconnectAt=>({name:'disconnect-'+disconnectAt,disconnectAt,policy:{enabled:true}})),
 ];
 const runs=[];
 for(const spec of specs){const f=fixture(spec);const first=f.controller.activate(),second=f.controller.activate();const initial={same:first===second,...f.counts()};
  const result=await capture(()=>first);await idle();const final=f.counts();let ready;
  if(final.state==='ready'){
   const next=f.controller.activate();ready={same:next===first,result:await capture(()=>next),maintenance:await capture(()=>f.controller.maintainAuthentication()),heartbeat:await capture(()=>f.controller.handleClassifiedHeartbeat('heartbeat')),test:await capture(()=>f.controller.handleClassifiedHeartbeat('test_request'))};
   await idle();ready.final=f.counts();
  }
  runs.push({spec,initial,result,final,...ready?{ready}:{}});
 }
 const guards=[];for(const state of ['idle','activating','ready','disconnected','rate-limit-backoff','failed']){
  const f=fixture();f.controller.state=state;
  const maintenance=await capture(()=>f.controller.maintainAuthentication());const heartbeat=await capture(()=>f.controller.handleClassifiedHeartbeat('heartbeat'));const test=await capture(()=>f.controller.handleClassifiedHeartbeat('test_request'));const ack=await capture(()=>f.controller.acknowledgeRateLimitBackoff());
  await idle();guards.push({state,maintenance,heartbeat,test,ack,final:f.counts()});
 }
 const disconnects=[];for(const reason of ['heartbeat-timeout','rate-limit','transport']){
  const f=fixture();f.controller.recordDisconnect(reason);const state=f.controller.state;
  const activation=await capture(()=>f.controller.activate());await idle();let acknowledgement;
  if(reason==='rate-limit') { acknowledgement=await capture(()=>f.controller.acknowledgeRateLimitBackoff());await f.controller.activate();await idle(); }
  disconnects.push({reason,state,activation,...acknowledgement?{acknowledgement}:{},final:f.counts()});
 }
 // A queued disconnect must interleave with the ready await continuations.
 const race=fixture();const pending=race.controller.activate();queueMicrotask(()=>race.controller.recordDisconnect('transport'));
 const queuedRace={result:await capture(()=>pending)};await idle();queuedRace.final=race.counts();
 const ignored=fixture();ignored.controller.activate();await idle();const ignoredResult=ignored.counts();
 // Old generations never clear or fail a replacement activation.
 const stale=fixture({gate:'public/auth'});const old=stale.controller.activate();old.catch(()=>{});await idle();stale.controller.recordDisconnect('transport');const fresh=stale.controller.activate();const shared=stale.controller.activate();stale.release();
 const replacement={sameFresh:shared===fresh,sameOld:fresh===old,old:await capture(()=>old),fresh:await capture(()=>fresh)};await idle();replacement.final=stale.counts();
 const policySpec={gate:'private/enable_cancel_on_disconnect',policy:{enabled:true,scope:'connection'}};
 const changed=fixture(policySpec);const changePromise=changed.controller.activate();await idle();policySpec.policy.enabled=false;policySpec.policy.scope='account';changed.release();
 const policyMutation={result:await capture(()=>changePromise)};await idle();policyMutation.final=changed.counts();
 const retry=fixture({replayError:true});const retryResults=[await capture(()=>retry.controller.activate())];await idle();retryResults.push(await capture(()=>retry.controller.activate()));await idle();
 process.stdout.write(JSON.stringify({constructors,runs,guards,disconnects,queuedRace,ignored:ignoredResult,replacement,policyMutation,retry:{results:retryResults,final:retry.counts()}}));
})().catch(e=>{console.error(e);process.exitCode=1});
