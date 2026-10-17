import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {loadContracts} from '../../safe-cex-parity/scripts/accepted-contract-files.mjs';
const require=createRequire(import.meta.url);
const axios=require('axios');
const contracts=loadContracts();
function observe(v){
 if(v===null)return {kind:'null'};
 if(typeof v==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(v);return {kind:'number',bits:b.readBigUInt64BE().toString()};}
 if(typeof v==='string'||typeof v==='boolean')return {kind:typeof v==='boolean'?'bool':'string',value:v};
 if(Array.isArray(v))return {kind:'array',items:v.map(observe)};
 return {kind:'object',entries:Object.entries(v).map(([k,v])=>[k,observe(v)])};
}
function entries(owner,method){return contracts['safe_cex_transport::provider::'+owner].entrypoints.find(e=>e.api.name===method).cases;}
let requests=0,responses=0,dispatches=0;
for(const {input,output} of entries('RestBody','transform')){
 if(input.phase!=='untransformed')continue;
 const headers=new axios.AxiosHeaders(input.content?{'Content-Type':input.content}:{});
 const value=axios.defaults.transformRequest[0].call({},JSON.parse(input.raw),headers);
 assert.deepEqual(observe(value),output.actual.value,'actual Axios request transform');requests++;
}
for(const {input,output} of entries('RestBody','wire_bytes_for_content_type')){
 if(input.phase!=='untransformed')continue;
 const headers=new axios.AxiosHeaders(input.content?{'Content-Type':input.content}:{});
 const value=axios.defaults.transformRequest[0].call({},JSON.parse(input.raw),headers);
 assert.equal(typeof value,'string');assert.deepEqual([...Buffer.from(value)],output.bytes,'actual Axios stringifySafely/form bytes');requests++;
}
for(const {input,output} of entries('RestResponseBody','transform_axios_response')){
 // Physical UTF-8/BOM decoding is tested separately in native response tests.
 // This oracle executes Axios's actual current-data transform; native tests
 // separately retain the documented UTF-8 representation refusal.
 if(input.phase==='encoded')continue;
 const actual=axios.defaults.transformResponse[0].call({},JSON.parse(input.raw));
 assert.deepEqual(observe(actual),output.actual.value,'actual Axios response transform');responses++;
}
for(const {input,output} of entries('RestRequest','transform_axios_request')){
 if(input.body?.phase==='transformed')continue; // remote adapter phase, not Axios pre-transform input
 let captured;
 await axios.request({method:input.method,url:'/source',headers:Object.fromEntries(input.headers),data:input.body==null?undefined:input.body.phase==='encoded'?Buffer.from(input.body.bytes):JSON.parse(input.body.raw),adapter:async config=>{captured=config;return {status:200,statusText:'OK',headers:{},data:'',config};}});
 const actual=captured.data==null?null:[...Buffer.from(captured.data)];
 assert.deepEqual(actual,output.wire.body?.bytes??null,'actual Axios dispatch body');
 const content=output.wire.headers.find(([k])=>k.toLowerCase()==='content-type')?.[1];
 assert.equal(captured.headers.getContentType()||undefined,content,'actual Axios dispatch content type');dispatches++;
}
assert.equal(requests,11);assert.equal(responses,5);assert.equal(dispatches,6);
console.log(`Installed Axios ${axios.VERSION}: ${requests} request transforms/bytes, ${responses} current-data response transforms, ${dispatches} actual dispatch adapter observations passed. Native phase/error/UTF-8 policy witnesses remain separate.`);
