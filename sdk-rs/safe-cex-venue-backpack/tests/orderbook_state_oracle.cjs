const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
assert.ok(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH),root=path.resolve(__dirname,'../../..');
function unused(name){return new Proxy(function(){throw Error('unused '+name);},{get(_t,key){if(key==='__esModule')return false;throw Error('unused '+name+'.'+String(key));}});}
function run(file,require,extra={}){const exports={};const compiled=ts.transpileModule(fs.readFileSync(file,'utf8'),{fileName:file,reportDiagnostics:true,compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}});assert.equal(compiled.diagnostics.length,0);vm.runInNewContext(compiled.outputText,{exports,require,...extra},{filename:file});return exports;}
const types=run(path.join(root,'packages/safe-cex/src/types.ts'),id=>unused(id));
const constants=run(path.join(root,'packages/safe-cex/src/exchanges/backpack/backpack.types.ts'),id=>unused(id));
class Base {constructor(parent){this.parent=parent;this.isDisposed=false;}}
const performance={now(){throw Error('unconfigured time');}};
let onLevel;
const {BackpackPublicWebSocket:Socket}=run(path.join(root,'packages/safe-cex/src/exchanges/backpack/backpack.ws-public.ts'),id=>{
 if(id==='../../utils/updateOrderBook')return {updateOrderBook:(...args)=>onLevel(...args)};
 if(id==='lodash/throttle')return require(id);
 if(id==='../../types')return types;
 if(id==='./backpack.types')return constants;
 if(id==='../base.ws')return {BaseWebSocket:Base};
 return unused(id);
},{performance});
const {BackpackPrivateWebSocket:Private}=run(path.join(root,'packages/safe-cex/src/exchanges/backpack/backpack.ws-private.ts'),id=>{
 if(id==='../../types')return types;
 if(id==='./backpack.types')return constants;
 if(id==='../base.ws')return {BaseWebSocket:Base};
 return unused(id);
},{performance,setTimeout:(...args)=>schedule(...args)});
let schedule;
const units=s=>Array.from({length:s.length},(_,i)=>s.charCodeAt(i));
const bits=n=>{if(Number.isNaN(n))return '7ff8000000000000';const b=Buffer.alloc(8);b.writeDoubleBE(n);return b.toString('hex');};
const packet=wire=>{const p=JSON.parse(wire);return {method:p.method,params:p.params===undefined?null:p.params.map(v=>v===null?null:units(v))};};
const projection=s=>({updates:s.updates.slice(),loaded:s.isSnapshotLoaded,terminal:s.isTerminal,last:bits(s.lastUpdateId),inFlight:s.isSnapshotFetchInFlight,scheduled:s.isRecoveryScheduled,attempts:bits(s.recoveryAttempts),emitted:s.terminalErrorEmitted,handlers:Array.from(s.terminalHandlers)});
async function main(){
 const s=new Socket({}),a=s.createOrderBookState(),b=s.createOrderBookState();
 const created=[projection(a),projection(b)];
 a.updates.push(9);a.terminalHandlers.add(7);
 const fresh={updates:a.updates!==b.updates,handlers:a.terminalHandlers!==b.terminalHandlers,other:projection(b)};
 const sequence=[];
 const states=[-0,0,1,5,-1,2.5,2**53-1,2**53,NaN,Infinity,-Infinity];
 const ids=[undefined,-Infinity,-0,1,2,6,2**53,Infinity,NaN];
 for(const last of states)for(const first of ids)for(const end of ids){const state=s.createOrderBookState();state.lastUpdateId=last;const update={U:first,u:end};sequence.push({state:bits(last),first:first===undefined?null:bits(first),last:end===undefined?null:bits(end),bridge:s.isInitialOrderBookBridge(state,update),continuous:s.isContinuousOrderBookUpdate(state,update)});}
 const samples=[['1','2'],['-0','Infinity'],['2.5e1x','3x'],['bad','-Infinity'],['1\ud800','\ud8002'],['\ufeff4.5','5'],['1e309','-1e309'],['.25','-0']];
 const apply=[];
 for(let i=0;i<8;i++)for(const fail of [false,true]){
  const sock=new Socket({}),book={},failure={},calls=[];const mode=i%4;
  const bids=[samples[i].slice(),['9','10']],asks=[['11','12']];const data={b:bids,a:asks};
  onLevel=(actual,side,level)=>{assert.equal(actual,book);calls.push({side,price:bits(level.price),amount:bits(level.amount),total:bits(level.total),sameBook:true});if(calls.length===1){if(mode===1)bids.push(['13','14']);if(mode===2)bids.length=0;if(mode===3)data.a=[['15','16']];if(fail)throw failure;}};
  let sameError=false;try{sock.applyOrderBookUpdate(book,data);}catch(e){assert.equal(e,failure);sameError=true;}
  apply.push({input:samples[i].map(units),mode,fail,calls,sameError});
 }
 const reset=[];
 for(const fail of ['none','create','handlers','replace','lookup','fetch'])for(const mutate of [false,true]){
  const sock=new Socket({}),failure={},calls=[],old={},stored={},newStates=[];let active=true;
  sock.orderBookState={};sock.orderBookSnapshotFetchers={};
  for(const k of ['b','2','1']){
   const state=sock.createOrderBookState(),handlers=state.terminalHandlers;handlers.add(k);old[k]=state;stored[k]=state;
   state.updates=[4];state.isSnapshotLoaded=true;state.isTerminal=true;state.lastUpdateId=88;state.isSnapshotFetchInFlight=true;state.isRecoveryScheduled=true;state.recoveryAttempts=2;state.terminalErrorEmitted=true;
   Object.defineProperty(state,'terminalHandlers',{get(){calls.push('handlers:'+k);if(active&&fail==='handlers'&&k==='2')throw failure;return handlers;}});
   Object.defineProperty(sock.orderBookState,k,{enumerable:true,get(){return stored[k];},set(value){calls.push('set:'+k);if(fail==='replace'&&k==='2')throw failure;stored[k]=value;}});
   const fetcher=()=>{calls.push('fetch:'+k);if(mutate&&k==='1')delete sock.orderBookSnapshotFetchers['2'];if(fail==='fetch'&&k==='1')throw failure;};
   Object.defineProperty(sock.orderBookSnapshotFetchers,k,{enumerable:true,configurable:true,get(){calls.push('lookup:'+k);if(fail==='lookup'&&k==='2')throw failure;return fetcher;}});
  }
  const create=sock.createOrderBookState;sock.createOrderBookState=()=>{calls.push('create');if(fail==='create')throw failure;const value=create();newStates.push(value);return value;};
  let sameError=false;try{sock.resetAndRefetchOrderBooks();}catch(e){assert.equal(e,failure);sameError=true;}
  active=false;const trace=calls.slice();
  const final=['1','2','b'].map(k=>({key:k,value:projection(stored[k]),fresh:stored[k]!==old[k],sameHandlers:stored[k].terminalHandlers===old[k].terminalHandlers,sameCreated:newStates.includes(stored[k])}));
  reset.push({fail,mutate,calls:trace,final,sameError});
 }
 const disposal=[];
 for(const fail of ['none','clearSub','clearState','clearFetch','cancel:1','cancel:2','clearThrottle','close','noSocket','alternateSocket','secondRead','missingSocket']){
  const sock=new Socket({}),failure={},calls=[],oldStates={a:1},oldFetch={a:2},oldThrottles={};let state=oldStates,fetch=oldFetch,throttles=oldThrottles;
  sock.subscriptions=new Set(['x']);const clear=sock.subscriptions.clear.bind(sock.subscriptions);sock.subscriptions.clear=()=>{calls.push('clearSub');if(fail==='clearSub')throw failure;clear();};
  for(const [field,label,get,set] of [['orderBookState','clearState',()=>state,v=>state=v],['orderBookSnapshotFetchers','clearFetch',()=>fetch,v=>fetch=v],['depthTickerUpdateThrottles','clearThrottle',()=>throttles,v=>throttles=v]])Object.defineProperty(sock,field,{get,set(v){calls.push(label);if(fail===label)throw failure;set(v);}});
  for(const k of ['b','2','1'])oldThrottles[k]={cancel(){calls.push('cancel:'+k);if(k==='1'){delete throttles['2'];throttles.new={cancel(){throw Error('new throttle must not be iterated');}};}if(fail==='cancel:'+k)throw failure;}};
  const ws={close(){calls.push('close');if(fail==='close')throw failure;}},second={close(){calls.push('close:second');}};let reads=0;Object.defineProperty(sock,'ws',{get(){calls.push('socket');reads++;if(fail==='secondRead'&&reads===2)throw failure;if(fail==='noSocket'||(fail==='missingSocket'&&reads>=2))return undefined;if(fail==='alternateSocket'&&reads>=2)return second;return ws;}});
  let sameError=false,missingError=false;try{sock.dispose();}catch(e){if(fail==='missingSocket'){assert.equal(e.name,'TypeError');missingError=true;}else{assert.equal(e,failure);sameError=true;}}
  disposal.push({fail,calls:calls.slice(),sameError,missingError,subscriptions:Array.from(sock.subscriptions),stateReplaced:state!==oldStates,fetchReplaced:fetch!==oldFetch,throttlesReplaced:throttles!==oldThrottles,oldStateKeys:Object.keys(oldStates),oldFetchKeys:Object.keys(oldFetch),throttleKeys:Object.keys(throttles),sameWs:sock.ws===ws});
 }
 const resubscribe=[];
 for(const nonempty of [false,true])for(let kind=0;kind<4;kind++){
  const sock=new Private({}),failure={},calls=[];sock.subscriptions=new Set(nonempty?['account.orderUpdate']:[]);let resolve;
  sock.authenticate=()=>{calls.push('auth');if(kind===1)throw failure;if(kind===2)return Promise.reject(failure);if(kind===3)return new Promise(r=>resolve=r);return Promise.resolve();};
  const future=sock.resubscribe(),eager=calls.slice();let settled=false,sameError=false;
  const done=future.then(()=>{settled=true;},e=>{assert.equal(e,failure);sameError=true;settled=true;});sock.subscriptions.clear();
  await Promise.resolve();const pending=nonempty&&kind===3&&!settled;if(resolve)resolve();await done;
  resubscribe.push({nonempty,kind,eager,calls,pending,sameError});
 }
 const timers=[];
 const handles=[undefined,0,-0,NaN,Infinity,-Infinity,1,-1,{opaque:true}];
 for(let i=0;i<9;i++)for(const fail of [false,true]){
  const sock=new Private({}),failure={},calls=[];sock.ordersThrottleTimeout=handles[i];
  schedule=(callback,delay)=>{assert.equal(callback,sock.fetchThrottledOrders);calls.push({delay:bits(delay),before:sock.ordersThrottleTimeout===undefined?'absent':typeof sock.ordersThrottleTimeout==='object'?'object':bits(sock.ordersThrottleTimeout),sameCallback:true});if(fail)throw failure;return 55;};
  let sameError=false;try{sock.throttledFetchOrders();}catch(e){assert.equal(e,failure);sameError=true;}
  const value=sock.ordersThrottleTimeout;
  timers.push({input:i,fail,calls,sameError,after:value===undefined?'absent':typeof value==='object'?'object':bits(value),sameHandle:value===handles[i]||Object.is(value,handles[i])});
 }
 const ping=[];
 const pcases=[...[0,-0,NaN,Infinity,-Infinity,42].map(now=>({disposed:false,connected:true,fail:'none',now})),{disposed:true,connected:true,fail:'connected',now:7},{disposed:false,connected:false,fail:'none',now:7},...['connected','now','send'].map(fail=>({disposed:false,connected:true,fail,now:7})),{disposed:true,connected:false,fail:'none',now:7}];
 for(const c of pcases){
  const sock=new Private({}),failure={},calls=[],packets=[],wires=[];sock.isDisposed=c.disposed;sock.pingAt=99;
  Object.defineProperty(sock,'isConnected',{get(){calls.push('connected');if(c.fail==='connected')throw failure;return c.connected;}});performance.now=()=>{calls.push('now');if(c.fail==='now')throw failure;return c.now;};
  sock.send=wire=>{calls.push('send');packets.push(packet(wire));wires.push(units(wire));if(c.fail==='send')throw failure;};
  let sameError=false;try{sock.ping();}catch(e){assert.equal(e,failure);sameError=true;}
  ping.push({...c,now:bits(c.now),calls,packets,wires,pingAt:bits(sock.pingAt),sameError});
 }
 const unsubscribe=[];const cases=[[],['a'],['a','b','a'],['\ud800',null,'c'],['a'],['a'],['a']];
 for(let mode=0;mode<7;mode++)for(const fail of [false,true]){
  const sock=new Private({}),failure={},packets=[],wires=[],before=[];sock.subscriptions=new Set(['a','b','c','\ud800']);const streams=cases[mode].slice();if(mode===3)delete streams[1];
  sock.send=wire=>{before.push(Array.from(sock.subscriptions,units));packets.push(packet(wire));wires.push(units(wire));if(mode===4)streams.push('b');if(mode===5)streams.length=0;if(mode===6)streams[0]='c';if(fail)throw failure;};
  let sameError=false;try{sock.unsubscribe(streams);}catch(e){assert.equal(e,failure);sameError=true;}
  unsubscribe.push({mode,fail,input:cases[mode].map(v=>v===null?null:units(v)),packets,wires,before,after:Array.from(sock.subscriptions,units),sameError});
 }
 process.stdout.write(JSON.stringify({created,fresh,sequence,apply,reset,disposal,resubscribe,timers,ping,unsubscribe}));
}
main().catch(e=>{console.error(e);process.exitCode=1;});
