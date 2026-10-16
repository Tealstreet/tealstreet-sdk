const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const {load, root} = require('./tables.cjs');
const types = load(path.join(root, 'packages/safe-cex/src/exchanges/kucoin/kucoinspot.types.ts'));
const parsing = load(path.join(root, 'packages/json-parse/src/jsonParse.ts'));
const orderbook = load(path.join(root, 'packages/safe-cex/src/utils/orderbook.ts'));
const source = fs.readFileSync(path.join(root, 'packages/safe-cex/src/exchanges/kucoin/kucoinspot.ws-public.ts'), 'utf8');
const code = ts.transpileModule(source, {compilerOptions: {
  target: ts.ScriptTarget.ESNext, module: ts.ModuleKind.CommonJS, useDefineForClassFields: true,
}}).outputText;
function fixture() {
  const trace = [], frames = [], listeners = [], parsed = [], updates = [];
  let resolve;
  const post = new Promise(r => {resolve = r;});
  class BaseWebSocket {
    constructor(parent) {this.parent = parent; this.isDisposed = false;}
  }
  const module = {exports: {}};
  new Function('require', 'module', 'exports', 'Date', 'performance', code)(name => {
    switch (name) {
      case '@tealstreet/json-parse/src/jsonParse': return {jsonParse: value => {const result = parsing.jsonParse(value); parsed.push(result); return result;}};
      case '../../config/config': return {ENABLE_RECONNECTING_WEBSOCKET: true};
      case '../../utils/orderbook': return orderbook;
      case '../base.ws': return {BaseWebSocket};
      case './kucoinspot.types': return types;
      default: throw Error(`unexpected dependency ${name}`);
    }
  }, module, module.exports, {now: () => 41}, {now: () => 79});
  const ticker = {id: 'native-ticker'};
  const parent = {xhr: {post: url => {trace.push(`post:${url}`); return post;}}, store: {
    tickersBySymbol: {'BTC-USDT': ticker}, updateTicker: (actual, update) => {assert.equal(actual, ticker); updates.push(update);},
  }, emitAndLogError: value => trace.push(`error:${value}`)};
  const Class = module.exports.KucoinSpotPublicWebsocket, r = new Class(parent);
  r.isConnected = true; r.messageHandlers = {};
  r.send = value => frames.push(JSON.parse(value));
  r.createWebsocket = url => {trace.push(`socket:${url}`); return {addEventListener: (event, callback) => listeners.push([event, callback])};};
  r.onClose = () => {};
  for (const key of ['trackLastReceived', 'handlePongEvent', 'recordMessageHandlerStats', 'cleanupMessageHandlerStats', 'retryFailedConnect', 'emitWsReconnected']) {
    r[key] = (...args) => trace.push(['handlePongEvent', 'recordMessageHandlerStats', 'cleanupMessageHandlerStats', 'retryFailedConnect'].includes(key) ? `${key}:${args[0]}` : key);
  }
  return {Class, r, parent, frames, trace, listeners, parsed, updates, resolve};
}
async function run(kind) {
 const f = fixture(), {r} = f;
 if (kind === 'layout') {
   const other = new f.Class(f.parent);
   const names = ['connectAndSubscribe','onOpen','ping','subscribe','subscribeTopic','unsubscribeTopic','onMessage','handleTickerSnapshot','listenOHLCV','listenOrderBook','getCandleHandlerTopic'];
   for (const name of names) assert.notEqual(r[name], other[name]);
   return {fields: Object.keys(other).slice(2), arrows: names.map(name => [r[name].name,r[name].length,r[name].constructor.name === 'AsyncFunction']), prototype: Object.getOwnPropertyNames(f.Class.prototype).filter(name=>name!=='constructor'), interval:r.pingInterval};
 }
 if (kind === 'ingress') {
   r.connectAndSubscribe(); assert.deepEqual(f.trace,['post:/api/v1/bullet-public']); assert.equal(r.token,null);
   f.resolve({data:{code:'200000',data:{token:'token-23',instanceServers:[{endpoint:'wss://socket.example',pingInterval:61}]}}});
   for(let i=0;i<8;i++)await Promise.resolve();
   for(const [event,field] of [['open','onOpen'],['message','onMessage'],['close','onClose']])assert.equal(f.listeners.find(row=>row[0]===event)[1],r[field]);
   r.messageHandlers={'candle_BTC-USDT_1min':[],'orderBook_BTC-USDT':[],ignored:[]};
   r.onOpen.call({isDisposed:true});
   let liveSend=0,pingAt;Object.defineProperty(r,'pingAt',{get:()=>pingAt,set:value=>{pingAt=value;r.send=()=>liveSend++;}});
   r.ping.call({isDisposed:true});assert.equal(pingAt,79);
   r.isDisposed=true;r.onMessage({get data(){r.isDisposed=false;return '{"type":"pong"}';}});
   return {url:r.socketEndpoint,interval:r.pingInterval,frames:f.frames,trace:f.trace,listeners:f.listeners.map(row=>row[0]),liveSend};
 }
 if (kind === 'streams') {
   const candles=[], books=[];
   const cleanCandle=r.listenOHLCV({symbol:'BTC-USDT',interval:'1m'},value=>candles.push(value));
   const cleanOther=r.listenOHLCV({symbol:'BTC-USDT',interval:'1m'},value=>candles.push(value));
   const cleanBook=r.listenOrderBook('BTC-USDT',value=>books.push(value));
   for(const fn of [cleanCandle,cleanOther,cleanBook]){assert.equal(fn.name,'');assert.equal(fn.length,0);}
   assert.equal(r.messageHandlers['candle_BTC-USDT_1min'].length,2);
   const candle={topic:'/market/candles:BTC-USDT_1min',data:{candles:['1700000000123','101','102','103','97','6']}};
   r.onMessage({data:JSON.stringify(candle)});
   r.onMessage({data:JSON.stringify({topic:'/spotMarket/level2Depth50:BTC-USDT',data:{bids:[['99','4'],['100','2']],asks:[['111','3'],['110','5']]}})});
   assert.equal(candles.length,2);assert.equal(books.length,1);
   r.onMessage({data:JSON.stringify({topic:'/market/ticker:all',subject:'BTC-USDT',data:{bestBid:'11',bestAsk:'13',price:'17',changeRate:'0.19',volume:'23',turnover:'29'}})});
   assert.equal(f.updates.length,1);assert.deepEqual(f.updates[0],{bid:11,ask:13,last:17,mark:17,index:17,percentage:19,volume:23,quoteVolume:29});
   let seen; const callbacks=[function(json){callbacks[1]=value=>{seen=value;};},()=>{throw Error('stale second callback');}];
   r.messageHandlers['candle_ETH-USDT_1min']=callbacks;
   r.onMessage({data:'{"topic":"/market/candles:ETH-USDT_1min"}'});
   assert.equal(seen,f.parsed[3]);
   cleanCandle();assert.equal(r.messageHandlers['candle_BTC-USDT_1min'].length,1);cleanOther();cleanBook();
   delete r.messageHandlers['candle_ETH-USDT_1min'];
   assert.deepEqual(Object.keys(r.messageHandlers),[]);
   return {candles,books,update:f.updates[0],frames:f.frames,trace:f.trace,liveAlias:true,remaining:Object.keys(r.messageHandlers)};
 }
 throw Error(`unknown case ${kind}`);
}
run(process.argv[2]).then(value=>process.stdout.write(JSON.stringify(value))).catch(error=>{console.error(error);process.exitCode=1;});
