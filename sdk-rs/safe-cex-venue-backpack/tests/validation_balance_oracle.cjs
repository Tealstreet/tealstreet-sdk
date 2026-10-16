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
function observe(v){if(v===undefined)return ['undefined'];if(v===null)return ['null'];if(typeof v==='number')return ['number',bits(v)];if(typeof v==='bigint')return ['bigint',String(v)];if(typeof v==='string'||typeof v==='boolean')return [typeof v,v];if(Array.isArray(v))return ['array',v.map(observe)];return ['object',Object.keys(v).map(k=>[k,observe(v[k])])];}

async function main(){
 const balance=[];
 const values=[undefined,null,'', '  ', '-0', 0, -0, NaN, Infinity,-Infinity,'1e309','2.75x',true,false,[],[3],{}, {toString:0},5n];
 const inputCases=[{netEquity:'101.125',netEquityAvailable:'37.25',netEquityLocked:'63.875',pnlUnrealized:'-4.5'},...values.map(v=>({netEquity:v,netEquityAvailable:v,netEquityLocked:v,pnlUnrealized:v})),undefined,null,7,{}];
 for(const [index,data] of inputCases.entries())for(const logThrows of [false,true]){
  const trace=[],failure=Object.assign(new Error('logger failed'),{name:'LoggerError'});
  exchange.xhr={get(endpoint){trace.push(['get',endpoint]);return Promise.resolve({data});}};
  exchange.log=(message,severity)=>{trace.push(['log',message,severity]);if(logThrows)throw failure;};
  const pending=exchange.fetchBalance(),atConstruction=structuredClone(trace);
  let outcome;try{const b=await pending;outcome={ok:true,fields:Object.keys(b),values:[b.total,b.free,b.used,b.upnl].map(bits)};}catch(e){outcome={ok:false,name:e.name,message:e.message,sameLogger:e===failure};}
  balance.push({index,input:observe(data),logThrows,atConstruction,trace,outcome});
 }
 const balanceFailures=[];
 for(const sync of [false,true])for(const logThrows of [false,true]){
  const trace=[],failure=Object.assign(new Error('network failed'),{name:'TransportError'}),logger=Object.assign(new Error('logger failed'),{name:'LoggerError'});
  exchange.xhr={get(endpoint){trace.push(['get',endpoint]);if(sync)throw failure;return Promise.reject(failure);}};
  exchange.log=(message,severity)=>{trace.push(['log',message,severity]);if(logThrows)throw logger;};
  const pending=exchange.fetchBalance(),atConstruction=structuredClone(trace);let outcome;
  try{await pending;assert.fail('transport rejection');}catch(e){outcome={name:e.name,message:e.message,sameFailure:e===failure,sameLogger:e===logger};}
  balanceFailures.push({sync,logThrows,atConstruction,trace,outcome});
 }
 exchange.xhr={get(){return Promise.resolve({data:{netEquity:'2\ud800'}});}};exchange.log=()=>assert.fail('boundary log');
 const b=await exchange.fetchBalance();const balanceBoundary={raw:JSON.stringify('2\ud800'),total:bits(b.total)};
 const validation=[];
 const failures=[undefined,null,{}, {message:''},{message:0},{message:NaN},{message:'message λ'},{message:12},{message:Infinity},{message:5n},{message:[]},{message:{tag:'object'}}, {response:{data:{msg:'response λ'}},message:'fallback'},{response:{data:{msg:0}},message:'fallback'},{response:{data:{msg:false}},message:0},{response:{data:{msg:NaN}},message:'fallback'},{response:{data:{msg:[]}},message:'fallback'},{response:{data:{msg:{tag:'response'}}},message:'fallback'}];
 async function addValidation(stage,error,emitThrows){
  const trace=[],token={balance:'identity'},replacement=Object.assign(new Error('emit failed'),{name:'EmitError'});
  exchange.xhr={get(endpoint){trace.push(['get',endpoint]);if(stage==='capital sync')throw error;return stage==='capital async'?Promise.reject(error):Promise.resolve();}};
  exchange.fetchBalance=()=>{trace.push(['balance']);if(stage==='balance sync')throw error;return stage==='balance async'?Promise.reject(error):Promise.resolve(token);};
  exchange.emitAndLogError=value=>{trace.push(['emit',observe(value)]);if(emitThrows)throw replacement;};
  const pending=exchange.validateAccount(),atConstruction=structuredClone(trace);let outcome;
  try{const result=await pending;outcome={ok:true,error:observe(result.error),hasBalance:Object.hasOwn(result,'balance'),balanceUndefined:result.balance===undefined,sameBalance:result.balance===token};}
  catch(e){outcome={ok:false,name:e.name,message:e.message,sameReplacement:e===replacement};}
  validation.push({stage,input:observe(error),emitThrows,atConstruction,trace,outcome});
 }
 for(const stage of ['ok','balance async'])await addValidation(stage,{message:'balance failure'},false);
 for(const stage of ['capital sync','capital async','balance sync'])for(const error of failures)for(const emitThrows of [false,true])await addValidation(stage,error,emitThrows);
 const error={message:'before'};exchange.xhr={get(){return Promise.reject(error);}};exchange.emitAndLogError=value=>{assert.equal(value,'before');error.message='after';};
 const validationMutation=await exchange.validateAccount();
 const series=[];
 for(const method of ['cancelOrders','cancelAllOrders'])for(const mode of ['empty','dense','cancel sync','cancel async','sparse','append','shrink','get throw','element reject','length throw','logger']){
  const trace=[],failure=Object.assign(new Error('series failed'),{name:'SeriesError'}),logger=Object.assign(new Error('logger failed'),{name:'LoggerError'});
  const items=mode==='empty'?[]:mode==='sparse'?[{id:'A'},,{id:'C'}]:[{id:'A'},{id:'B'},{id:'C'}];
  if(mode==='get throw'||mode==='logger')Object.defineProperty(items,1,{get(){throw failure;}});
  if(mode==='element reject')items[1]=Promise.resolve().then(()=>{throw failure;});
  // The promise has a handler immediately, preventing premature unhandled rejection.
  if(mode==='element reject')items[1].catch(()=>{});
  const array=mode==='length throw'?new Proxy(items,{get(t,key){if(key==='length')throw failure;return t[key];}}):items;
  const invoke=(item)=>{trace.push(['invoke',item.id]);assert.ok(items.includes(item));if(mode==='append'&&item.id==='A')items.push({id:'D'});if(mode==='shrink'&&item.id==='A')items.length=1;if(mode==='cancel sync'&&item.id==='B')throw failure;return mode==='cancel async'&&item.id==='B'?Promise.reject(failure):Promise.resolve();};
  exchange.cancelOrder=invoke;
  exchange.xhr={delete(endpoint,config){assert.equal(endpoint,'/api/v1/orders');assert.deepEqual(Object.keys(config),['data']);assert.deepEqual(Object.keys(config.data),['symbol']);return invoke(items.find(item=>item&&item.id===config.data.symbol));}};
  exchange.store={markets:array};exchange.log=(message,severity)=>{trace.push(['log',message,severity]);if(mode==='logger')throw logger;};
  const pending=method==='cancelOrders'?exchange.cancelOrders(array):exchange.cancelAllOrders(),atConstruction=structuredClone(trace);let outcome;
  try{assert.equal(await pending,undefined);outcome={ok:true};}catch(e){outcome={ok:false,name:e.name,message:e.message,sameFailure:e===failure,sameLogger:e===logger};}
  series.push({method,mode,atConstruction,trace,outcome});
 }
 const waits=[];
 for(const method of ['cancelOrders','cancelAllOrders']){
  let releaseItem,releaseCancel;const trace=[];const input=[new Promise(resolve=>{releaseItem=resolve;}),{id:'B'}];
  const effect=id=>{trace.push(['start',id]);if(id==='A')return new Promise(resolve=>{releaseCancel=()=>{trace.push(['finish',id]);resolve();};});trace.push(['finish',id]);return Promise.resolve();};
  exchange.cancelOrder=item=>effect(item.id);exchange.store={markets:input};exchange.xhr={delete(endpoint,body){assert.equal(endpoint,'/api/v1/orders');return effect(body.data.symbol);}};exchange.log=()=>assert.fail('unexpected wait log');
  const pending=method==='cancelOrders'?exchange.cancelOrders(input):exchange.cancelAllOrders();const before=structuredClone(trace);
  releaseItem({id:'A'});await Promise.resolve();const afterItem=structuredClone(trace);
  await Promise.resolve();const whileCancelPending=structuredClone(trace);
  releaseCancel();await pending;
  waits.push({method,before,afterItem,whileCancelPending,after:trace});
 }
 process.stdout.write(JSON.stringify({runtime:{node:process.version,arch:process.arch,platform:process.platform},balance,balanceFailures,balanceBoundary,validation,validationMutation,series,waits}));
}
main().catch(e=>{console.error(e);process.exitCode=1;});
