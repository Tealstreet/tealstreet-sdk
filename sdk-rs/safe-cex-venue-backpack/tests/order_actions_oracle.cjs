const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
assert.ok(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root=path.resolve(__dirname,'../../..');
function unused(name){return new Proxy(function(){throw Error('unused '+name);},{get(_t,key){if(key==='__esModule')return false;throw Error('unused '+name+'.'+String(key));}});}
function run(file,require){const exports={};const compiled=ts.transpileModule(fs.readFileSync(file,'utf8'),{fileName:file,reportDiagnostics:true,compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}});assert.equal(compiled.diagnostics.length,0);vm.runInNewContext(compiled.outputText,{exports,require},{filename:file});return exports;}
const severity=run(path.join(root,'packages/logging/src/LogSeverity.ts'),id=>unused(id));
const types=run(path.join(root,'packages/safe-cex/src/types.ts'),id=>id==='@tealstreet/logging/src/LogSeverity'?severity:unused(id));
const constants=run(path.join(root,'packages/safe-cex/src/exchanges/backpack/backpack.types.ts'),id=>unused(id));
let xhrSetups=0,sockets=0;
class Base {createXhrWithSecondaryKey(){xhrSetups++;return unused('xhr');}}
class Socket {constructor(){sockets++;}}
const api=run(path.join(root,'packages/safe-cex/src/exchanges/backpack/backpack.exchange.ts'),id=>{
 if(id==='p-iteration')return require('p-iteration');
 if(id==='../../types')return types;
 if(id==='./backpack.types')return constants;
 if(id==='../base')return {BaseExchange:Base};
 if(id==='./backpack.api')return {createAPI:unused('createAPI')};
 if(id==='./backpack.ws-public')return {BackpackPublicWebSocket:Socket};
 if(id==='./backpack.ws-private')return {BackpackPrivateWebSocket:Socket};
 return unused(id);
});
const exchange=new api.BackpackExchange('backpack',{},{});
assert.equal(xhrSetups,1);assert.equal(sockets,2);

const bits=n=>{assert.equal(typeof n,'number');if(Number.isNaN(n))return '7ff8000000000000';const b=Buffer.alloc(8);b.writeDoubleBE(n);return b.toString('hex');};
const units=s=>Array.from({length:s.length},(_,i)=>s.charCodeAt(i));
async function main(){
 const eligibility=[];
 const eligibleSpecs=[['ordinary false',false,undefined,'none'],['ordinary throws',true,undefined,'ordinary'],['absent ID',true,undefined,'none'],['null ID',true,null,'none'],['zero ID',true,0,'none'],['empty ID',true,'','none'],['string ID',true,'mine','none'],['ID getter throws',true,undefined,'id'],['builder throws',true,undefined,'build']];
 for(const [label,ordinary,clientOrderId,failure] of eligibleSpecs){
  const trace=[],error={opaque:'eligible'},opts={tag:'opts'};
  Object.defineProperty(opts,'clientOrderId',{get(){trace.push(['id']);if(failure==='id')throw error;return clientOrderId;}});
  exchange.isOrdinaryLimitOrder=arg=>{assert.equal(arg,opts);trace.push(['ordinary']);if(failure==='ordinary')throw error;return ordinary;};
  exchange.buildBatchLimitOrderRequest=(arg,clientId)=>{assert.equal(arg,opts);trace.push(['build',bits(clientId)]);if(failure==='build')throw error;return {ignored:'result'};};
  let outcome;try{outcome={ok:true,value:exchange.canUseNativeBatch(opts)};}catch(e){assert.equal(e,error);outcome={ok:false,sameFailure:true};}
  eligibility.push({label,ordinary,clientOrderId:clientOrderId===undefined?['undefined']:clientOrderId===null?['null']:typeof clientOrderId==='number'?['number',bits(clientOrderId)]:['string',clientOrderId],failure,trace,outcome});
 }
 const pairs=[];
 const stages=['ok','ids','build0','build1','post sync','post async','decode','short ids','mutate requests'];
 for(const stage of stages){
  const orders=[{tag:'first'},{tag:'second'}],trace=[],failure={opaque:'pair'},response={opaque:'response'},result=['id λ','id \ud800'];let posted;
  exchange.createBatchClientIds=count=>{trace.push(['ids',count]);if(stage==='ids')throw failure;return stage==='short ids'?[17]:[17,29];};
  exchange.buildBatchLimitOrderRequest=(order,clientId)=>{const index=orders.indexOf(order);assert.ok(index===0||index===1);trace.push(['build',index,clientId===undefined?null:bits(clientId)]);if(stage==='build'+index)throw failure;return {symbol:'REQ'+index,clientId,tag:order.tag};};
  exchange.xhr={post(endpoint,requests){trace.push(['post',endpoint,requests.map(r=>[r.symbol,r.clientId===undefined?null:bits(r.clientId),r.tag])]);posted=requests;if(stage==='post sync')throw failure;if(stage==='mutate requests')requests.push({symbol:'ADDED',clientId:31,tag:'third'});return stage==='post async'?Promise.reject(failure):Promise.resolve({data:response});}};
  exchange.decodeBatchOrderResponse=(data,requests)=>{assert.equal(data,response);assert.equal(requests,posted);trace.push(['decode',requests.map(r=>[r.symbol,r.clientId===undefined?null:bits(r.clientId),r.tag])]);if(stage==='decode')throw failure;return result;};
  const pending=exchange.placeBatchPair(orders),atConstruction=structuredClone(trace);let outcome;
  try{const actual=await pending;assert.equal(actual,result);outcome={ok:true,sameResult:true,result:actual.map(units)};}catch(e){assert.equal(e,failure);outcome={ok:false,sameFailure:true};}
  pairs.push({stage,atConstruction,trace,outcome});
 }
 const cancels=[];
 for(const stage of ['ok','missing market','market throws','delete sync','delete async'])for(const logThrows of [false,true]){
  const trace=[],failure=Object.assign(new Error('action failed'),{name:'ActionError'}),logger=Object.assign(new Error('logger failed'),{name:'LoggerError'}),order={symbol:'BTC λ',id:'order 29'};
  exchange.store={marketsBySymbol:new Proxy({},{get(_t,key){trace.push(['market',key]);if(stage==='market throws')throw failure;return stage==='missing market'?undefined:{id:'VENUE 17'};}})};
  exchange.xhr={delete(endpoint,config){trace.push(['delete',endpoint,config]);if(stage==='delete sync')throw failure;return stage==='delete async'?Promise.reject(failure):Promise.resolve();}};
  exchange.log=(message,severity)=>{trace.push(['log',message,severity]);if(logThrows)throw logger;};
  const pending=exchange.cancelOrder(order),atConstruction=structuredClone(trace);let outcome;
  try{await pending;outcome={ok:true};}catch(e){outcome={ok:false,name:e.name,message:e.message,sameFailure:e===failure,sameLogger:e===logger};}
  cancels.push({stage,logThrows,order,atConstruction,trace,outcome});
 }
 let release;const trace=[],orders=[{tag:'first'},{tag:'second'}];let posted;
 exchange.createBatchClientIds=count=>{trace.push(['ids',count]);return [17,29];};exchange.buildBatchLimitOrderRequest=(o,id)=>{trace.push(['build',o.tag,bits(id)]);return {tag:o.tag,id};};
 exchange.xhr={post(endpoint,requests){posted=requests;trace.push(['post',endpoint]);return new Promise(resolve=>{release=resolve;});}};
 exchange.decodeBatchOrderResponse=(data,requests)=>{assert.equal(requests,posted);assert.equal(data,'resolved');trace.push(['decode']);return ['done'];};
 const pending=exchange.placeBatchPair(orders),before=structuredClone(trace);await Promise.resolve();const whilePending=structuredClone(trace);release({data:'resolved'});const result=await pending;
 const waits={before,whilePending,after:trace,result};
 process.stdout.write(JSON.stringify({eligibility,pairs,cancels,waits}));
}
main().catch(e=>{console.error(e);process.exitCode=1;});
