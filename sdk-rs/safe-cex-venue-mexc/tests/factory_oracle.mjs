import {load,boundaries,root} from './source.mjs';
import path from 'node:path';
import {createRequire} from 'node:module';
const require=createRequire(path.join(path.resolve(process.env.SAFE_CEX_TYPESCRIPT_PATH),'../mexc-factory.cjs'));
const axios=require('axios'),hmac=require('create-hmac');
let capture,plan,preimages,attempt;
// Axios retry elapsed-time accounting uses the same deterministic attempt clock.
Date.now=()=>1791800000123+(attempt??0);
boundaries.set('create-hmac',(...args)=>{const hash=hmac(...args),update=hash.update.bind(hash);hash.update=value=>{preimages.push(value);return update(value);};return hash;});
boundaries.set(path.join(root,'packages/safe-cex/src/utils/virtualClock.ts'),{virtualClock:{getCurrentTimeMs:()=>1791800000123+attempt}});
// Only the shared transport factory and clock are host boundaries. Venue source,
// AxiosHeaders/interceptors/transforms, retries, qs and HMAC execute unchanged.
boundaries.set(path.join(root,'packages/safe-cex/src/safeCexAxiosCreate.ts'),{safeCexAxiosCreate(config){
 return axios.create({...config,timeout:20000,adapter:async request=>{
  attempt++;
  capture.push({method:request.method,url:request.url,baseURL:request.baseURL,timeout:request.timeout,
    headers:request.headers.toJSON(),data:request.data??null,skip:Object.keys(request).some(k=>k.includes('SkipRemoteSigning')&&request[k]===true),
    wireURL:axios.getUri(request)});
  if(attempt <= (plan.failures??0))throw new axios.AxiosError('network lost','ECONNRESET',request);
  return {status:200,statusText:'OK',headers:{},data:plan.response??'{"code":0,"data":{"id":9007199254740993123}}',config:request};
 }});
}});
process.env.MEXC_TS_FACTORY='1';
const api=load('packages/safe-cex/src/exchanges/mexc/mexc.api.ts');
const scenarios=[];
const configs=[
 {url:'/private?z=2&a=1'},
 {url:'/private',params:{z:1,ä:[2,null],A:3}},
 {method:'post',url:'/private',data:{z:1,a:null,omitted:undefined}},
 {method:'post',url:'/private',data:'{"z":1,"a":2}'},
 {method:'post',url:'/private',data:'raw text'},
 ...[null,false,0,''].map(data=>({method:'post',url:'/private',data})),
 {method:'head',url:'/private?x=1',data:{a:1}},
 {url:'/api/v1/contract/detail',params:{z:1,a:2}},
 {url:'/private',headers:{apikey:'old-api-key','request-time':'old-request-time',signature:'old-signature','content-type':'text/plain'}},
];
for(const config of configs)for(const options of [{key:'key',secret:'secret'}, {key:'',secret:'secret'}, {key:'key',secret:'secret',publicDataOnly:true}, {key:'key',secret:'secret',remoteSigner:{}}])scenarios.push({config,options});
for(const response of ['bad json','{"success":false,"message":"denied","code":511}','{"code":"200","msg":"ok"}','{"code":false,"message":"false-code"}','{"code":null,"message":{"x":1}}','{"success":false,"message":0,"msg":"fallback"}','{"code":9,"message":""}','[1,2,3]'])scenarios.push({config:{url:'/private'},options:{key:'key',secret:'secret'},response});
for(const method of ['get','head','post','delete'])scenarios.push({config:{method,url:'/private'},options:{key:'key',secret:'secret'},failures:3});
scenarios.push({config:{url:'/private'},options:{key:'key',secret:'secret',testnet:true,extra:{recvWindow:12345}}});
for(const data of ['  {"a":1} \n','  [1,2]  ','  null  ',' true ',' 1e400 ',' "text" ','\uFEFF{"a":1}','\u0085{"a":1}','   ','NaN'])scenarios.push({config:{method:'post',url:'/private',data},options:{key:'key',secret:'secret'}});
if(process.argv.includes('--retry-body')) {
 scenarios.length=0;
 for(const data of ['raw text','  {"a":1} \n','']) scenarios.push({config:{method:'head',url:'/private',data},options:{key:'key',secret:'secret'},failures:1});
}
if(process.argv.includes('--response-numbers')) {
 scenarios.length=0;
 for(const response of ['{"code":0,"data":-0}','{"code":0,"data":1e400}','{"code":0,"data":-1e400}','{"code":0,"data":[-0,1e400,-1e400,0,1.25]}']) scenarios.push({config:{url:'/private'},options:{key:'key',secret:'secret'},response});
}
if(process.argv.includes('--response-units')) {
 scenarios.length=0;
 for(const response of ['{"code":0,"data":[{"symbol":"\\ud800","vol":2,"price":3}]}', '{"code":0,"data":[{"symbol":"'+String.fromCharCode(0xd800)+'","vol":2,"price":3}]}']) scenarios.push({config:{url:'/private'},options:{key:'key',secret:'secret'},response});
}
// Observe the number before JSON.stringify erases -0 and non-finite values.
function observe(value) {
 if(typeof value==='number'){if(Number.isNaN(value))return {$number:'7ff8000000000000'};const bytes=Buffer.alloc(8);bytes.writeDoubleBE(value);return {$number:bytes.toString('hex')};}
 if(Array.isArray(value))return value.map(observe);
 if(value && typeof value==='object')return Object.fromEntries(Object.entries(value).map(([k,v])=>[k,observe(v)]));
 return value;
}
const results=[];
for(const scenario of scenarios){capture=[];preimages=[];attempt=0;plan=scenario;let result;try{const response=await api.createAPI(scenario.options,{name:'mexc'}).request(scenario.config);result={data:response.data};}catch(error){result={error:error.message,hasResponse:!!error.response,data:error.response?.data??null};}results.push({scenario,capture,preimages,result,observedResult:observe(result)});}
if(process.argv.includes('--response-units')) {
 process.stdout.write(JSON.stringify(results.map(row=>({symbolUnits:Array.from(row.result.data.data[0].symbol,c=>c.charCodeAt(0)),code:row.result.data.code,vol:row.result.data.data[0].vol,price:row.result.data.data[0].price,http:row.capture.length}))));
} else {
process.stdout.write(JSON.stringify(results.map(item=>({captureArchitecture:process.arch,captureNodeMajor:Number(process.versions.node.split('.')[0]),...item}))));

}
