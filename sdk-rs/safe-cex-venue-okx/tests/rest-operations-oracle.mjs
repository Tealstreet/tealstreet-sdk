import fs from 'node:fs';
import vm from 'node:vm';
import path from 'node:path';
import {createRequire} from 'node:module';
import assert from 'node:assert/strict';
import {ts} from '../../safe-cex-store/tests/source_loader.mjs';
import {loadSource} from './source-loader.mjs';
const url=new URL('../../../packages/safe-cex/src/exchanges/okx/okx.exchange.ts',import.meta.url);
let source=fs.readFileSync(url,'utf8');
if(process.argv.includes('--mutate-lookup-response'))source=source.replace('error?.response?.data?.code === OKX_ORDER_DOES_NOT_EXIST','error?.message === OKX_ORDER_DOES_NOT_EXIST');
if(process.argv.includes('--mutate-placement-response'))source=source.replaceAll('err?.response?.data?.data?.[0]','undefined');
if(process.argv.includes('--mutate-eager-id-refusal'))for(const id of ['ordId','algoId'])source=source.replace(`if (o.${id}) {`,`if (o.${id}) { if(typeof o.${id} !== 'string') throw new Error('OKX order id must be a string');`);
const ast=ts.createSourceFile(url.pathname,source,ts.ScriptTarget.Latest,true);
const owner=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text==='OKXExchange');
const names=['findOrderByClientOrderId','placeOrderBatch','placeAlgoOrderBatch'];
const members=names.map(name=>{const m=owner.members.find(n=>n.name?.getText(ast)===name);assert.ok(m,name);return m.getText(ast);});
const constants=['OKX_ORDER_DOES_NOT_EXIST','OKX_LOOKUP_STATUS','OKX_LOOKUP_LIVE_STATE'];
const declarations=ast.statements.filter(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>constants.includes(d.name.getText(ast))));
assert.equal(declarations.length,3);
const domain=loadSource(new URL('../../../packages/safe-cex/src/types.ts',import.meta.url));
const {ENDPOINTS}=loadSource(new URL('../../../packages/safe-cex/src/exchanges/okx/okx.types.ts',import.meta.url));
const dependencies=process.env.SAFE_CEX_TYPESCRIPT_PATH?createRequire(path.join(path.dirname(process.env.SAFE_CEX_TYPESCRIPT_PATH),'oracle.cjs')):createRequire(import.meta.url);
const {mapSeries}=dependencies(process.argv.includes('--mutate-missing-package')?'missing-okx-p-iteration-control':'p-iteration');
const chunk=dependencies('lodash/chunk');const flatten=dependencies('lodash/flatten');
const {orderCarriesClientOrderId}=loadSource(new URL('../../../packages/safe-cex/src/exchanges/clientOrderId.ts',import.meta.url),['orderCarriesClientOrderId'],['CLIENT_ID_INFO_FIELDS','sameClientOrderId','orderCarriesClientOrderId']);
const module={exports:{}};
const code=ts.transpileModule(declarations.map(n=>n.getText(ast)).join('\n')+'\nexport class Selected {'+members.join('\n')+'}',{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
vm.runInThisContext('(function(module,exports,OrderStatus,ENDPOINTS,mapSeries,chunk,flatten,orderCarriesClientOrderId){'+code+'\n})')(module,module.exports,domain.OrderStatus,ENDPOINTS,mapSeries,chunk,flatten,orderCarriesClientOrderId);
const apiUrl=new URL('../../../packages/safe-cex/src/exchanges/okx/okx.api.ts',import.meta.url);
const apiAst=ts.createSourceFile(apiUrl.pathname,fs.readFileSync(apiUrl,'utf8'),ts.ScriptTarget.Latest,true);
const create=apiAst.statements.find(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>d.name.getText(apiAst)==='createAPI'));
assert.ok(create);
let checkResponse;
const xhr={interceptors:{response:{use:fn=>checkResponse=fn},request:{use:()=>{}}}};
const apiModule={exports:{}};
const helpers=apiAst.statements.filter(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>['isOkxSignatureRejection','formatOkxApiErrorMessage'].includes(d.name.getText(apiAst))));
assert.equal(helpers.length,2);
const apiCode=ts.transpileModule(helpers.map(n=>n.getText(apiAst).replace(/^export\s+/,'')).join('\n')+'\n'+create.getText(apiAst),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
// No placement fixture is a signature rejection, so the resend declines every response.
vm.runInThisContext('(function(module,exports,BASE_URLS,RECV_WINDOW,safeCexAxiosCreate,retry,networkErrorOnRead,signatureRejectedResponse,resendOnSignatureRejection){'+apiCode+'\n})')(apiModule,apiModule.exports,{okx:{livenet:'https://www.okx.com'}},20000,()=>xhr,()=>{},()=>{},()=>()=>false,()=>undefined);
apiModule.exports.createAPI({publicDataOnly:true},{});
assert.equal(typeof checkResponse,'function');
const baseOrder={id:'store-order',status:'open',symbol:'BTCUSDT',type:'limit',side:'buy',price:1,amount:1,filled:0,remaining:1,reduceOnly:false,lastUpdate:123,info:{clOrdId:'client'}};
const cases=[];
async function run(input){
 const selected=new module.exports.Selected(), calls=[], emissions=[], mappings=[];
 selected.store={orders:input.orders??[],marketsBySymbol:input.market?{BTCUSDT:input.market}:{}};
 selected.mapOrders=rows=>{mappings.push(rows);return input.mapEmpty?[]:rows.map(row=>({...baseOrder,id:'mapped-order',info:row}));};
 selected.emitAndLogError=message=>emissions.push(message);
 let responseIndex=0;
 const perform=async (method,url,payload)=>{
  calls.push({method,url,...(method==='post'?{body:JSON.stringify(payload)}:{params:payload.params})});
  const item=input.responses[responseIndex++];assert.ok(item,'unexpected request');
  const response={status:item.status??200,statusText:'venue reason',headers:{'x-venue':'real-response'},data:item.bodyRaw?JSON.parse(item.bodyRaw):item.body};
  if(item.reject){const error=new Error(item.message??'DISPLAY TEXT IS NOT THE RESPONSE');error.response=response;throw error;}
  return checkResponse(response);
 };
 selected.xhr={get:(url,params)=>perform('get',url,params),post:(url,payload)=>perform('post',url,payload)};
 try{const result=input.op==='lookup'?await selected.findOrderByClientOrderId('BTCUSDT','client'):await selected[input.op==='batch'?'placeOrderBatch':'placeAlgoOrderBatch'](input.payloads);
 const key=input.op==='lookup'?input.responses[0]?.body.data?.[0]?.state:undefined;
 let sourcePrototype;
 if(['constructor','__proto__','toString','valueOf','hasOwnProperty'].includes(key)){
  const expected=key==='__proto__'?Object.prototype:Object.prototype[key];
  assert.equal(result.order.status,expected,'source native inherited status identity BEFORE serialization');
  assert.equal(mappings[0][0].state,expected,'source native inherited mapper input BEFORE serialization');
  sourcePrototype={key,statusType:typeof result.order.status,mapperType:typeof mappings[0][0].state};
 }
 cases.push({input,result,calls,emissions,mappings,sourcePrototype});}
 catch(error){cases.push({input,error:{name:error.name,message:error.message,response:error.response?.data},calls,emissions,mappings});}
}
const market={id:'BTC-USDT-SWAP',symbol:'BTCUSDT',base:'BTC',quote:'USDT',active:true,precision:{amount:1,price:1},limits:{amount:{min:1,max:100},leverage:{min:1,max:100}}};
await run({op:'lookup',orders:[baseOrder],market,responses:[]});
await run({op:'lookup',responses:[]});
for(const reject of [true,false])for(const code of ['51603',51603,'50000','0',0,null,undefined])await run({op:'lookup',market,responses:[{reject,body:{code,msg:'VENUE BODY'}}]});
await run({op:'lookup',market,responses:[{reject:true,message:'51603 appears ONLY in display text',body:{code:'50000'}}]});
for(const state of ['live','partially_filled','filled','canceled','mmp_canceled','unknown','constructor','__proto__','toString','valueOf','hasOwnProperty'])await run({op:'lookup',market,responses:[{body:{code:'0',data:[{state,ordId:'native'}]}}]});
await run({op:'lookup',market,responses:[{body:{code:{toString:null}}}]});
await run({op:'lookup',market,responses:[{body:{code:'1',msg:{toString:null}}}]});
await run({op:'lookup',market,responses:[{bodyRaw:'{"code":1e400}',body:{code:null}}]});
await run({op:'lookup',market,mapEmpty:true,responses:[{body:{code:'0',data:[{state:'filled'}]}}]});
for(const op of ['batch','algo']){
 for(const size of [0,1,20,21,41]){
  const payloads=Array.from({length:size},(_,i)=>({clOrdId:'c'+i,px:1e-7,sz:1e21}));
  const responses=Array.from({length:op==='batch'?Math.ceil(size/20):size},(_,i)=>({body:{code:'0',data:Array.from({length:op==='batch'?Math.min(20,size-i*20):1},(_,j)=>({[op==='batch'?'ordId':'algoId']:'o'+(op==='batch'?i*20+j:i)}))}}));
  await run({op,payloads,responses});
 }
 for(const sMsg of ['venue rejected','',null,0,'\uFEFF venue rejection'])for(const sCode of ['51000','',0,51000])await run({op,payloads:[{clOrdId:'x'}],responses:[{reject:true,message:'DISPLAY TEXT IS NOT THE RESPONSE',body:{code:'1',msg:'TOP LEVEL',data:[{sMsg,sCode}]}}]});
 await run({op,payloads:[{}],responses:[{body:{code:'1',msg:'TOP LEVEL',data:[{sMsg:'PER ORDER',sCode:'51008'}]}}]});
 await run({op,payloads:[{}],responses:[{body:{code:'0',data:[{sMsg:'resolved failure'}]}}]});
 await run({op,payloads:[{}],responses:[{body:{code:'0',data:[{}]}}]});
 await run({op,payloads:[{}],responses:[{body:{code:'0',data:[null]}}]});
 for(const item of [{body:{code:'0',data:[{sMsg:{toString:null}}]}},{reject:true,body:{code:'1',data:[{sMsg:{toString:null},sCode:'1'}]}},{reject:true,body:{code:'1',data:[{sMsg:'rejected',sCode:{toString:null}}]}},{body:{code:'1',msg:{toString:null}}}])await run({op,payloads:[{}],responses:[item]});
 for(const id of [17,{},['native']])await run({op,payloads:[{}],responses:[{body:{code:'0',data:[{[op==='batch'?'ordId':'algoId']:id}]}}],malformedId:true});
 await run({op,payloads:[{}],responses:[{body:{code:'0'}}],malformedData:true});
 await run({op,payloads:Array.from({length:41},()=>({})),responses:[{reject:true,body:{msg:'first failure'}}]});
}
for(const op of ['batch','algo'])for(const acrossRequests of [false,true])for(const later of [null,{sMsg:'LATER SOURCE REJECTION'}]){
 const id=op==='batch'?'ordId':'algoId';
 const responses=acrossRequests?[{body:{code:'0',data:[{[id]:17}]}},{body:{code:'0',data:[later]}}]:[{body:{code:'0',data:[{[id]:17},later]}}];
 await run({op,payloads:Array.from({length:acrossRequests?(op==='batch'?21:2):1},()=>({})),responses,refusalTiming:true});
 const witness=cases.at(-1);
 assert.equal(witness.calls.length,acrossRequests?2:1,'deferred refusal source request count');
 assert.equal(witness.error.message,later===null?`Cannot read properties of null (reading '${id}')`:'LATER SOURCE REJECTION','deferred refusal source later error');
}
for(const op of ['batch','algo']){
 const id=op==='batch'?'ordId':'algoId';
 await run({op,payloads:Array.from({length:op==='batch'?21:2},()=>({})),responses:[{body:{code:'0',data:[{[id]:17}]}},{body:{code:'0',data:[{[id]:'later-valid'}]}}],malformedId:true,refusalTiming:true});
 assert.equal(cases.at(-1).calls.length,2,'deferred refusal source request count');
}
assert.equal(cases.length,118);
const missing=cases.find(c=>c.input.op==='lookup'&&c.input.responses[0]?.reject&&c.input.responses[0].body.code==='51603');assert.deepEqual(missing.result,{found:false,definitive:true});
const placement=cases.find(c=>c.input.op==='batch'&&c.input.responses[0]?.body.data?.[0]?.sMsg==='PER ORDER');assert.equal(placement.error.message,'PER ORDER (code 51008)');
let inheritedStatusExecutions=0, placementIdConversions=0;
for(const c of cases){
 if(c.input.malformedId){
  // Keep the actual source result separately; decision 4 converts live IDs
  // at the native string result boundary, rather than losing the placed order.
  c.sourceMalformed=JSON.parse(JSON.stringify(c));
  c.result=c.result.map(String);
  placementIdConversions++;
 }
 if(c.input.malformedData){
  c.sourceMalformed=JSON.parse(JSON.stringify(c));delete c.result;
  const message="OKX order response data must be an array";c.error={name:"Error",message};c.emissions=[message];
 }
 if(c.sourcePrototype)inheritedStatusExecutions++;
}
assert.equal(inheritedStatusExecutions,5,'REST-BRANCH inherited status executions');
assert.equal(placementIdConversions,8,'REST-BRANCH live placement ID conversions');
process.stdout.write(JSON.stringify(cases));
