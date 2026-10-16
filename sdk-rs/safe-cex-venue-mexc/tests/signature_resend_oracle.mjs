import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import path from 'node:path';
import {createRequire} from 'node:module';
import {load,root,ts} from './source.mjs';
const require=createRequire(path.join(path.resolve(process.env.SAFE_CEX_TYPESCRIPT_PATH),'../mexc-signature-resend.cjs'));
const axios=require('axios');const realHmac=require('create-hmac');
const retryModule=require('axios-retry');const retry=retryModule.default??retryModule;
const qs=require('qs');
const filename=root+'packages/safe-cex/src/exchanges/mexc/mexc.api.ts';
const source=fs.readFileSync(filename,'utf8');const ast=ts.createSourceFile(filename,source,ts.ScriptTarget.Latest,true);
const names=new Set(['createAPI','serializeMexcParams','signMexcRequest','isMexcPublicEndpoint','normalizeRequestData','getQueryString','isMexcSignatureRejection']);
const selected=ast.statements.filter(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>names.has(d.name.getText(ast))));
assert.equal(selected.length,7,'SOURCE independent seven real factory/helper declarations');
const code=ts.transpileModule(selected.map(n=>n.getText(ast)).join('\n'),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
const types=load('packages/safe-cex/src/exchanges/mexc/mexc.types.ts');
const {markSkipRemoteSigning}=load('packages/safe-cex/src/remoteSigningAdapter.ts');
const {networkErrorOnRead}=load('packages/safe-cex/src/utils/retryReadsOnly.ts');
const {resendOnSignatureRejection,signatureRejectedResponse}=load('packages/safe-cex/src/utils/signatureRejection.ts');
const {bigIntAsStringJsonParse}=load('packages/json-parse/src/jsonParse.ts');
const rejection='{"success":false,"code":602,"message":"Signature verification failed!"}';
const ok='{"code":0,"data":[]}';
// Each step is [HTTP status, raw body]; non-2xx steps reject like a settled axios adapter.
const scenarios=[
 {label:'get-envelope',method:'get',steps:[[200,rejection],[200,ok]]},
 {label:'post-envelope',method:'post',steps:[[200,rejection],[200,ok]]},
 {label:'post-envelope-twice',method:'post',steps:[[200,rejection],[200,rejection]]},
 {label:'post-http',method:'post',steps:[[401,rejection],[200,ok]]},
 {label:'get-http-twice',method:'get',steps:[[401,rejection],[401,rejection]]},
 {label:'get-http-then-envelope',method:'get',steps:[[401,rejection],[200,rejection]]},
 {label:'msg-only',method:'get',steps:[[200,'{"success":false,"code":10,"msg":"signature VERIFICATION failed"}'],[200,ok]]},
 {label:'not-signature',method:'post',steps:[[200,'{"success":false,"code":401,"message":"No authority"}'],[200,ok]]},
];
const predicateInputs=[
 {code:602},{code:'602'},{code:' 602 '},{code:[602]},{code:'0x25a'},{code:601},{code:null},
 {message:'SIGNATURE VERIFICATION FAILED!'},{message:null,msg:'signature verification failed'},
 {message:'',msg:'signature verification failed'},{msg:['Signature verification failed']},
 {message:{a:1}},{message:'signature verification failure'},{message:'ſignature verification failed'},
 'Signature verification failed',null,[],{},
];
let active;const outputs=[];const originalNow=Date.now;
Date.now=()=>1791800000123+(active?.attempt??0);
try{
 const context={exports:{},...types,AxiosHeaders:axios.AxiosHeaders,retry,qs,networkErrorOnRead,resendOnSignatureRejection,signatureRejectedResponse,bigIntAsStringJsonParse,markSkipRemoteSigning,
  virtualClock:{getCurrentTimeMs(){return 1791800000123+active.attempt;}},
  createHmac:realHmac,
  // Declared shared-factory child boundary; the MEXC body, Axios, retry and interceptors run unchanged.
  safeCexAxiosCreate(config){
   return axios.create({...config,adapter:async request=>{
    active.attempt++;
    const headers=request.headers.toJSON();
    active.wire.push({time:headers['Request-Time'],signature:headers.Signature,data:request.data??null});
    const [status,body]=active.spec.steps[active.attempt-1]??assert.fail('SOURCE unscripted extra attempt');
    const response={status,statusText:String(status),headers:{},data:body,config:request};
    if(status>=200&&status<300)return response;
    throw new axios.AxiosError('Request failed with status code '+status,'ERR_BAD_REQUEST',request,null,response);
   }});
  },
 };
 vm.runInNewContext(code,context,{filename});
 const predicate=predicateInputs.map(data=>context.exports.isMexcSignatureRejection(data));
 for(const spec of scenarios){
  active={spec,attempt:0,wire:[]};
  const client=context.exports.createAPI({key:'fixture-key',secret:'fixture-secret'},{name:'mexc'});
  let result;
  try{
   const response=spec.method==='get'?await client.get('/private',{params:{symbol:'BTC_USDT'}}):await client.post('/private',{symbol:'BTC_USDT',vol:1});
   result={ok:response.data.code};
  }catch(error){result={error:error.message,status:error.response?.status??null};}
  for(const [index,wire] of active.wire.entries()){
   const param=spec.method==='get'?'symbol=BTC_USDT':wire.data;
   assert.equal(wire.signature,realHmac('sha256','fixture-secret').update('fixture-key'+wire.time+param).digest('hex'),'SOURCE attempt '+index+' carries its own fresh signature');
  }
  outputs.push({label:spec.label,attempts:active.attempt,wire:active.wire,result});
 }
 console.log(JSON.stringify({predicateInputs,predicate,scenarios:outputs}));
}finally{Date.now=originalNow;}
