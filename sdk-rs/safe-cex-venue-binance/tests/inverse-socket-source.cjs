// Execute the maintained classes and imported algorithms unchanged. Base ports
// supply only wire/timer/log observation; native tests use the real Base/Store.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const {createRequire} = require('node:module');
assert(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'explicit installed TypeScript required');
const compiler = path.resolve(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const dependencies = createRequire(path.join(compiler, '../inverse-socket-source.cjs'));
const ts = dependencies(compiler);
const root = path.resolve(__dirname, '../../..');
const source = rel => path.join(root, 'packages/safe-cex/src', rel);
const cache = new Map();
const clock = {now: 1_700_000_000_000};
let timers = new Map(), nextTimer = 0;
const setTimer = (fn, delay) => {const id=++nextTimer;timers.set(id,{fn,delay});return id;};
const clearTimer = id => timers.delete(id);
const enumsFile = source('types.ts');
const enumAst = ts.createSourceFile(enumsFile, fs.readFileSync(enumsFile,'utf8'),ts.ScriptTarget.Latest,true);
const enums = {};
const enumCode = enumAst.statements.filter(ts.isEnumDeclaration).map(n=>n.getText(enumAst)).join('\n');
function evaluate(code, filename, exports={}) {
  const js = ts.transpileModule(code,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022,esModuleInterop:true},fileName:filename}).outputText;
  const requireSource = spec => {
    if (spec.endsWith('/base.ws')) return {BaseWebSocket:BasePorts};
    if (spec.endsWith('/types') && path.resolve(path.dirname(filename),spec+'.ts')===enumsFile) return enums;
    if (spec.endsWith('/virtualClock')) return {virtualClock:{getCurrentTimeMs:()=>clock.now}};
    if (spec==='nanoid') return {nanoid:()=>`source-book-${++nextTimer}`};
    if (spec==='@tealstreet/json-parse/src/jsonParse') return load(path.join(root,'packages/json-parse/src/jsonParse.ts'));
    if (spec==='./binance.ws-public') return {dispatchBinanceMessage:dispatch};
    if (spec.startsWith('.')) return load(path.resolve(path.dirname(filename),spec+'.ts'));
    return dependencies(spec);
  };
  const module={exports};
  new Function('require','exports','module','setTimeout','clearTimeout','performance','Date',js)(requireSource,exports,module,setTimer,clearTimer,{now:()=>clock.now},class extends Date{static now(){return clock.now;}});
  return module.exports;
}
evaluate(enumCode,enumsFile,enums);
function load(filename) {
  if(cache.has(filename))return cache.get(filename);
  const exports={};cache.set(filename,exports);
  return Object.assign(exports,evaluate(fs.readFileSync(filename,'utf8'),filename,exports));
}
class BasePorts {
  constructor(parent){this.parent=parent;this.sent=[];this.errors=[];this.stats=[];this.cleaned=[];this.received=0;this.pongs=0;this.orderBookInnerState={};this.isConnected=true;this.isForceClosed=false;this.onClose=()=>{};}
  get store(){return this.parent.store;}
  get isDisposed(){return this.parent.isDisposed;}
  send(text){this.sent.push(JSON.parse(text));}
  sendWithLog(text){this.send(text);}
  trackLastReceived(){this.received++;}
  emitWsDiagnosticReceive(data){this.diagnostic=data;}
  recordMessageHandlerStats(topic){this.stats.push(topic);}
  cleanupMessageHandlerStats(topic){this.cleaned.push(topic);}
  handlePongEvent(){this.pongs++;}
  emitAndLogError(text){this.errors.push(text);}
  resetOrderbook(){this.resetCalls=(this.resetCalls||0)+1;}
  createWebsocket(url){this.url=url;const listeners=[];return{readyState:1,addEventListener:(event,fn)=>listeners.push([event,fn]),removeEventListener:(event,fn)=>{const i=listeners.findIndex(x=>x[0]===event&&x[1]===fn);if(i>=0)listeners.splice(i,1);},close(){this.readyState=3;},listeners};}
  dispose(){this.baseDisposed=(this.baseDisposed||0)+1;}
}
const dispatchFile=source('exchanges/binance/binance.ws-public.ts');
const dispatchAst=ts.createSourceFile(dispatchFile,fs.readFileSync(dispatchFile,'utf8'),ts.ScriptTarget.Latest,true);
const declaration=dispatchAst.statements.find(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>d.name.getText(dispatchAst)==='dispatchBinanceMessage'));
assert(declaration,'actual shared dispatcher');
const dispatch=evaluate(`const {jsonParse}=require('@tealstreet/json-parse/src/jsonParse');\n${declaration.getText(dispatchAst)}`,dispatchFile,{}).dispatchBinanceMessage;
const {BinanceInversePublicWebsocket:Public}=load(source('exchanges/binance/binanceinverse.ws-public.ts'));
const {BinanceInversePrivateWebsocket:Private}=load(source('exchanges/binance/binanceinverse.ws-private.ts'));
const SYMBOL='BTCUSD_PERP';
function parent(publicDataOnly=false,testnet=false){
 const positions=[],orders=[],fills=[],logs=[];
 const ticker={id:SYMBOL,symbol:SYMBOL,bid:49999,ask:50001,last:50000,mark:50000,index:50000,percentage:0,fundingRate:0,fundingTime:0,volume:0,quoteVolume:0,openInterest:0};
 const store={positions,orders,tickersBySymbol:{[SYMBOL]:ticker},marketsBySymbol:{[SYMBOL]:{contractSize:100}},loaded:{orders:false},
  updateTicker(t,patch){Object.assign(t,patch);},addOrUpdateOrder(row){const old=orders.find(o=>o.id===row.id);if(old)Object.assign(old,row);else orders.push(row);},
  updateOrder(target,patch){const old=orders.find(o=>o.id===target.id);if(old)Object.assign(old,patch);},updateOrderStatus(target,status){this.updateOrder(target,{status});},
  addOrUpdatePosition(row){const old=positions.find(p=>p.id===row.id);if(old)Object.assign(old,row);else positions.push(row);}};
 return{store,options:{publicDataOnly,testnet},isDisposed:false,emitter:{emit(event,value){assert.equal(event,'fill');fills.push(value);}},fills,logs,
  log(...args){logs.push(args);},isBackgroundReconciliationPaused:()=>true,markPositionStateUpdatedByWebsocket(){this.version=(this.version||0)+1;},getPositionStateVersion(){return this.version||0;}};
}
let cases=0;
(async()=>{
 for(const [publicOnly,testnet,url] of [[true,false,'wss://dstream.binance.com/ws'],[false,true,'wss://dstream.binancefuture.com/ws']]){
  const p=parent(publicOnly,testnet),s=new Public(p);s.connectAndSubscribe();assert.equal(s.url,url);s.onOpen.call(undefined);
  assert.deepEqual(s.sent[0],{id:1,method:'SUBSCRIBE',params:publicOnly?['!ticker@arr','!markPrice@arr@1s']:['!ticker@arr','!markPrice@arr@1s','!bookTicker']});
  s.onMessage({data:JSON.stringify({e:'24hrTicker',s:SYMBOL,c:'50020.5',P:'-1.25',v:'9',q:'',Q:'450184.5'})});
  s.onMessage({data:JSON.stringify({e:'markPriceUpdate',s:SYMBOL,p:'50010',i:'50011',r:'0.0001',T:1700000000123})});
  s.onMessage({data:JSON.stringify({e:'bookTicker',s:SYMBOL,b:'50009',a:'50012'})});
  const t=p.store.tickersBySymbol[SYMBOL];assert.deepEqual([t.last,t.percentage,t.volume,t.quoteVolume,t.mark,t.index,t.bid,t.ask],[50020.5,-1.25,9,450184.5,50010,50011,50009,50012]);assert.equal(s.received,3);cases++;
 }
 {
  const s=new Public(parent(true)),seen=[];const opts={symbol:SYMBOL,interval:'1m'};
  const a=s.listenOHLCV(opts,c=>seen.push(c)),b=s.listenOHLCV(opts,()=>{});
  s.onMessage({data:JSON.stringify({e:'kline',s:SYMBOL,k:{s:SYMBOL,i:'1m',t:1700000000000,o:'1',h:'2',l:'0.5',c:'1.5',v:'7'}})});
  assert.deepEqual(seen,[{timestamp:1700000000,open:1,high:2,low:0.5,close:1.5,volume:7}]);a();a();assert.equal(s.sent.filter(x=>x.method==='UNSUBSCRIBE').length,0);b();assert.equal(s.sent.filter(x=>x.method==='UNSUBSCRIBE').length,1);cases++;
 }
 {
  const p=parent(true),s=new Public(p),seen=[],requests=[];
  const replies=[{lastUpdateId:100,bids:[['100','2']],asks:[['101','3']]},{lastUpdateId:102,bids:[['100','5']],asks:[]}];
  p.publicXhr={async get(url,config){requests.push({url,config});assert(replies.length);return{data:replies.shift()};}};
  const cleanup=s.listenOrderBook(SYMBOL,b=>seen.push(structuredClone(b)));
  const deliver=async body=>{s.onMessage({data:JSON.stringify(body)});for(let i=0;i<6;i++)await Promise.resolve();};
  await deliver({e:'depthUpdate',s:SYMBOL,U:101,u:101,pu:100,b:[['100','4']],a:[]});
  assert.equal(seen.length,1);assert.equal(seen[0].bids[0].amount,4);assert.equal(seen[0].bids[0].total,4);
  await deliver({e:'depthUpdate',s:SYMBOL,U:100,u:100,pu:99,b:[['100','99']],a:[]});assert.equal(seen.length,1);
  await deliver({e:'depthUpdate',s:SYMBOL,U:103,u:103,pu:999,b:[['100','6']],a:[]});assert.equal(seen.length,2);assert.equal(seen[1].bids[0].amount,6);
  assert.equal(requests.length,2);for(const r of requests){assert.equal(r.url,'/dapi/v1/depth');assert.deepEqual(r.config,{params:{symbol:SYMBOL,limit:1000}});}cleanup();cases++;
 }
 {
  timers=new Map();const p=parent(),s=new Private(p);p.xhr={async post(){return{data:{listenKey:'key /+λ'}};}};
  await s.connectAndSubscribe();assert.equal(s.url,'wss://dstream.binance.com/ws/key%20%2F%2B%CE%BB');s.onOpen.call(undefined);assert.deepEqual(s.sent[0],{id:42,method:'LIST_SUBSCRIPTIONS'});
  const event=(X,z,r='')=>({e:'ORDER_TRADE_UPDATE',o:{i:'12345678901234567',s:SYMBOL,S:'BUY',ot:'STOP_MARKET',q:'10',z,l:'2',ap:'50000',p:'0',sp:'49000',R:true,cp:false,wt:'MARK_PRICE',X,r}});
  // Keep a numeric token on the real parser input; its source regex protects it.
  const deliver=body=>s.onMessage({data:JSON.stringify(body).replace('"i":"12345678901234567"','"i":12345678901234567')});
  deliver(event('NEW','0'));const o=p.store.orders[0];assert.equal(o.id,'12345678901234567');assert.equal(o.amount,10);assert.equal(o.price,49000);assert.equal(o.reduceOnly,true);assert.equal(o.slTriggerType,enums.OrderStopTriggerType.MarkPrice);
  deliver(event('PARTIALLY_FILLED','3'));assert.equal(p.fills.length,1);assert.equal(p.fills[0].amount,2);assert.equal(o.filled,3);assert.equal(o.remaining,7);
  deliver(event('CANCELLED','3','GTX would cross'));assert.equal(o.status,enums.OrderStatus.Canceled);assert.equal(o.extra.rejectReason,enums.OrderRejectReason.PostOnly);cases++;
 }
 {
  const p=parent(),s=new Private(p);p.store.positions.push({id:SYMBOL,symbol:SYMBOL,side:'long',contracts:0.1,entryPrice:40000,notional:1000,leverage:17,liquidationPrice:20000,marginType:'isolated',positionMode:'oneway',info:{old:true}});
  s.onMessage({data:JSON.stringify({e:'ACCOUNT_UPDATE',a:{P:[{s:SYMBOL,pa:'-2',ps:'BOTH',ep:'45000',up:'-0.02'}]}})});
  const position=p.store.positions[0];assert.deepEqual([position.contracts,position.notional,position.side,position.leverage,position.liquidationPrice,position.info.contractsNative,position.info.old],[0.004,200,'short',17,20000,2,true]);assert.equal(p.version,1);cases++;
 }
 assert.equal(cases,6);console.log('Maintained inverse socket classes/imported algorithms: 6 actual source groups PASS (wire/data/candle/depth/fill/inverse-position); Base IO ports only.');
})().catch(error=>{console.error(error);process.exitCode=1;});
