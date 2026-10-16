const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
assert.ok(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH),root=path.resolve(__dirname,'../../..');
function unused(name){return new Proxy(function(){throw Error('unused '+name);},{get(_t,key){if(key==='__esModule')return false;throw Error('unused '+name+'.'+String(key));}});}
function run(file,require,extra={}){const exports={};const compiled=ts.transpileModule(fs.readFileSync(file,'utf8'),{fileName:file,reportDiagnostics:true,compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}});assert.equal(compiled.diagnostics.length,0);vm.runInNewContext(compiled.outputText,{exports,require,...extra},{filename:file});return exports;}
const types=run(path.join(root,'packages/safe-cex/src/types.ts'),id=>unused(id));
const constants=run(path.join(root,'packages/safe-cex/src/exchanges/backpack/backpack.types.ts'),id=>unused(id));
class Base {constructor(parent){this.parent=parent;this.isDisposed=false;}}
let cacheContext,authContext,disposalClear;
const throttle=(updater,wait,opts)=>{if(cacheContext){const c=cacheContext;c.calls.push('factory');assert.equal(updater,c.updater);c.factoryArgs.push({wait,leading:opts.leading,trailing:opts.trailing,sameUpdater:true});if(c.input==='factoryMap')c.sock.depthTickerUpdateThrottles=c.nextMap;if(c.fail==='factory')throw c.failure;return c.newValue;}return ()=>{};};
const signing={};Object.defineProperty(signing,'generateWsAuthSignature',{get(){const c=authContext;c.calls.push('signer');if(c.mode==='signer')throw c.primary;return (key,secret,window)=>{c.calls.push('factory');c.requests.push({key:units(key),secret:units(secret),window,sameSigner:true});Set.prototype.add.call(c.socket.subscriptions,'during');if(c.mode==='factory')throw c.primary;if(c.mode==='await')return Promise.reject(c.primary);if(c.mode==='pending')return new Promise(r=>c.resolve=r);return Promise.resolve(c.signature);};}});
const Public=run(path.join(root,'packages/safe-cex/src/exchanges/backpack/backpack.ws-public.ts'),id=>{if(id==='lodash/throttle')return throttle;if(id==='../../types')return types;if(id==='./backpack.types')return constants;if(id==='../base.ws')return {BaseWebSocket:Base};return unused(id);}).BackpackPublicWebSocket;
const Private=run(path.join(root,'packages/safe-cex/src/exchanges/backpack/backpack.ws-private.ts'),id=>{if(id==='../../types')return types;if(id==='./backpack.types')return constants;if(id==='./backpack.signing')return signing;if(id==='../base.ws')return {BaseWebSocket:Base};return unused(id);},{clearTimeout:id=>disposalClear(id)}).BackpackPrivateWebSocket;
class ExchangeBase {createXhrWithSecondaryKey(){return unused('xhr');}}
class SocketStub {}
const Exchange=run(path.join(root,'packages/safe-cex/src/exchanges/backpack/backpack.exchange.ts'),id=>{if(id==='../../types')return types;if(id==='./backpack.types')return constants;if(id==='../base')return {BaseExchange:ExchangeBase};if(id==='./backpack.api')return {createAPI:unused('api')};if(id==='./backpack.ws-public')return {BackpackPublicWebSocket:SocketStub};if(id==='./backpack.ws-private')return {BackpackPrivateWebSocket:SocketStub};return unused(id);}).BackpackExchange;
const units=s=>Array.from({length:s.length},(_,i)=>s.charCodeAt(i));
const set=s=>Array.from(s,units);
const bits=n=>{if(Number.isNaN(n))return '7ff8000000000000';const b=Buffer.alloc(8);b.writeDoubleBE(n);return b.toString('hex');};
function packet(w){const p=JSON.parse(w);return {method:p.method,params:p.params.map(units),signature:p.signature.map(units)};}
const symbols=[undefined,'','BTC_USDC','λ🚀','X\ud800'];
const prefixes=['Failed to authenticate WebSocket: ','Failed to subscribe to order updates: ','Failed to subscribe to position updates: '];
async function main(){
 const auth=[];
 for(let method=0;method<3;method++)for(let symbolIndex=0;symbolIndex<(method===1?5:1);symbolIndex++)for(const existing of [false,true])for(const mode of ['success','pending','signer','key','secret','factory','await','lookup','send','format','emitLookup','emit','has','add']){
  const calls=[],requests=[],packets=[],wires=[],before=[],messages=[],primary={},coerce={},emitLookup={},emit={};let socket;
  primary[Symbol.toPrimitive]=hint=>{assert.equal(hint,'string');calls.push('format');if(mode==='format')throw coerce;return 'boom\ud800';};
  const options={};Object.defineProperty(options,'key',{get(){calls.push('key');if(mode==='key')throw primary;return 'KEYλ\ud800';}});Object.defineProperty(options,'secret',{get(){calls.push('secret');if(mode==='secret')throw primary;return 'SECRET\udc00';}});
  const parent={options};Object.defineProperty(parent,'emitAndLogError',{get(){calls.push('emitLookup');if(mode==='emitLookup')throw emitLookup;return message=>{calls.push('emit');messages.push(units(message));if(mode==='emit')throw emit;};}});
  socket=new Private(parent);const symbol=symbols[symbolIndex];const stream=method===1?(symbol?`account.orderUpdate.${symbol}`:'account.orderUpdate'):'account.positionUpdate';
  const streams=method===0?['account.orderUpdate','account.positionUpdate']:[stream];function watched(values){const set=new Set(values);set.has=s=>{calls.push('has');if(mode==='has')throw primary;return Set.prototype.has.call(set,s);};set.add=s=>{calls.push('add:'+streams.indexOf(s));Set.prototype.add.call(set,s);if(mode==='add')throw primary;return set;};return set;}socket.subscriptions=watched(['keep',...(existing?streams:[])]);
  const signature=['SIGKEY','\ud800','\udc00"\\','W\u0000'];const c={socket,calls,requests,mode,primary,signature};authContext=c;
  Object.defineProperty(socket,'send',{get(){calls.push('lookup');if(mode==='lookup')throw primary;signature[2]='getter\ud800';if(mode==='pending')socket.subscriptions=watched(['replaced']);return wire=>{calls.push('send');before.push(set(socket.subscriptions));packets.push(packet(wire));wires.push(units(wire));socket.subscriptions.delete('during');Set.prototype.add.call(socket.subscriptions,'sent');if(['send','format','emitLookup','emit'].includes(mode))throw primary;};}});
  const future=method===0?socket.authenticate():method===1?socket.subscribeToOrderUpdates(symbol):socket.subscribeToPositionUpdates();const eager=calls.slice();let outcome='ok';
  const done=future.catch(e=>{if(e===coerce)outcome='coerce';else if(e===emitLookup)outcome='emitLookup';else if(e===emit)outcome='emit';else if(e===primary)outcome='primary';else throw e;});
  if(c.resolve){signature[1]='changed\udc00';c.resolve(signature);}await done;
  auth.push({method,symbolIndex,symbol:symbol===undefined?null:units(symbol),existing,mode,eager,calls,requests,packets,wires,before,messages,outcome,after:set(socket.subscriptions),pending:!!c.resolve});
 }
 const connection=[];
 for(const kind of ['public','private'])for(const disposed of [false,true])for(const testnet of [false,true])for(const fail of ['none','disposed','options','url','create','assign','method','handler','open','message','close']){
  const calls=[],registrations=[],created=[],failure={},options={};let current,second;const handlers={open:()=>{},message:()=>{},close:()=>{}};
  Object.defineProperty(options,'testnet',{get(){calls.push('options');if(fail==='options')throw failure;return testnet;}});
  for(const network of ['PRODUCTION','TESTNET'])Object.defineProperty(constants.WSS_URLS,network,{configurable:true,get(){calls.push('url:'+network);if(fail==='url')throw failure;return network==='PRODUCTION'?'wss://production.fixture/λ':'wss://testnet.fixture/\ud800';}});
  const sock=new (kind==='public'?Public:Private)({options});
  Object.defineProperty(sock,'isDisposed',{get(){calls.push('disposed');if(fail==='disposed')throw failure;return disposed;}});
  function ws(id){return Object.defineProperty({id},'addEventListener',{get(){calls.push('method:'+id);if(fail==='method')throw failure;return function(event,handler){calls.push('listen:'+event+':'+id);assert.equal(this.id,id);assert.equal(handler,handlers[event]);registrations.push({event,socket:id,sameHandler:true,sameReceiver:true});if(fail===event)throw failure;};}});}
  current=ws('old');second=ws('second');const initial=current;
  Object.defineProperty(sock,'ws',{get(){calls.push('socket');return current;},set(value){calls.push('assign:'+value.id);if(fail==='assign')throw failure;current=value;}});
  for(const event of ['open','message','close'])Object.defineProperty(sock,event==='open'?'onOpen':event==='message'?'onMessage':'onClose',{get(){calls.push('handler:'+event);if(fail==='handler')throw failure;if(event==='open')sock.ws=second;return handlers[event];}});
  sock.createWebsocket=url=>{calls.push('create');created.push(units(url));if(fail==='create')throw failure;return ws('created');};
  let sameError=false,eager;try{const result=sock.connectAndSubscribe();eager=calls.slice();if(kind==='private')await result;}catch(e){assert.equal(e,failure);sameError=true;eager=calls.slice();}
  connection.push({kind,disposed,testnet,fail,eager,calls,created,registrations,sameError,current:current.id,sameInitial:current===initial});
 }
 const open=[];
 for(const disposed of [false,true])for(const fail of ['none','disposed','track','factory','await','resubscribe','ping','pending']){
  const calls=[],failure={},sock=new Private({});let resolve;
  Object.defineProperty(sock,'isDisposed',{get(){calls.push('disposed');if(fail==='disposed')throw failure;return disposed;}});
  sock.trackLastReceived=()=>{calls.push('track');if(fail==='track')throw failure;};
  sock.authenticate=()=>{calls.push('auth');if(fail==='factory')throw failure;if(fail==='await')return Promise.reject(failure);if(fail==='pending')return new Promise(r=>resolve=r);return Promise.resolve();};
  sock.resubscribe=()=>{calls.push('resubscribe');if(fail==='resubscribe')throw failure;return new Promise(()=>{});};sock.ping=()=>{calls.push('ping');if(fail==='ping')throw failure;};
  const future=sock.onOpen(),eager=calls.slice();let sameError=false;const done=future.catch(e=>{assert.equal(e,failure);sameError=true;});if(resolve)resolve();await done;
  open.push({disposed,fail,eager,calls,pending:!!resolve,sameError});
 }
 const disposal=[];
 for(const mode of ['none','absentTimer','zeroTimer','clearSub','timerSecond','clearTimer','unsetTimer','socketSecond','close','differentSocket','missingSocket']){
  const calls=[],failure={},sock=new Private({}),timerA={id:'A'},timerB={id:'B'};let timer=mode==='absentTimer'?undefined:mode==='zeroTimer'?0:timerA,reads=0,wsReads=0;
  sock.subscriptions=new Set(['a']);sock.subscriptions.clear=()=>{calls.push('clearSub');if(mode==='clearSub')throw failure;Set.prototype.clear.call(sock.subscriptions);};
  Object.defineProperty(sock,'ordersThrottleTimeout',{get(){calls.push('timer');reads++;if(mode==='timerSecond'&&reads===2)throw failure;return reads>=2&&timer===timerA?timerB:timer;},set(v){calls.push('unset');if(mode==='unsetTimer')throw failure;timer=v;}});
  const first={close(){calls.push('close:A');if(mode==='close')throw failure;}},second={close(){calls.push('close:B');}};
  Object.defineProperty(sock,'ws',{get(){calls.push('socket');wsReads++;if(mode==='socketSecond'&&wsReads===2)throw failure;if(mode==='missingSocket'&&wsReads>=2)return undefined;return mode==='differentSocket'&&wsReads>=2?second:first;}});
  const clearTimer=id=>{calls.push('clearTimer');assert.equal(id,timerB);if(mode==='clearTimer')throw failure;};
  disposalClear=clearTimer;
  let sameError=false,missingError=false;
  try{sock.dispose();}catch(e){if(mode==='missingSocket'){assert.equal(e.name,'TypeError');missingError=true;}else{assert.equal(e,failure);sameError=true;}}
  disposal.push({mode,calls,sameError,missingError,subscriptions:Array.from(sock.subscriptions),timer:timer===undefined?'undefined':timer===0?'zero':'A'});
 }
 const cache=[];
 for(const input of ['missing','zero','nan','old','constructor','__proto__','lone','replacement','updaterMap','factoryMap'])for(const fail of ['none','get','updater','factory','set']){
  const sock=new Public({}),failure={},calls=[],factoryArgs=[],oldValue=()=>{},newValue=()=>{},alternate=()=>{};oldValue.tag='old';newValue.tag='new';alternate.tag='alternate';
  const symbol=input==='constructor'?'constructor':input==='__proto__'?'__proto__':input==='lone'?'X\ud800':'BTC';let value=input==='zero'?0:input==='nan'?NaN:input==='old'?oldValue:undefined;
  const map={};if(!['constructor','__proto__'].includes(input))Object.defineProperty(map,symbol,{get(){calls.push('get');if(fail==='get')throw failure;return value;},set(v){calls.push('set');if(fail==='set')throw failure;value=input==='replacement'?alternate:v;}});
  const nextMap={[symbol]:alternate};sock.depthTickerUpdateThrottles=map;const updater=sock.updateTickerFromDepth;Object.defineProperty(sock,'updateTickerFromDepth',{get(){calls.push('updater');if(input==='updaterMap')sock.depthTickerUpdateThrottles=nextMap;if(fail==='updater')throw failure;return updater;}});
  cacheContext={calls,factoryArgs,updater,newValue,fail,failure,input,sock,nextMap};
  let result,sameError=false;try{result=sock.getDepthTickerUpdateThrottle(symbol);}catch(e){assert.equal(e,failure);sameError=true;}cacheContext=undefined;
  const observed=v=>v===undefined?'undefined':typeof v==='number'?'number:'+bits(v):v===Object.prototype?'prototype':v===Object?'constructor':v.tag;
  const original=['constructor','__proto__'].includes(input)?map[symbol]:value,current=sock.depthTickerUpdateThrottles===nextMap?alternate:original;
  cache.push({input,fail,symbol:units(symbol),calls,factoryArgs,sameError,result:sameError?null:observed(result),current:observed(current),original:observed(original),sameValue:!sameError&&result===current});
 }
 const delegation=[];
 const syms=['BTC','λ🚀','X\ud800'];
 for(const method of ['ohlcv','book'])for(let i=0;i<3;i++)for(const fail of [false,true]){
  const exchange=new Exchange('backpack',{},{}),failure={},callback=()=>{},unsubscribe=()=>{},calls=[];const opts={symbol:syms[i],interval:['1m','5m','\udc00'][i]};
  exchange.publicWebsocket={listenOHLCV(o,c){assert.equal(o,opts);assert.equal(c,callback);calls.push({symbol:units(o.symbol),interval:units(o.interval),sameOpts:true,sameCallback:true});if(fail)throw failure;return unsubscribe;},listenOrderBook(symbol,c){assert.equal(symbol,syms[i]);assert.equal(c,callback);calls.push({symbol:units(symbol),sameCallback:true});if(fail)throw failure;return unsubscribe;}};
  let sameError=false,sameReturn=false;try{const result=method==='ohlcv'?exchange.listenOHLCV(opts,callback):exchange.listenOrderBook(syms[i],callback);sameReturn=result===unsubscribe;}catch(e){assert.equal(e,failure);sameError=true;}
  delegation.push({method,index:i,fail,symbol:units(syms[i]),interval:units(opts.interval),calls,sameReturn,sameError});
 }
 process.stdout.write(JSON.stringify({auth,connection,open,disposal,cache,delegation}));
}
main().catch(e=>{console.error(e);process.exitCode=1;});
