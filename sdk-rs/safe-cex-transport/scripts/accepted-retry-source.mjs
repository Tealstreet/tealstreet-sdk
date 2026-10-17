// Execute maintained signature helpers plus installed Axios/axios-retry.
import path from 'node:path';
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url);
const root=path.resolve(path.dirname((await import('node:url')).fileURLToPath(import.meta.url)), '../../..');
const {load,dependencies}=require(path.join(root,'sdk-rs/safe-cex-types/scripts/accounts-source.cjs'));
const {loadContracts}=await import(path.join(root,'sdk-rs/safe-cex-parity/scripts/accepted-contract-files.mjs'));
const contracts=loadContracts();
const signature=load(path.join(root,'packages/safe-cex/src/utils/signatureRejection.ts'));
const reads=load(path.join(root,'packages/safe-cex/src/utils/retryReadsOnly.ts'));
const axios=dependencies('axios').default;
const retry=dependencies('axios-retry').default;
const key=signature.SIGNATURE_RETRY_CONFIG_KEY;
let helperCases=0;
for(const name of ['signature_rejected_response','resend_on_signature_rejection','resend_on_rejected_data']){
 const rows=contracts['safe_cex_transport::signature_rejection::'+name].cases;
 assert.equal(rows.length,name==='signature_rejected_response'?5:4);
 for(const {input,output} of rows){
  let config={url:'/effective',timeout:27,method:'POST'};
  if(input.spent)assert.equal(signature.signatureRejectedResponse(()=>true)({config,response:{data:null}}),true);
  const before=signature.hasRetriedSignature(config);
  let calls=0;const predicate=data=>(calls++,signature.isBinanceStyleSignatureRejection(data));
  let outcome;
  try{
   let decision;
   if(name==='signature_rejected_response')decision=signature.signatureRejectedResponse(predicate)({config,response:input.response===false?undefined:{data:input.data}});
   else decision=signature.resendOnSignatureRejection({request:next=>(config=next,Promise.resolve('resent'))},{config,data:input.data},predicate)!==undefined;
   outcome={ok:true,decision};
  }catch(error){outcome={ok:false,name:error.name,message:error.message};}
  const expectedOutcome=output.outcome.ok?output.outcome:{ok:false,name:output.outcome.error?.name??output.outcome.name,message:output.outcome.error?.message??output.outcome.message};
  assert.deepEqual({before,after:signature.hasRetriedSignature(config),calls,outcome},{before:output.before,after:output.after,calls:output.calls,outcome:expectedOutcome},name);
  helperCases++;
 }
}
const engine=contracts['safe_cex_transport::signature_rejection::retry_network_reads_and_signature_rejection'].cases;
assert.equal(engine.length,9);
const originalNow=Date.now,originalTimeout=globalThis.setTimeout;
try{
 for(const {input,output} of engine){
  let now=100,calls=0,stamped=false;
  const attempts=[],sleeps=[];
  Date.now=()=>now;
  globalThis.setTimeout=(fn,ms,...args)=>(sleeps.push(ms??0),originalTimeout(fn,0,...args));
  const xhr=axios.create({adapter:async config=>{
   const n=attempts.length;
   attempts.push({method:config.method.toUpperCase(),url:config.url,timeout:config.timeout});now+=5;
   if(input.rewrite)config={...config,url:'/rewritten'};
   stamped ||= signature.hasRetriedSignature(config);
   if(n>0&&input.kind!=='repeat')return {status:200,statusText:'OK',data:{accepted:true},headers:{},config};
   const error=new Error(input.kind==='network'?'reset':'HTTP rejection');error.config=config;
   if(input.kind==='network')error.code='ECONNRESET';
   else error.response={status:401,data:input.kind==='throw'?{code:{toString:0}}:{code:-1022},config};
   throw error;
  }});
  const predicate=data=>(calls++,signature.isBinanceStyleSignatureRejection(data));
  const condition=signature.signatureRejectedResponse(predicate);
  retry(xhr,{retries:reads.isReadOnlyMethod(input)?input.retries:1,shouldResetTimeout:false,retryCondition:error=>{
   const approved=reads.networkErrorOnRead()(error)||condition(error);
   stamped ||= signature.hasRetriedSignature(error.config);return approved;
  }});
  let outcome;
  try{const actual=await xhr.request({method:input.method,url:'/order',timeout:input.timeout});outcome={ok:true,status:actual.status,url:actual.config.url,timeout:actual.config.timeout};}
  catch(error){outcome={ok:false,name:error.name,message:error.message,code:error.code??null,response:error.response?.status??null};}
  const expected=output.outcome.ok?output.outcome:{ok:false,name:output.outcome.error.name??'Error',message:output.outcome.error.message,code:output.outcome.error.code,response:output.outcome.error.response};
  assert.deepEqual({attempts,sleeps,calls,stamp:stamped,now,outcome},{attempts:output.attempts,sleeps:output.sleeps,calls:output.calls,stamp:output.stamp,now:output.now,outcome:expected},'actual source/Axios retry '+JSON.stringify(input));
 }
}finally{Date.now=originalNow;globalThis.setTimeout=originalTimeout;}
console.log(`${helperCases} maintained helper cases and ${engine.length} actual Axios/axios-retry traces PASS; native error/request identity is checked separately.`);
