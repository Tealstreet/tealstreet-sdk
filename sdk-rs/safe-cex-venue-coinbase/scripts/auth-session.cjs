const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH??'typescript');
const dir=path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/coinbase');
const exportsObject={};vm.runInNewContext(ts.transpileModule(fs.readFileSync(path.join(dir,'coinbase.starbase.ts'),'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports:exportsObject});
const {CoinbaseStarbaseAuthSession:Auth,CoinbaseStarbaseJsonRpcClient:Client}=exportsObject;
const error=e=>({name:e.name,message:e.message});
const capture=async fn=>{try{return{ok:await fn()}}catch(e){return{error:error(e)}}};
const copy=value=>JSON.parse(JSON.stringify(value));
const value=input=>input?.special==='NaN'?NaN:input?.special==='Infinity'?Infinity:input;
const authResult=(token='access-0',expires=2)=>({access_token:token,expires_in:expires,scope:'scope identity',token_type:'bearer',extra:{marker:'retained'}});
(async()=>{
 const constructors=[];
 for(const kind of ['http','websocket'])for(const refresh of [{missing:true},null,0,-1,0.1,1,1000,'1000',false,[],{},{special:'NaN'},{special:'Infinity'}]){
  const options=refresh?.missing?{}:{refreshAfterMs:value(refresh)};
  constructors.push({kind,refresh,result:await capture(()=>{new Auth(new Client(async()=>null,{kind}),async()=>'jwt',options);return true})});
 }
 const scenarios=[
  {name:'response-expiry',refresh:100000,steps:[{now:1000},{now:2999},{now:3000}]},
  {name:'policy-expiry',refresh:500,steps:[{now:1000},{now:1499},{now:1500}]},
  {name:'http-default',kind:'http',expires:3600,steps:[{now:100},{now:900099},{now:900100}]},
  {name:'ws-default',kind:'websocket',expires:3600,steps:[{now:100},{now:2940099},{now:2940100}]},
  {name:'forced',refresh:10000,steps:[{now:1000},{now:1001,force:true},{now:1002}]},
  {name:'clock-backwards',refresh:10000,steps:[{now:1000},{now:-100},{now:3000}]},
  {name:'nan-clock',refresh:10000,steps:[{now:1000},{now:{special:'NaN'}},{now:1001}]},
  {name:'header',refresh:10000,steps:[{now:1000,action:'header'},{now:1001,action:'header'},{now:1002,action:'header',force:true}]},
  {name:'websocket-header',kind:'websocket',steps:[{now:1000,action:'header'}]},
  ...[null,0,'3',[],{}, {toString:null}].map(edit=>({name:'expiry-edit-'+JSON.stringify(edit),refresh:10000,steps:[{now:1000},{now:1001,edit:{expires_in:edit}},{now:1002,force:true}]})),
  ...[null,0,'edited',[],{}, {toString:null}].map(edit=>({name:'token-edit-'+JSON.stringify(edit),refresh:10000,steps:[{now:1000},{now:1001,edit:{access_token:edit},action:'header'}]})),
  {name:'delete-expiry',refresh:10000,steps:[{now:1000},{now:1001,remove:'expires_in'}]},
  {name:'delete-token',refresh:10000,steps:[{now:1000},{now:1001,remove:'access_token',action:'header'}]},
  {name:'empty-jwt',tokens:['',' next '],steps:[{now:1000},{now:1000}]},
  ...[null,{}, {access_token:'',expires_in:2,scope:'',token_type:'bearer'}, {access_token:'valid',expires_in:0,scope:'',token_type:'bearer'}, {access_token:'valid',expires_in:2,scope:'',token_type:'wrong'}].map(result=>({name:'invalid-auth-'+JSON.stringify(result),responses:[result,authResult('recovered',3)],steps:[{now:1000},{now:1001}]})),
 ];
 const runs=[];
 for(const spec of scenarios){
  let now=0,jwtCalls=0,clockCalls=0,responseCount=0,last;
  const calls=[];
  const client=new Client(async(request,context)=>{calls.push(copy({request,context}));const result=spec.responses?spec.responses[responseCount]:authResult('access-'+responseCount,spec.expires??2);responseCount++;return{jsonrpc:'2.0',id:request.id,result}}, {kind:spec.kind??'http',initialRequestId:7});
  const session=new Auth(client,()=>{const token=spec.tokens?.[jwtCalls]??'jwt-'+jwtCalls;jwtCalls++;return Promise.resolve(token)}, {clock:()=>{clockCalls++;return now}, ...spec.refresh===undefined?{}:{refreshAfterMs:spec.refresh}});
  const steps=[];
  for(const step of spec.steps){
   now=value(step.now);if(step.edit)Object.assign(last,step.edit);if(step.remove)delete last[step.remove];
   let same=false;const outcome=await capture(async()=>{if(step.action==='header')return await session.getHttpAuthorization(step.force??false);const result=await session.authenticate(step.force??false);same=last===result;last=result;return result});
   steps.push({outcome:copy(outcome),same,jwtCalls,clockCalls,rpcCalls:calls.length});
  }
  runs.push({spec,steps,calls});
 }
 // Ignoring the returned promise still populates the cache, and every refresh is independent.
 let jwtCalls=0;const calls=[];let now=1000;
 const client=new Client(async request=>{calls.push(request.id);return{jsonrpc:'2.0',id:request.id,result:authResult('access-'+request.id,60)}},{kind:'http',initialRequestId:7});
 const session=new Auth(client,async()=>{jwtCalls++;return'jwt'}, {clock:()=>now});
 session.authenticate();for(let i=0;i<12;i++)await Promise.resolve();const cached=await session.authenticate();
 const ignored={jwtCalls,calls:[...calls],result:copy(cached)};
 let resolveA,resolveB;let providerIndex=0;const deferred=[new Promise(r=>resolveA=r),new Promise(r=>resolveB=r)];
 const concurrentCalls=[];const concurrentClient=new Client(async request=>{concurrentCalls.push(copy(request));return{jsonrpc:'2.0',id:request.id,result:authResult(request.params.token,2)}},{kind:'http',initialRequestId:7});
 const concurrent=new Auth(concurrentClient,()=>deferred[providerIndex++], {clock:()=>now,refreshAfterMs:10000});
 const first=concurrent.authenticate();now=2000;const second=concurrent.authenticate();
 resolveB('second');const secondResult=await second;resolveA('first');const firstResult=await first;
 now=2999;const latest=await concurrent.authenticate();
 const concurrency={providerIndex,calls:concurrentCalls,first:copy(firstResult),second:copy(secondResult),latest:copy(latest),sameFirst:latest===firstResult};
 let racingJwt=0,racingRpc=0;
 const racingClient=new Client(async request=>{racingRpc++;return {jsonrpc:'2.0',id:request.id,result:authResult('racing-'+request.id,60)}},{kind:'http',initialRequestId:7});
 const racingSession=new Auth(racingClient,()=>{racingJwt++;return Promise.resolve('jwt')},{clock:()=>1000});
 const a=racingSession.authenticate();let b;queueMicrotask(()=>{b=racingSession.authenticate()});
 await a;await b;for(let i=0;i<12;i++)await Promise.resolve();
 const queuedSecondAuth={jwtCalls:racingJwt,rpcCalls:racingRpc,result:copy(await racingSession.authenticate())};
 let releaseOrdered;const orderedToken=new Promise(resolve=>releaseOrdered=resolve),orderedEvents=[];
 (async()=>{await orderedToken;orderedEvents.push('earlier-await')})();
 const orderedClient=new Client(async request=>{orderedEvents.push(request.method);return {jsonrpc:'2.0',id:request.id,result:authResult()}},{kind:'http'});
 const orderedSession=new Auth(orderedClient,()=>orderedToken,{clock:()=>1000});const orderedAuth=orderedSession.authenticate();releaseOrdered('jwt');await orderedAuth;
 const fulfilledEvents=[];(async()=>{await Promise.resolve('ready');fulfilledEvents.push('after-await')})();fulfilledEvents.push('synchronous');await Promise.resolve();await Promise.resolve();
 process.stdout.write(JSON.stringify({constructors,runs,ignored,concurrency,queuedSecondAuth,orderedEvents,fulfilledEvents}));
})().catch(e=>{console.error(e);process.exitCode=1});
