// Execute real Axios transforms and axios-retry before defining the Rust factory seam.
const fs=require('node:fs'),path=require('node:path');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const runtimeRequire=process.env.SAFE_CEX_TYPESCRIPT_PATH ? require('node:module').createRequire(require.resolve(process.env.SAFE_CEX_TYPESCRIPT_PATH)) : require;
const axios=runtimeRequire('axios'),retry=runtimeRequire('axios-retry'),createHmac=runtimeRequire('create-hmac'),qs=runtimeRequire('qs');
const {root,load:loadTables}=require('./tables.cjs');
async function capture(venue,method,url,data,failure,elapsed=0,extra={}) {
 const originalNow=Date.now;let retryClock=100000;Date.now=()=>retryClock;
 let clock=1700000000000,attempts=0;const hashes=[],wire=[],signRequests=[];
 const adapter=async config=>{
  attempts++;
  wire.push({url:axios.getUri({...config,baseURL:undefined}),method:config.method,data:config.data??null,headers:config.headers.toJSON(),...(elapsed?{timeout:config.timeout}:{})});
  retryClock+=elapsed;
  if(attempts<=(extra.failures??3)){
   const response=failure==='response'?{status:503,statusText:'Unavailable',headers:{},data:{code:'fixture-error'},config}:undefined;
   throw new axios.AxiosError('fixture failure','ECONNRESET',config,{},response);
  }
  return {status:200,statusText:'OK',headers:{},data:{code:'200000'},config};
 };
 const cache=new Map();
 function load(file){
  if(cache.has(file))return cache.get(file).exports;
  const module={exports:{}};cache.set(file,module);
  const code=ts.transpileModule(fs.readFileSync(file,'utf8'),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS,esModuleInterop:true}}).outputText;
  new Function('module','exports','require',code)(module,module.exports,name=>{
   if(name==='axios')return axios;if(name==='axios-retry')return retry;if(name==='qs')return qs;
   if(name==='create-hmac')return (algorithm,key)=>{const hmac=createHmac(algorithm,key);let preimage='';return{update(data){preimage+=data;hmac.update(data);return this;},digest(encoding){const digest=hmac.digest(encoding);hashes.push({preimage,digest});return digest;}};};
   if(name.endsWith('/safeCexAxiosCreate'))return {safeCexAxiosCreate:(config,options)=>{
    const inner=extra.remote?load(path.join(root,'packages/safe-cex/src/remoteSigningAdapter.ts')).createRemoteSigningAdapter(options.remoteSigner,venue,()=>adapter):adapter;
    return axios.create({...config,timeout:20000,adapter:inner});
   },installTransportAdapter:()=>{}};
   if(name==='nanoid')return runtimeRequire('nanoid');
   if(name.endsWith('/virtualClock'))return {virtualClock:{getCurrentTimeMs:()=>clock++}};
   if(name.endsWith('.types'))return loadTables(path.resolve(path.dirname(file),name+'.ts'));
   if(name==='../base')return {BaseExchange:class{}};
   if(name.startsWith('.'))return load(path.resolve(path.dirname(file),name+'.ts'));
   throw Error(`unexpected API retry dependency ${name}`);
  });return module.exports;
 }
 const factory=load(path.join(root,`packages/safe-cex/src/exchanges/kucoin/${venue}.api.ts`));
 if(extra.signatureCorpus){Date.now=originalNow;return extra.signatureCorpus.map(data=>factory.isKucoinSignatureRejection(data));}
 const options={key:'fixture-key',secret:'fixture-secret',passphrase:'fixture-pass',...extra.options};
 const remoteSigner={sign:async request=>{signRequests.push(request.config);return {id:request.id,config:request.config};}};
 if(extra.remote)options.remoteSigner=remoteSigner;
 const api=(factory.createAPI||factory.createKucoinSpotAPI)(options,{name:venue});
 const optionViews=extra.afterCreate?{before:JSON.parse(JSON.stringify(options))}:undefined;
 if(extra.afterCreate){Object.assign(options,extra.afterCreate);optionViews.after=JSON.parse(JSON.stringify(options));}
 let outcome;try{const response=await api.request({method,url,data,params:{q:'a b'},...extra.request});outcome={ok:response.data};}catch(error){outcome={error:error.message,status:error.response?.status??null};}
 Date.now=originalNow;
 return {venue,method,url,data:data??null,failure,...(elapsed?{elapsed}:{}),wire,hashes,outcome,...(optionViews?{optionViews}:{}),...(extra.extended?{extra,dataPresent:data!==undefined,signRequests}:{})};
}
module.exports={capture};
if(require.main===module)(async()=>{
 const out=[];
 for(const venue of ['kucoin','kucoinspot']){
  for(const [method,url,data,failure,elapsed] of [
   ['get','/api/v1/accounts',{symbol:'BTC USDT'},'network'],
   ['post','/api/v1/bullet-private',{symbol:'BTC USDT'},'network'],
   ['post','/api/v1/orders',{symbol:'BTC USDT'},'network'],
   ['get','/api/v1/accounts',false,'network'],
   ['get','/api/v1/accounts',undefined,'response'],
   ['get','/api/v1/accounts',undefined,'network',8000],
   ['get','/api/v1/accounts',undefined,'network',21000],
  ])out.push(await capture(venue,method,url,data,failure,elapsed));
 }
 process.stdout.write(JSON.stringify(out,null,2)+'\n');
})().catch(error=>{console.error(error);process.exitCode=1;});
