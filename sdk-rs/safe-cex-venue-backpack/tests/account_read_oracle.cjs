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
 const limits=[undefined,null,0,-0,NaN,Infinity,-Infinity,1e21,1.25,-3,true,false,'',' 2.75x','-0','Infinity','1e309','bad',[],[3],[' 4x'],[null,2],{}, {toString:0},1n];
 const inputs=[...limits.map(leverageLimit=>({leverageLimit})),undefined,null,true,false,7,'abc',[],{}];
 const cases=[];
 async function add(data,readError,logThrows,initial){
  const calls=[],logs=[];const failure=Object.assign(new Error('logger failed'),{name:'SubscriberError'});
  exchange.accountLeverageLimit=initial;
  exchange.xhr={get(...args){calls.push(args);return readError?Promise.reject(Object.assign(new Error(readError.message),{name:readError.name})):Promise.resolve({data});}};
  exchange.log=(message,severity)=>{logs.push([message,severity]);if(logThrows)throw failure;};
  const pending=exchange.fetchAccountInfo();const callsAtConstruction=calls.length;const before=bits(exchange.accountLeverageLimit);
  let outcome;try{const value=await pending;assert.equal(value,undefined);outcome={ok:true};}catch(e){outcome={ok:false,name:e.name,message:e.message,sameLoggerError:e===failure};}
  cases.push({input:observe(data),readError,logThrows,initial:bits(initial),callsAtConstruction,before,calls,logs,after:bits(exchange.accountLeverageLimit),outcome});
 }
 for(const data of inputs)for(const logThrows of [false,true])await add(data,null,logThrows,17);
 for(const readError of [{name:'Error',message:'network λ'},{name:'',message:'bare'},{name:'Error',message:''},{name:'TypeError',message:'bad payload'}])for(const initial of [-0,17,NaN,Infinity])for(const logThrows of [false,true])await add(undefined,readError,logThrows,initial);
 let release;const deferredCalls=[];
 exchange.accountLeverageLimit=17;
 exchange.log=()=>{throw Error('unexpected deferred log');};
 exchange.xhr={get(...args){deferredCalls.push(args);return new Promise(resolve=>{release=resolve;});}};
 const pending=exchange.fetchAccountInfo();
 const before=bits(exchange.accountLeverageLimit),callsAtConstruction=deferredCalls.length;
 exchange.accountLeverageLimit=41;await Promise.resolve();
 const whilePending=bits(exchange.accountLeverageLimit);
 release({data:{leverageLimit:'2.5'}});await pending;
 const deferred={before,callsAtConstruction,whilePending,after:bits(exchange.accountLeverageLimit),calls:deferredCalls};
 const strings=[];
 for(const leverageLimit of [' 2.75 λ','2\ud800']){
  const requests=[];exchange.accountLeverageLimit=17;
  exchange.xhr={get(...args){requests.push(args);return Promise.resolve({data:{leverageLimit}});}};
  exchange.log=()=>{throw Error('unexpected string boundary log');};
  await exchange.fetchAccountInfo();
  strings.push({raw:JSON.stringify(leverageLimit),after:bits(exchange.accountLeverageLimit),requests});
 }
 const accounts=[];
 async function account(data, readError, logThrows) {
  const calls=[],logs=[];const sourceError=readError?Object.assign(new Error(readError.message),{name:readError.name}):null;
  const loggerError=Object.assign(new Error('logger failed'),{name:'SubscriberError'});
  exchange.xhr={get(...args){calls.push(args);return sourceError?Promise.reject(sourceError):Promise.resolve({data});}};
  exchange.log=(message,severity)=>{logs.push([message,severity]);if(logThrows)throw loggerError;};
  const pending=exchange.getAccount(),callsAtConstruction=calls.length,logsAtConstruction=logs.length;
  let outcome;try{const result=await pending;outcome={ok:true,userId:observe(result.userId),affiliateId:observe(result.affiliateId)};}
  catch(e){outcome={ok:false,name:e.name,message:e.message,sameReadError:e===sourceError,sameLoggerError:e===loggerError};}
  accounts.push({input:observe(data),readError,logThrows,callsAtConstruction,logsAtConstruction,calls,logs,outcome});
 }
 for(const accountId of [undefined,null,false,true,0,-0,NaN,Infinity,-Infinity,'','user λ',[],{},0n,4n])
  for(const logThrows of [false,true])await account({accountId,userId:'fallback λ'},null,logThrows);
 for(const data of [undefined,null,true,7,'abc',[],{}])
  for(const logThrows of [false,true])await account(data,null,logThrows);
 for(const readError of [{name:'Error',message:'network λ'},{name:'',message:'bare'},{name:'Error',message:''},{name:'TypeError',message:'bad payload'}])
  for(const logThrows of [false,true])await account(undefined,readError,logThrows);
 const positionMode=await exchange.getPositionMode();assert.equal(typeof positionMode,'string');
 const modes=[];
 for(const method of ['changePositionMode','changeMarginType'])for(const enabled of [false,true])for(const throws of ['none','emit','log']){
  const trace=[],failure=Object.assign(new Error(throws+' failed'),{name:'SubscriberError'});
  exchange.emitter={emit(event,error){assert.equal(error.name,'Error');trace.push(['emit',event,error.name,error.message]);if(throws==='emit')throw failure;}};
  exchange.log=(message,severity)=>{trace.push(['log',message,severity]);if(throws==='log')throw failure;};
  const pending=exchange[method](enabled,'ignored λ'),atConstruction=trace.length;
  let outcome;try{assert.equal(await pending,undefined);outcome={ok:true};}catch(e){outcome={ok:false,name:e.name,message:e.message,sameFailure:e===failure};}
  modes.push({method,enabled,throws,atConstruction,trace,outcome});
 }
 process.stdout.write(JSON.stringify({runtime:{node:process.version,arch:process.arch,platform:process.platform},cases,deferred,strings,accounts,positionMode,modes}));
}
main().catch(e=>{console.error(e);process.exitCode=1;});
