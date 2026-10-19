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
(async()=>{
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
