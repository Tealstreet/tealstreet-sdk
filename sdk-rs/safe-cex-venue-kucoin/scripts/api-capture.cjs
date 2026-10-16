// Capture live factory interceptor preimages before implementing the Rust signer.
const fs=require('node:fs'), path=require('node:path');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const {root,load:loadTables}=require('./tables.cjs');
const runtimeRequire = process.env.SAFE_CEX_TYPESCRIPT_PATH
  ? require('node:module').createRequire(require.resolve(process.env.SAFE_CEX_TYPESCRIPT_PATH))
  : require;
const axios=runtimeRequire('axios');
const createHmac=runtimeRequire('create-hmac');
const qs=runtimeRequire('qs');
function capture(venue,opts,config) {
 const hashes=[];let interceptor;let defaults;let retries;let installed=false;
 const cache=new Map();
 function load(file) {
  if(cache.has(file))return cache.get(file).exports;
  const module={exports:{}};cache.set(file,module);
  const code=ts.transpileModule(fs.readFileSync(file,'utf8'),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS,esModuleInterop:true}}).outputText;
  new Function('module','exports','require',code)(module,module.exports,specifier=> {
   if(specifier==='axios')return axios;
   if(specifier==='qs')return qs;
   if(specifier==='create-hmac')return (algorithm,key)=>{
    const hmac=createHmac(algorithm,key);let text='';
    return {update(data){text+=data;hmac.update(data);return this;},digest(encoding){const digest=hmac.digest(encoding);hashes.push({algorithm,key,preimage:text,digest});return digest;}};
   };
   if(specifier==='axios-retry')return Object.assign((xhr,options)=>{retries=options.retries;},{isNetworkError:()=>false});
   if(specifier.endsWith('/safeCexAxiosCreate'))return {
    safeCexAxiosCreate(options){defaults=options;return {interceptors:{request:{use(fn){interceptor=fn;}},response:{use(){}}}};},
    installTransportAdapter(){installed=true;},
   };
   if(specifier.endsWith('/virtualClock'))return {virtualClock:{getCurrentTimeMs:()=>1700000000123}};
   if(specifier==='../base')return {BaseExchange:class{}};
   if(specifier.endsWith('.types'))return loadTables(path.resolve(path.dirname(file),specifier+'.ts'));
   if(specifier.startsWith('.'))return load(path.resolve(path.dirname(file),specifier+'.ts'));
   throw Error(`unexpected live dependency ${specifier}`);
  });
  return module.exports;
 }
 const factory=load(path.join(root,`packages/safe-cex/src/exchanges/kucoin/${venue}.api.ts`));
 (factory.createAPI || factory.createKucoinSpotAPI)(opts,{name:venue});
 const initial={...config,headers:new axios.AxiosHeaders({...defaults.headers,...config.headers})};
 const prepared=interceptor(initial);
 let wireBody=prepared.data;
 for(const transform of axios.defaults.transformRequest) wireBody=transform.call(prepared,wireBody,prepared.headers);
 if(wireBody===undefined)wireBody=null;
 return {hashes,headers:prepared.headers.toJSON(),baseURL:defaults.baseURL,timeout:defaults.timeout,retries,installed,wireUrl:axios.getUri({...prepared,baseURL:undefined,paramsSerializer:defaults.paramsSerializer}),wireBody};
}
const opts={key:'fixture-key',secret:'fixture-secret',passphrase:'fixture-pass',extra:{kucoinBroker:{partner:' partner ',brokerKey:' broker-key ',brokerName:' broker-name '}}};
const result=[];
for(const venue of ['kucoin','kucoinspot']) {
 for(const [name,body] of [['complete',{symbol:'BTC-USDT',size:2,note:'x y'}],['missing-optional',{symbol:'BTC-USDT',size:2,optional:undefined}],['raw-json-string','{"raw":"body"}'],['raw-text','raw body'],['false',false]]) {
  result.push({venue,name,...capture(venue,opts,{method:'post',url:'/api/v1/orders',params:{symbol:'BTC USDT',nested:{key:'a+b'},ids:[1,2]},data:body})});
 }
 result.push({venue,name:'public-only',...capture(venue,{...opts,publicDataOnly:true},{method:'get',url:'/api/v1/accounts'})});
 result.push({venue,name:'remote',...capture(venue,{...opts,remoteSigner:{}},{method:'post',url:'/api/v1/orders',data:{size:2},headers:{'kc-api-timestamp':'123'}})});
}
process.stdout.write(JSON.stringify(result,null,2)+'\n');
