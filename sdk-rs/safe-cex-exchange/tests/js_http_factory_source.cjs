// Execute the maintained factory with installed Axios; only transport/telemetry
// hosts are injected. No replacement factory or interceptor implementation.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const axios=require('axios');
const root=path.resolve(__dirname,'../../..');
let provider;
const source=fs.readFileSync(path.join(root,'packages/safe-cex/src/safeCexAxiosCreate.ts'),'utf8');
const code=ts.transpileModule(source,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS,esModuleInterop:true}}).outputText;
const captureModule={exports:{}};
const captureCode=ts.transpileModule(fs.readFileSync(path.join(root,'packages/safe-cex/src/utils/signingCapture.ts'),'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText;
new Function('module','exports',captureCode)(captureModule,captureModule.exports);
const imported={axios,'./remoteSigningAdapter':{createRemoteSigningAdapter:()=>{throw Error('unexpected source signer');}},'./transport/registry':{getTransport:()=>provider},'./utils/axiosErrorEnhancer':{enhanceAxiosError:e=>e},'./utils/isDevelopment':{isDevelopment:false},'./utils/signingCapture':captureModule.exports,'./utils/venueRestDiagnostics':{markRestRequestStart:()=>{},noteRestFailure:()=>{},noteRestSuccess:()=>{}},'./utils/runtime':{isNodeOrBunRuntime:()=>true}};
const moduleValue={exports:{}};
new Function('module','exports','require',code)(moduleValue,moduleValue.exports,name=>{assert.ok(Object.hasOwn(imported,name),'unexpected source dependency '+name);return imported[name];});
async function outcomeAudit() {
 const {sourceStore}=await import('../../safe-cex-store/tests/store_source.mjs');
 const {Store}=sourceStore();
 const markers={missing:undefined,false:false,true:true,one:1,text:'yes',array:[],object:{},null:null,zero:0,empty:'',nan:NaN};
 const profiles=[];
 for(const status of [200,201,299,300,401,403,418,429,500,503,'network'])
  for(const marker of ['missing','false','true']) profiles.push({status,marker,mode:'ordinary'});
 for(const marker of ['one','text','array','object','null','zero','empty','nan'])
  profiles.push({status:200,marker,mode:'ordinary'});
 for(const mode of ['frozen','disabled'])
  for(const [status,marker] of [[200,'missing'],[503,'missing'],['network','missing'],[200,'true']])
   profiles.push({status,marker,mode});
 const rows=[];
 provider={rest:({config})=>({baseURL:config.baseURL,proxied:false})};
 for(const factory of ['routed','direct'])for(const profile of profiles){
  const {status,marker,mode}=profile;
  const store=new Store();
  const initial={ok:2,rateLimit:3,auth:5,network:7,other:11};
  store.update({requestStats:mode==='disabled'?undefined:{...initial}});
  const stats=store.requestStats;
  if(mode==='frozen')Object.freeze(stats);
  let notices=0;store.subscribe(()=>notices++);
  let sends=0,capture=false,headerMarker=false,original;
  const config={baseURL:'https://venue',adapter:async config=>{
   sends++;capture=captureModule.exports.isSigningCaptureRequest(config);
   headerMarker=Object.hasOwn(config.headers,captureModule.exports.SIGNING_CAPTURE_FLAG);
   const response={status:status==='network'?undefined:status,data:{},headers:{},config};
   if(status==='network'||status>=400){original=Object.assign(Error('fixture rejection'),{config});if(status!=='network')original.response=response;throw original;}
   return response;
  }};
  const owner={name:'polymarket',store,grafanaEndpointsEvery:[],grafanaEndpointsPeriodic:[],emitter:{emit:()=>{}}};
  const xhr=moduleValue.exports[factory==='direct'?'safeCexAxiosCreateNoProxy':'safeCexAxiosCreate'](config,{key:'',secret:''},owner);
  const request=marker==='missing'?{}:{[captureModule.exports.SIGNING_CAPTURE_FLAG]:markers[marker]};
  let success=true,errorIdentity=true;
  try{await xhr.get('/audit',request);}catch(error){success=false;errorIdentity=error===original;}
  const sameStats=store.requestStats===stats;
  assert.equal(sameStats,true);assert.equal(notices,0);assert.equal(sends,1);assert.equal(headerMarker,false);assert.equal(errorIdentity,true);
  const expected={...initial};
  if(mode==='ordinary'&&!capture){const key=status==='network'?'network':[429,418].includes(status)?'rateLimit':[401,403].includes(status)?'auth':status>=200&&status<300?'ok':'other';expected[key]++;}
  assert.deepEqual(stats===undefined?null:{...stats},mode==='disabled'?null:expected);
  const statsBits=stats===undefined?null:Object.fromEntries(Object.entries(stats).map(([key,value])=>{const bytes=Buffer.alloc(8);bytes.writeDoubleBE(value);return [key,bytes.toString('hex')];}));
  rows.push({factory,...profile,statsBits,success,errorIdentity,sameStats,notices,sends,capture,headerMarker});
 }
 assert.equal(rows.length,98);
 return rows;
}
async function requestStartAudit() {
 const {loadSource}=await import('../../safe-cex-store/tests/source_loader.mjs');
 Object.assign(imported['./utils/venueRestDiagnostics'],loadSource(path.join(root,'packages/safe-cex/src/utils/venueRestDiagnostics.ts')));
 const key='safeCexRestDiagStartedAt', rows=[], originalNow=Date.now;
 const bits=value=>{const bytes=Buffer.alloc(8);bytes.writeDoubleBE(value);return bytes.toString('hex');};
 provider={rest:({config})=>({baseURL:config.baseURL,proxied:false})};
 try {
  for(const factory of ['routed','direct'])for(const epoch of [0,12345.75,-12345.25,1700000000000])
   for(const mode of ['fresh','overwrite','readonly','readonly-undefined']) {
    Date.now=()=>Math.floor(epoch);
    const events=[];let seen,originalConfig,sends=0; const view=value=>{if(typeof value==='number')return {kind:'number',bits:bits(value)};assert.equal(value,undefined);return {kind:'undefined'};};
    const owner={name:'polymarket',grafanaEndpointsEvery:[],grafanaEndpointsPeriodic:[],emitter:{emit:()=>{}}};
    const config={baseURL:'https://venue',adapter:async config=>{
     events.push('send');sends++;
     seen={own:Object.hasOwn(config,key),value:view(config[key])};
     return {status:200,data:{},headers:{},config};
    }};
    const xhr=moduleValue.exports[factory==='direct'?'safeCexAxiosCreateNoProxy':'safeCexAxiosCreate'](config,{key:'',secret:''},owner);
    xhr.interceptors.request.use(config=>{
     events.push('prepare');originalConfig=config;
     if(mode!=='fresh')Object.defineProperty(config,key,{value:mode==='readonly-undefined'?undefined:-0,writable:!mode.startsWith('readonly'),enumerable:true,configurable:true});
     return config;
    });
    await xhr.get('/diagnostic-start');
    assert.equal(seen.own,true);
    const untouched=mode==='readonly';
    assert.deepEqual(seen.value,mode==='readonly-undefined'?{kind:'undefined'}:{kind:'number',bits:bits(untouched?-0:Math.floor(epoch))});
    assert.deepEqual(view(originalConfig[key]),seen.value);assert.equal(sends,1);assert.deepEqual(events,['prepare','send']);
    rows.push({factory,epoch,mode,...seen,prepared:view(originalConfig[key]),sends,events});
   }
 } finally { Date.now=originalNow; }
 assert.equal(rows.length,32);return rows;
}
async function diagnosticBindingAudit(kind) {
 const cache=new Map();
 let token='';
 const mocks={
  nanoid:{nanoid:()=> 'audit-id'},
  '../../accessToken':{getProxyToken:async()=>token},
  '../../httpOverWebsocket/axiosHttpOverWebsocketAdapter':{},
  './routing/resolveExchangeRouting':{resolveExchangeRouting:()=>({restProxyHost:'https://proxy'})},
  './transportResolution':{getPublicCorsProxy:()=> 'https://proxy'},
  '@tealstreet/http-over-websocket/src/tunnelEndpoints':{isRelayUrl:()=>false},
 };
 function load(file){
  file=path.resolve(file);if(cache.has(file))return cache.get(file).exports;
  const mod={exports:{}};cache.set(file,mod);
  const js=ts.transpileModule(fs.readFileSync(file,'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022,esModuleInterop:true}}).outputText;
  new Function('require','module','exports',js)(name=>{
   if(Object.hasOwn(mocks,name))return mocks[name];
   if(name==='axios')return axios;
   if(name.startsWith('.'))return load(path.resolve(path.dirname(file),name)+'.ts');
   return require(name);
  },mod,mod.exports);return mod.exports;
 }
 const names=['safeCexRestDiagProxyHeader','safeCexRestDiagSignMs','safeCexRestDiagSignedAt','safeCexRestDiagSignatureAgeMs'];
 const bits=n=>{const b=Buffer.alloc(8);b.writeDoubleBE(n);return b.toString('hex');};
 const view=config=>({...Object.fromEntries(names.map(key=>[key,!Object.hasOwn(config,key)?{kind:'absent'}:typeof config[key]==='number'?{kind:'number',bits:bits(config[key])}:{kind:typeof config[key],value:config[key]}])),order:Object.keys(config).filter(key=>names.includes(key))});
 const rows=[];
 const originalNow=Date.now;Date.now=()=>1000;
 try {
  if(kind==='consumer'){
   const diagnostics=load(path.join(root,'packages/safe-cex/src/utils/venueRestDiagnostics.ts'));
   Object.assign(imported['./utils/venueRestDiagnostics'],diagnostics);
   Object.assign(imported['./utils/axiosErrorEnhancer'],load(path.join(root,'packages/safe-cex/src/utils/axiosErrorEnhancer.ts')));
   provider={rest:({config})=>({baseURL:config.baseURL,proxied:false})};
   for(const factory of ['routed','direct'])for(const outcome of ['success','failure'])
    for(const sign of ['number','zero','nan','text','throw','undefined','null','boolean','infinity','negative-infinity','hidden-number','inherited-number','terminal-number','mutable-number','sealed-number'])for(const authorization of ['bearer','malformed','absent','unmarked']){
     diagnostics.resetVenueRestDiagnostics();const events=[];let success=true,sends=0;
     const owner={name:'polymarket',grafanaEndpointsEvery:[],grafanaEndpointsPeriodic:[],emitter:{emit:()=>{}}};
     const xhr=moduleValue.exports[factory==='direct'?'safeCexAxiosCreateNoProxy':'safeCexAxiosCreate']({baseURL:'https://venue',adapter:async config=>{
      sends++;const response={status:outcome==='success'?200:503,statusText:'',data:{opaque:[NaN,Infinity,-0,undefined],retCode:0},headers:{server:'audit-edge','x-bapi-limit-status':'12','retry-after':'3'},config};
      if(outcome==='failure')throw new axios.AxiosError('Request failed with status code 503',undefined,config,undefined,response);
      return response;
     }},{key:'',secret:''},owner);
     xhr.interceptors.request.use(config=>{
      let target=config;
      if(sign==='inherited-number'||sign==='terminal-number'){target=Object.create(sign==='terminal-number'?null:Object.prototype);Object.setPrototypeOf(config,target);}
      if(sign==='mutable-number')config.safeCexRestDiagSignMs=7;
      else Object.defineProperty(target,'safeCexRestDiagSignMs',{enumerable:sign!=='hidden-number',configurable:true,get(){events.push('sign');if(sign==='throw')throw Error('audit getter');return sign.endsWith('-number')?7:({number:7,zero:-0,nan:NaN,text:'7',undefined:undefined,null:null,boolean:true,infinity:Infinity,'negative-infinity':-Infinity})[sign];}});
      if(authorization!=='unmarked')config.safeCexRestDiagProxyHeader='present';
      if(authorization==='bearer')config.headers.set('Authorization','Bearer audit-token');
      if(authorization==='malformed')config.headers.set('Authorization','Basic audit-token');
      if(sign==='sealed-number')Object.preventExtensions(config);
      return config;
     });
     try{await xhr.get('/diagnostic-consumer');}catch{success=false;}
     const stats=diagnostics.getVenueRestStats().polymarket;
     const samples=diagnostics.drainVenueRestFailureSamples();
     assert.equal(sends,sign==='throw'?0:1);assert.equal(success,outcome==='success'&&sign!=='throw');assert.equal(stats.requests,1);assert.equal(stats[outcome==='success'&&sign!=='throw'?'ok':'failures'],1);
     rows.push({factory,outcome,sign,authorization,success,sends,events,stats:{requests:bits(stats.requests),ok:bits(stats.ok),failures:bits(stats.failures),total:typeof stats.totalP50Ms==='number'?bits(stats.totalP50Ms):null,sign:typeof stats.signP50Ms==='number'?bits(stats.signP50Ms):null},samples:samples.map(x=>({proxy:x.restProxyHeaderSent??null,sign:typeof x.signMs==='number'?bits(x.signMs):null,name:x.errorName??null,message:x.errorMessage??null,path:x.path??null,endpoint:x.endpoint??null,bodyKind:x.bodyKind,headers:x.headers,status:typeof x.status==='number'?bits(x.status):null}))});
   }
   assert.equal(rows.length,240);
  }else if(kind==='proxy'){
   const {tealstreetTransport}=load(path.join(root,'packages/safe-cex/src/transport/tealstreet/tealstreetTransport.ts'));
   for(const exchange of ['okx','grvt','mexc'])for(token of ['', 'audit-token'])for(const replacement of [false,true])for(const mode of ['fresh','readonly','readonly-undefined']){
    provider=tealstreetTransport;
    imported['./transport/registry'].getTransport=()=>({rest:context=>tealstreetTransport.rest({...context,exchange})});
    const xhr=moduleValue.exports.safeCexAxiosCreate({baseURL:'https://venue',adapter:async config=>{
     if(replacement)config.headers=new axios.AxiosHeaders({'X-Venue':'replacement'});
     return {status:200,data:{},headers:{},config};
    }},{key:'',secret:'',tealstreetProxy:true},{name:exchange,grafanaEndpointsEvery:[],grafanaEndpointsPeriodic:[],emitter:{emit:()=>{}}});
    xhr.interceptors.request.use(config=>{
     if(mode!=='fresh')Object.defineProperty(config,names[0],{value:mode==='readonly-undefined'?undefined:-0,writable:false,enumerable:true,configurable:true});
     return config;
    });
    const {config}=await xhr.get('/diagnostic');
    assert.ok(Object.is(config[names[0]],mode==='fresh'?(token?'present':'empty'):mode==='readonly'?-0:undefined));
    assert.ok(!Object.hasOwn(config.headers,names[0]));
    rows.push({exchange,token:!!token,replacement,mode,view:view(config),authorization:config.headers.get(exchange==='okx'?'Authorization':'Tealstreet-Authorization')??null});
   }
   assert.equal(rows.length,36);
  }else{
   const {createRemoteSigningAdapter}=load(path.join(root,'packages/safe-cex/src/remoteSigningAdapter.ts'));
   for(const stamp of ['900','1000','1100','0','Infinity','garbage'])for(const mode of ['signed','skip','network','sign-error'])for(const delay of [0,17]){
    Date.now=()=>1000;let sent,signs=0,error=null;
    const signer={sign:async request=>{signs++;Date.now=()=>1000+delay;if(mode==='sign-error')return {id:request.id,error:'audit signer refused'};return {id:request.id,config:{headers:{'X-BAPI-TIMESTAMP':stamp}}};}};
    const adapter=createRemoteSigningAdapter(signer,'bybit',()=>async config=>{
     sent=config;
     if(mode==='network')throw Object.assign(Error('audit network failure'),{config});
     return {status:200,data:{},headers:{},config};
    });
    const request={method:'get',url:'/diagnostic',headers:new axios.AxiosHeaders(),safeCexSkipRemoteSigning:mode==='skip',safeCexRestDiagSignatureAgeMs:-0};
    try{await adapter(request);}catch(e){error=e.message;}
    assert.equal(signs,mode==='skip'?0:1);
    if(mode!=='skip'&&mode!=='sign-error')assert.equal(sent[names[1]],delay);
    rows.push({stamp,mode,delay,signs,error,view:sent?view(sent):null});
   }
   assert.equal(rows.length,48);
  }
 } finally {Date.now=originalNow;}
 return rows;
}
(async()=>{
 if(process.argv[2]==='consumer-diagnostics'){process.stdout.write(JSON.stringify(await diagnosticBindingAudit('consumer')));return;}
 if(process.argv[2]==='proxy-diagnostics'||process.argv[2]==='sign-diagnostics'){process.stdout.write(JSON.stringify(await diagnosticBindingAudit(process.argv[2]==='proxy-diagnostics'?'proxy':'sign')));return;}
 if(process.argv[2]==='request-start'){process.stdout.write(JSON.stringify(await requestStartAudit()));return;}
 if(process.argv[2]==='outcomes'){process.stdout.write(JSON.stringify(await outcomeAudit()));return;}
 const rows=[];
 for(const proxied of [false,true])for(const failAuth of [false,true]){
  const events=[];
  provider={rest:({config})=>({baseURL:proxied?'https://proxy/'+config.baseURL:config.baseURL,proxied,authorize:async config=>{events.push('authorize');if(failAuth)throw Error('token rejected');config.headers.set('Proxy-Authorization','Bearer token');}})};
  let wire=null;
  const xhr=moduleValue.exports.safeCexAxiosCreate({baseURL:'https://venue',timeout:31,headers:{'X-Venue':'initial'},adapter:async config=>{events.push('send');wire={baseURL:config.baseURL,timeout:config.timeout,headers:config.headers.toJSON()};return {status:200,data:{ok:true},headers:{},config};}},{key:'',secret:''},{name:'polymarket',grafanaEndpointsEvery:[],grafanaEndpointsPeriodic:[],emitter:{emit:()=>{}}});
  xhr.interceptors.request.use(config=>{events.push('venue');config.headers.set('X-Venue','signed');return config;});
  let error=null;try{await xhr.get('/book');}catch(e){error=e.message;}
  rows.push({proxied,failAuth,events,wire,error});
 }
 assert.equal(rows.length,4);assert.deepEqual(rows[3].events,['venue','authorize']);
 process.stdout.write(JSON.stringify(rows));
})().catch(e=>{console.error(e);process.exitCode=1;});
