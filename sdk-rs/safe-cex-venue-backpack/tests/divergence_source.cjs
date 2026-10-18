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


const Private = run(path.join(root,'packages/safe-cex/src/exchanges/backpack/backpack.ws-private.ts'),id=>{
 if(id==='../../types') return types;
 if(id==='../base.ws') return {BaseWebSocket:class {constructor(parent){this.parent=parent;} get isDisposed(){return false;} get store(){return this.parent.store;}}};
 return unused(id);
}).BackpackPrivateWebSocket;
(async()=>{
 const reads=[];
 for(const name of ['getAccount','fetchBalance']) {
  let reject; const calls=[],logs=[];
  exchange.xhr={get(url){calls.push(url);return new Promise((_resolve,no)=>{reject=no;});}};
  exchange.log=(message,severity)=>logs.push([message,severity]);
  // Discard the caller's waiter. The rejection handler merely observes settlement.
  exchange[name]().catch(()=>{});
  assert.equal(calls.length,1); assert.equal(logs.length,0);
  reject(new Error('held response failure'));
  for(let i=0;i<8;i++) await Promise.resolve();
  assert.equal(logs.length,1); assert.equal(logs[0][1],'error');
  reads.push({name,calls,logs});
 }
 const catches=[];
 for(const name of ['fetchThrottledOrders','executeBalanceFetch']) for(const stage of ['factory','reject','update']) {
  const failure=new Error('reconcile-'+stage),calls=[],updates=[];
  const parent={store:{loaded:{orders:false},update(patch){calls.push('update');if(stage==='update')throw failure;updates.push(patch);}}};
  parent[name==='fetchThrottledOrders'?'fetchOrders':'fetchBalance']=()=>{
   calls.push('fetch');if(stage==='factory')throw failure;
   return stage==='reject'?Promise.reject(failure):Promise.resolve(name==='fetchThrottledOrders'?[]:{total:31});
  };
  const socket=new Private(parent); socket.ordersThrottleTimeout=27;
  assert.equal(await socket[name](),undefined);
  assert.equal(calls[0],'fetch');assert.equal(updates.length,0);
  catches.push({name,stage,calls,fulfilled:true});
 }
 process.stdout.write(JSON.stringify({reads,catches}));
})().catch(error=>{console.error(error);process.exitCode=1;});
