import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import path from 'node:path';
import {createRequire} from 'node:module';
import {load,root,ts} from './source.mjs';
const require=createRequire(path.join(path.resolve(process.env.SAFE_CEX_TYPESCRIPT_PATH),'../mexc-rich-factory.cjs'));
const axios=require('axios');const realHmac=require('create-hmac');
const retryModule=require('axios-retry');const retry=retryModule.default??retryModule;
const qs=require('qs');
const filename=root+'packages/safe-cex/src/exchanges/mexc/mexc.api.ts';
const source=fs.readFileSync(filename,'utf8');const ast=ts.createSourceFile(filename,source,ts.ScriptTarget.Latest,true);
const names=new Set(['createAPI','serializeMexcParams','signMexcRequest','isMexcPublicEndpoint','normalizeRequestData','getQueryString']);
const selected=ast.statements.filter(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>names.has(d.name.getText(ast))));
assert.equal(selected.length,6,'SOURCE independent six real factory/helper declarations');
const code=ts.transpileModule(selected.map(n=>n.getText(ast)).join('\n'),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
const types=load('packages/safe-cex/src/exchanges/mexc/mexc.types.ts');
const {markSkipRemoteSigning}=load('packages/safe-cex/src/remoteSigningAdapter.ts');
const {networkErrorOnRead}=load('packages/safe-cex/src/utils/retryReadsOnly.ts');
const {bigIntAsStringJsonParse}=load('packages/json-parse/src/jsonParse.ts');
const bits=value=>{if(Number.isNaN(value))return'7ff8000000000000';const b=Buffer.alloc(8);b.writeDoubleBE(value);return b.toString('hex');};
const observe=value=>typeof value==='number'?{number:bits(value)}:Array.isArray(value)?value.map(observe):value&&typeof value==='object'?Object.fromEntries(Object.entries(value).map(([k,v])=>[k,observe(v)])):value;
const array=Array.from({length:11},(_,i)=>'v'+i);
const unicodeMode=process.argv.includes("--unicode");
const exhaustedMode=process.argv.includes("--exhausted");
const richCases=[
 {label:'historical',params:{symbol:'BTC_USDT',start_time:NaN,end_time:-Infinity,page_num:1,page_size:100},keys:['symbol','start_time','end_time','page_num','page_size'],query:'end_time=-Infinity&page_num=1&page_size=100&start_time=NaN&symbol=BTC_USDT',failures:0},
 {label:'repeat-array',params:{z:array,zero:-0,positive:Infinity},keys:['z','zero','positive'],query:'positive=Infinity&z=v0&z=v1&z=v10&z=v2&z=v3&z=v4&z=v5&z=v6&z=v7&z=v8&z=v9&zero=0',failures:0},
 {label:'unicode-ties',params:{'é':'first','e\u0301':'second',a:'before'},keys:['é','e\u0301','a'],query:'a=before&%C3%A9=first&e%CC%81=second',failures:0},
 {label:'public',params:{zero:-0,positive:Infinity},keys:['zero','positive'],query:'positive=Infinity&zero=0',public:true,failures:0},
 {label:'retry',params:{from:NaN,to:-0},keys:['from','to'],query:'from=NaN&to=0',failures:2},
];
const units=text=>Array.from({length:text.length},(_,i)=>text.charCodeAt(i));
const cases=exhaustedMode ? [{label:"exhausted",params:{from:NaN,to:-0},keys:["from","to"],query:"from=NaN&to=0",failures:4}] : unicodeMode ? [
 {label:'lone-key',key:'\uD800'}, {label:'bmp-key',key:'é'}, {label:'astral-key',key:'😃'},
 {label:'internal-space-key',key:'a\uFEFF\u3000b'}, {label:'trimmed-key',key:' \t'},
].map(spec=>({...spec,params:{},keys:[],query:'',failures:0})) : richCases;
const {normalizeExecution}=load('packages/safe-cex/src/exchanges/mexc/mexc.utils.ts');
let factories=0,clocks=0,hmacCalls=0,adapters=0,dateCalls=0,timerCalls=0;const timerDelays=[];
const outputs=[];const originalNow=Date.now;const originalSetTimeout=globalThis.setTimeout;let active;
Date.now=()=>{dateCalls++;return 1791800000123+(active?.attempt??0);};
globalThis.setTimeout=(callback,delay,...args)=>{timerCalls++;timerDelays.push(delay);return originalSetTimeout(callback,delay,...args);};
try{
 const context={exports:{},...types,AxiosHeaders:axios.AxiosHeaders,retry,qs,networkErrorOnRead,bigIntAsStringJsonParse,markSkipRemoteSigning,
  virtualClock:{getCurrentTimeMs(){clocks++;active.clock++;return 1791800000123+active.attempt;}},
  createHmac(...args){const hash=realHmac(...args);const update=hash.update.bind(hash);hash.update=value=>{hmacCalls++;active.preimages.push(value);return update(value);};return hash;},
  // Declared shared-factory child boundary. Actual MEXC body, Axios, headers,
  // transforms, retry predicate/owner, qs and HMAC execute unchanged.
  safeCexAxiosCreate(config){factories++;active.factory++;
   assert.equal(config.timeout,20000,'SOURCE actual MEXC factory timeout');
   assert.equal(typeof config.paramsSerializer.serialize,'function','SOURCE configured actual query serializer');
   return axios.create({...config,adapter:async request=>{
    adapters++;active.attempt++;
    assert.deepEqual(Object.keys(request.params),active.spec.keys,'SOURCE original native query key order before wire');
    assert.equal(typeof request.params,'object','SOURCE actual query object');
    if(active.spec.label==='historical'){assert(Number.isNaN(request.params.start_time),'SOURCE raw NaN before URI rendering');assert.equal(request.params.end_time,-Infinity);}
    if(['repeat-array','public'].includes(active.spec.label)){assert(Object.is(request.params.zero,-0),'SOURCE native signed zero before URI rendering');assert.equal(request.params.positive,Infinity);}
    if(active.spec.label==='repeat-array'){assert.deepEqual(request.params.z,array,'SOURCE array retains native original index order');}
    if(['retry','exhausted'].includes(active.spec.label)){assert(Number.isNaN(request.params.from));assert(Object.is(request.params.to,-0));}
    const raw=observe(request.params);
    const wireURL=axios.getUri(request);
    const endpoint=active.spec.public?'/api/v1/contract/detail':'/private';
    assert.equal(wireURL,types.BASE_URL.livenet+endpoint+(active.spec.query?'?'+active.spec.query:''),'SOURCE actual Axios wire URI');
    const headers=request.headers.toJSON();
    if(active.spec.public){assert.equal(headers.Signature,undefined,'SOURCE public query does not sign');}
    else{
     const time=String(1791800000123+active.attempt-1);
     assert.equal(headers['Request-Time'],time,'SOURCE deterministic actual signing clock');
     const preimage=(active.spec.key??'fixture-key')+time+active.spec.query;
     assert.equal(active.preimages[active.attempt-1],preimage,'SOURCE observed actual HMAC update preimage');
     assert.equal(headers.Signature,realHmac('sha256','fixture-secret').update(preimage).digest('hex'),'SOURCE actual HMAC signature');
    }
    active.capture.push({raw,wireURL,time:headers['Request-Time']??null,signature:headers.Signature??null,apiKeyUnits:headers.ApiKey===undefined?null:units(headers.ApiKey)});
    if(unicodeMode)assert.deepEqual(units(headers.ApiKey),active.spec.label==='trimmed-key'?[]:units(active.spec.key),'SOURCE actual program-readable API-key UTF16 units');
    if(active.attempt<=active.spec.failures)throw new axios.AxiosError('network lost','ECONNRESET',request);
    return{status:200,statusText:'OK',headers:{},data:unicodeMode?'{"code":0,"data":[{"symbol":"\\ud800","vol":2,"price":3}]}':'{"code":0,"data":[]}',config:request};
   }});
  },
 };
 vm.runInNewContext(code,context,{filename});
 for(const spec of cases){active={spec,attempt:0,factory:0,clock:0,preimages:[],capture:[]};
  const client=context.exports.createAPI({key:spec.key??'fixture-key',secret:'fixture-secret'},{name:'mexc'});
  let response,failure=null;
  try {response=await client.get(spec.public?'/api/v1/contract/detail':'/private',{params:spec.params});}
  catch(error){
   if(!exhaustedMode)throw error;
   assert.equal(error.code,'ECONNRESET','Source exhausted native original error code');
   assert(Number.isNaN(error.config.params.from),'Source exhausted raw NaN retained');assert(Object.is(error.config.params.to,-0),'Source exhausted raw signed zero retained');
   assert.deepEqual(Object.keys(error.config.params),['from','to'],'Source exhausted original query order');
   assert.equal(error.config.url,'/private','Source exhausted original URL');assert.equal(error.config.timeout,19997,'Source exhausted measured timeout');
   assert.equal(error.response,undefined,'Source exhausted no response');
   failure={code:error.code,url:error.config.url,timeout:error.config.timeout,raw:observe(error.config.params),hasResponse:error.response!==undefined};
  }
  const expectedAttempts=exhaustedMode?4:spec.label==='retry'?3:1;
  assert.equal(active.factory,1,'SOURCE independent factory calls');assert.equal(active.attempt,expectedAttempts,'SOURCE independent adapter attempts');
  assert.equal(active.clock,spec.public?0:expectedAttempts,'SOURCE independent venue virtual clock calls');
  assert.equal(active.preimages.length,spec.public?0:expectedAttempts,'SOURCE independent HMAC update calls');
  assert.deepEqual(Object.keys(spec.params),spec.keys,'SOURCE original producer key order after actual factory requests');
  if(!exhaustedMode)assert.equal(response.data.code,0,'SOURCE real response transform result');
  let responseUnits=null;
  if(unicodeMode){const fill=normalizeExecution(response.data.data[0]);assert.deepEqual(units(fill.symbol),[55296],'SOURCE actual transformed response execution UTF16 symbol');assert.equal(fill.qty,2);assert.equal(fill.price,3);responseUnits=units(fill.symbol);}
  outputs.push({label:spec.label,attempts:active.attempt,clock:active.clock,preimages:unicodeMode?active.preimages.map(units):active.preimages,capture:active.capture,failure,responseUnits});
 }
 if(exhaustedMode){assert.equal(factories,1);assert.equal(clocks,4);assert.equal(hmacCalls,4);assert.equal(adapters,4);assert.equal(dateCalls,7);assert.equal(timerCalls,3);assert.deepEqual(timerDelays,[0,0,0]);assert.equal(outputs.length,1);}else if(unicodeMode){assert.equal(factories,5);assert.equal(clocks,5);assert.equal(hmacCalls,5);assert.equal(adapters,5);assert.equal(dateCalls,5);assert.equal(timerCalls,0);assert.equal(outputs.length,5);}else{
 assert.equal(factories,5,'SOURCE independent total factory calls');assert.equal(clocks,6,'SOURCE independent total signing clock calls');assert.equal(hmacCalls,6,'SOURCE independent total real HMAC updates');assert.equal(adapters,7,'SOURCE independent total adapter calls');
 assert.equal(outputs.length,5,'SOURCE independent rich factory root floor');
 assert.equal(dateCalls,9,'SOURCE independent axios-retry Date.now calls');
 assert.equal(timerCalls,2,'SOURCE independent zero-delay retry scheduling calls');
 assert.deepEqual(timerDelays,[0,0],'SOURCE actual zero-delay retry schedule');
 }
}finally{Date.now=originalNow;globalThis.setTimeout=originalSetTimeout;}
console.log(JSON.stringify(outputs));
