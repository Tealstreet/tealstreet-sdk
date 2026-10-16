const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const {load, root} = require('./tables.cjs');
const types = load(path.join(root, 'packages/safe-cex/src/exchanges/kucoin/kucoin.types.ts'));
const parsing = load(path.join(root, 'packages/json-parse/src/jsonParse.ts'));
const bookHelpers = load(path.join(root, 'packages/safe-cex/src/utils/orderbook.ts'));
const bookUpdates = load(path.join(root, 'packages/safe-cex/src/utils/updateOrderBook.ts'));
const source = fs.readFileSync(path.join(root, 'packages/safe-cex/src/exchanges/kucoin/kucoin.ws-public.ts'), 'utf8');
const code = ts.transpileModule(source, {compilerOptions: {target:ts.ScriptTarget.ESNext,module:ts.ModuleKind.CommonJS,useDefineForClassFields:true}}).outputText;
const names=['connectAndSubscribe','reconnect','onOpen','onMarketsChanged','ping','subscribe','snapshotShardSymbols','subscribeActiveTicker','unsubscribeActiveTicker','sendChannelOp','onMessage','tickerForApiSymbol','handleTickerV2','handleSnapshot','handleInstrument','listenOHLCV','listenOrderBook'];
const deferred=()=>{let resolve;const promise=new Promise(r=>{resolve=r;});return {promise,resolve};};
async function settle(){for(let i=0;i<16;i++)await Promise.resolve();}
function fixture(opts){
 const trace=[],frames=[],listeners=[],removed=[],parsed=[],updates=[],requests=[],posts=[deferred(),deferred()],snapshot=deferred();let postCount=0,dateCalls=0,perfCalls=0;
 class BaseWebSocket{constructor(parent){this.parent=parent;this.isDisposed=false;}}
 const module={exports:{}};
 new Function('require','module','exports','Date','performance',code)(name=>{switch(name){
  case '@tealstreet/json-parse/src/jsonParse':return{jsonParse:value=>{const result=parsing.jsonParse(value);parsed.push(result);return result;}};
  case '../../config/config':return{ENABLE_RECONNECTING_WEBSOCKET:true};
  case '../../utils/orderbook':return bookHelpers;
  case '../../utils/updateOrderBook':return bookUpdates;
  case '../base.ws':return{BaseWebSocket};
  case './kucoin.types':return types;
  default:throw Error(`unexpected dependency ${name}`);
 }},module,module.exports,{now:()=>{dateCalls++;return 41;}},{now:()=>{perfCalls++;return 79;}});
 const ticker={id:'ticker-1',bid:7,ask:9};
 const parent={store:{markets:Array.from({length:205},(_,i)=>({id:`API-${i}`,symbol:`S-${i}`})),tickersBySymbol:{'S-1':ticker},updateTicker:(actual,update)=>{assert.equal(actual,ticker);updates.push(update);}},toApiSymbol:symbol=>symbol.replace('S-','API-'),toInternalSymbol:symbol=>symbol.replace('API-','S-'),xhr:{post:url=>{trace.push(`post:${url}`);return posts[postCount++].promise;}},emitter:{},emitAndLogError:message=>trace.push(`error:${message}`)};
 const Class=module.exports.KucoinPublicWebsocket,r=new Class(parent,opts),fields=Object.keys(r).slice(2);
 r.isConnected=true;r.onClose=()=>{};
 r.send=value=>frames.push(JSON.parse(value));r.createWebsocket=url=>{trace.push(`socket:${url}`);const socket={addEventListener(event,callback){assert.equal(this,socket);listeners.push([event,callback]);},removeEventListener(event,callback){assert.equal(this,socket);assert.equal(callback,r[{open:'onOpen',message:'onMessage',close:'onClose'}[event]]);removed.push(event);},close(){assert.equal(this,socket);assert.equal(r.ws,undefined);trace.push('close');}};return socket;};
 for(const key of['trackLastReceived','handlePongEvent','recordMessageHandlerStats','cleanupMessageHandlerStats','retryFailedConnect','emitWsReconnected'])r[key]=(...args)=>trace.push(args.length?`${key}:${args[0]}`:key);
 for(const key of['off','on'])parent.emitter[key]=function(event,callback){assert.equal(this,parent.emitter);assert.equal(event,'markets:changed');assert.equal(callback,r.onMarketsChanged);trace.push(`emitter.${key}`);};
 const get=function(route,opts){assert.equal(this,parent.xhr);assert.equal(route,'/probe-orderbook');assert.equal(opts.params.symbol,'API-1');requests.push([route,opts]);return snapshot.promise;};
 Object.defineProperty(parent.xhr,'get',{get(){types.ENDPOINTS.ORDERBOOK='/probe-orderbook';return get;}});
 return{Class,r,parent,fields,trace,frames,listeners,removed,parsed,updates,requests,posts,snapshot,clocks:()=>[dateCalls,perfCalls]};
}
async function run(kind){
 const f=fixture(kind==='snapshot'||kind==='reconnect'?{role:'snapshot',shardIndex:1,shardCount:2}:undefined),{r}=f;
 if(kind==='layout'){
  const other=new f.Class(f.parent,{role:null,shardIndex:0,shardCount:0});for(const name of names)assert.notEqual(r[name],other[name]);
  assert.equal(other.role,'detail');assert.equal(other.shardIndex,0);assert.equal(other.shardCount,0);
  assert.deepEqual(f.clocks(),[0,0]);return{clocks:f.clocks(),fields:f.fields,arrows:names.map(name=>[r[name].name,r[name].length,r[name].constructor.name==='AsyncFunction']),prototype:Object.getOwnPropertyNames(f.Class.prototype).filter(name=>name!=='constructor'),defaults:[r.role,r.shardIndex,r.shardCount],nullish:[other.role,other.shardIndex,other.shardCount]};
 }
 if(kind==='snapshot'){
  const ids=r.snapshotShardSymbols();assert.equal(ids.length,102);for(let i=0;i<102;i++)assert.equal(ids[i],`API-${2*i+1}`);
  r.subscribe();assert.equal(f.frames.length,2);r.subscribeActiveTicker('API-1');r.unsubscribeActiveTicker('API-1');assert.equal(f.frames.length,6);
  r.ws={};r.onMarketsChanged({added:[{id:'API-1',symbol:'S-1'},{id:'API-2',symbol:'S-2'},{id:'',symbol:'S-3'}]});assert.equal(f.frames.length,7);assert.equal(f.frames[6].topic,'/contractMarket/snapshot:API-1,API-3');
  assert.deepEqual(f.clocks(),[7,0]);return{clocks:f.clocks(),ids,frames:f.frames};
 }
 if(kind==='reconnect'){
  r.connectAndSubscribe();assert.equal(r.token,null);f.posts[0].resolve({data:{code:'200000',data:{token:'token-23',instanceServers:[{endpoint:'wss://socket.example'}]}}});await settle();
  for(const[event,field]of[['open','onOpen'],['message','onMessage'],['close','onClose']])assert.equal(f.listeners.find(row=>row[0]===event)[1],r[field]);
  r.onOpen.call({isDisposed:true});assert.equal(r.pingAt,79);assert.equal(f.frames.length,3);assert.deepEqual(f.trace.slice(-2),['emitter.on','emitWsReconnected']);
  let finished=false;r.reconnect().then(()=>finished=true);assert.deepEqual(f.removed,['open','message','close']);assert.equal(r.ws,undefined);assert.equal(r.token,null);assert.equal(r.socketEndpoint,null);assert.equal(finished,false);await settle();assert.equal(finished,false);
  f.posts[1].resolve({data:{code:'200000',data:{token:'token-29',instanceServers:[{endpoint:'wss://other.example'}]}}});await settle();assert.equal(finished,true);
  assert.deepEqual(f.clocks(),[5,1]);return{clocks:f.clocks(),url:r.socketEndpoint,trace:f.trace,removed:f.removed,listeners:f.listeners.map(row=>row[0]),frames:f.frames,retained:true};
 }
 if(kind==='streams'){
  const candles=[],books=[];const cleanCandle=r.listenOHLCV({symbol:'S-1',interval:'1m'},value=>candles.push(value));const cleanBook=r.listenOrderBook('S-1',value=>books.push(value));
  assert.equal(f.requests.length,1);assert.equal(books.length,0);
  r.onMessage({data:'{"topic":"/contractMarket/limitCandle:API-1_1min","data":{"candles":["1700000000","101","102","103","97","6"]}}'});assert.equal(candles.length,1);
  r.onMessage({data:'{"topic":"/contractMarket/level2:API-1","type":"update","data":{"sequence":101,"change":"100,buy,5"}}'});
  f.snapshot.resolve({data:{code:'200000',data:{sequence:100,bids:[['100','2']],asks:[['110','3']]}}});await settle();assert.equal(books.length,1);assert.equal(books[0].bids[0].amount,5);
  r.onMessage({data:'{"topic":"/contractMarket/level2:API-1","type":"update","data":{"sequence":102,"change":"99,buy,7"}}'});assert.equal(books.length,2);assert.equal(books[0],books[1]);const book=JSON.parse(JSON.stringify(books[1]));
  for(const payload of[{topic:'/contractMarket/tickerV2:API-1',data:{symbol:'API-1',bestBidPrice:'11',bestAskPrice:'13'}},{topic:'/contractMarket/snapshot:API-1',data:{symbol:'API-1',lastPrice:'17',priceChgPct:'0.19',fundingRate:'0.023',volume:'29',turnover:'31'}},{topic:'/contract/instrument:API-1',data:{markPrice:'37',indexPrice:'41'}}])r.onMessage({data:JSON.stringify(payload)});assert.equal(f.updates.length,3);
  cleanCandle();cleanBook();assert.deepEqual(Object.keys(r.messageHandlers),['candle_S-1_1min','orderBook_S-1']);for(const key of ['candle_S-1_1min','orderBook_S-1'])assert.deepEqual(r.messageHandlers[key],[]);assert.deepEqual(books[0],{bids:[],asks:[]});
  assert.deepEqual(f.clocks(),[8,0]);return{clocks:f.clocks(),candles,book,updates:f.updates,frames:f.frames,trace:f.trace,requests:f.requests,liveBook:true,cleared:true,remaining:Object.keys(r.messageHandlers)};
 }
 throw Error(`unknown case ${kind}`);
}
run(process.argv[2]).then(value=>process.stdout.write(JSON.stringify(value))).catch(error=>{console.error(error);process.exitCode=1;});
