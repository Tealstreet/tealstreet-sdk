const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript'),root=path.resolve(__dirname,'../../..'),actual=require('node:module').createRequire(path.join(root,'packages/safe-cex/package.json'));
function load(file,require=actual){const exports={};vm.runInNewContext(ts.transpileModule(fs.readFileSync(path.join(root,file),'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022,esModuleInterop:true}}).outputText,{exports,require});return exports;}
const types=load('packages/safe-cex/src/types.ts',()=>({})),constants=load('packages/safe-cex/src/exchanges/woo/woo.types.ts',id=>{assert.equal(id,'../../types');return types;}),utils=load('packages/safe-cex/src/exchanges/woo/woo.utils.ts'),book=load('packages/safe-cex/src/utils/updateOrderBook.ts'),totals=load('packages/safe-cex/src/utils/orderbook.ts'),epoch=load('packages/safe-cex/src/utils/normalizeEpochToSeconds.ts'),candle=load('packages/safe-cex/src/utils/getCandleStartTime.ts',id=>id==='../types'?types:epoch),notNumber=load('packages/math/src/isNotNumber.ts'),math=load('packages/safe-cex/src/utils/safe-math.ts',id=>{assert.equal(id,'@tealstreet/math/src/isNotNumber');return notNumber;});
const file=path.join(root,'packages/safe-cex/src/exchanges/woo/woo.ws-public.ts'),src=fs.readFileSync(file,'utf8'),ast=ts.createSourceFile(file,src,ts.ScriptTarget.Latest,true),klass=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text==='WooPublicWebsocket');
const members=['listenOHLCV','listenOrderBook','processOrderBookUpdate'].map(name=>{const m=klass.members.find(m=>m.name?.getText(ast)===name);assert(m,name);return m.getText(ast);});
let timers=[],nextId=1,warnings=[],reverseCalls=0;
const context={ENDPOINTS:constants.ENDPOINTS,reverseSymbol:s=>{reverseCalls++;return utils.reverseSymbol(s);},getCandleStartTime:candle.getCandleStartTime,add:math.add,calcOrderBookTotal:totals.calcOrderBookTotal,updateOrderBookByPrice:book.updateOrderBookByPrice,nanoid:()=> 'source-inner',setTimeout:(fn,ms)=>{const id=nextId++;timers.push({id,fn,ms});return id;},clearTimeout:id=>{timers=timers.filter(t=>t.id!==id);},console:{warn:(...args)=>warnings.push(args)}};
vm.createContext(context);vm.runInContext(ts.transpileModule(`class Stream{${members.join('\n')}}globalThis.Stream=Stream;`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
const flush=async()=>{for(let i=0;i<32;i++)await Promise.resolve();};const deferred=()=>{let resolve,reject;const promise=new Promise((a,b)=>{resolve=a;reject=b;});return{promise,resolve,reject};};
function setup(publicOnly=false){const e=new context.Stream();e.isConnected=true;e.isDisposed=false;e.messageHandlers={};e.topics={};e.orderBookInnerState={};e.parent={options:{publicDataOnly:publicOnly}};e.sent=[];e.send=text=>e.sent.push(text);e.cleanupMessageHandlerStats=()=>{};return e;}
async function fire(ms){const i=timers.findIndex(t=>t.ms===ms);assert(i>=0);const[t]=timers.splice(i,1);t.fn();await flush();}
const frame=(e,topic,json)=>{assert.equal(e.messageHandlers[topic].length,1);e.messageHandlers[topic][0](json);};
function matrix(book){return {bids:Array.from(book.bids,r=>[r.price,r.amount,r.total]),asks:Array.from(book.asks,r=>[r.price,r.amount,r.total])};}
async function run(c){timers=[];warnings=[];reverseCalls=0;utils.reverseSymbol.cache.clear();constants.ENDPOINTS.ORDERBOOK='/v1/public/orderbook';
 if(c.kind==='candle'){
  const e=setup(c.case==='public-only'),values=[],opts={symbol:'BTCUSDT',interval:c.case==='live'?'1m':'1s'};if(c.case==='retry-after-cleanup')e.isConnected=false;if(c.case==='disposed')e.isDisposed=true;
  const cleanup=e.listenOHLCV(opts,value=>values.push(value));const main='PERP_BTC_USDT@kline_'+opts.interval,secondary='PERP_BTC_USDT@trades';
  if(c.case==='live'){
   frame(e,main,{data:{startTime:60000,open:11,high:13,low:7,close:17,volume:19}});frame(e,main,{data:{startTime:60000,open:997,high:'23',low:'5',close:29,volume:31}});frame(e,main,{data:{startTime:30000,open:991,high:991,low:991,close:991,volume:991}});
   frame(e,secondary,{ts:65000,data:[{p:'37',a:'0.2'}]});frame(e,secondary,{ts:1000,data:[{p:'997',a:'991'}]});frame(e,secondary,{ts:120000,data:[{p:'41',a:'0.3'}]});opts.interval='1s';opts.symbol='ETHUSDT';frame(e,secondary,{ts:125250,data:[{p:'47',a:'0.4'}]});frame(e,secondary,{ts:125500,data:[]});
   assert.equal(values.length,5);assert.equal(warnings.length,1);assert.notEqual(values[0],values[1]);cleanup();const fields=['timestamp','open','high','low','close','volume'];return {candles:values.map(v=>fields.map(k=>v[k])),sent:e.sent};
  }
  if(c.case==='retry-after-cleanup'){assert.equal(timers.length,1);cleanup();assert.equal(timers.length,1);e.isConnected=true;await fire(100);assert.equal(e.messageHandlers[main].length,1);assert.equal(e.messageHandlers[secondary].length,1);cleanup();}
  else if(c.case==='public-only'){assert.equal(e.messageHandlers[main].length,1);assert.equal(e.messageHandlers[secondary],undefined);cleanup();}else{assert.equal(c.case,'disposed');assert.equal(Object.keys(e.messageHandlers).length,0);}
  assert.equal(values.length,0);return {sent:e.sent};
 }
 if(c.kind==='book'){
  const e=setup(),first=deferred(),second=deferred(),values=[],requests=[];e.parent.xhr={get:url=>{requests.push(url);return requests.length===1?first.promise:second.promise;}};
  if(c.case==='connect-cleanup')e.isConnected=false;if(c.case==='disposed')e.isDisposed=true;
  const cleanup=e.listenOrderBook('BTCUSDT',value=>values.push(value)),topic='PERP_BTC_USDT@orderbookupdate';const snapshot={data:{timestamp:100,bids:[{price:19,quantity:23}],asks:[{price:31,quantity:37}]}};
  if(c.case==='buffer'){
   frame(e,topic,{ts:99,data:{bids:[[997,41]],asks:[]}});frame(e,topic,{ts:101,data:{bids:[[19,43]],asks:[]}});frame(e,topic,{ts:'bad',data:{bids:[],asks:[[31,47]]}});frame(e,topic,{event:'ack'});assert.equal(requests.length,1);assert.equal(values.length,0);first.resolve(snapshot);await flush();assert.equal(values.length,1);assert.deepEqual(matrix(values[0]),{bids:[[19,43,43]],asks:[[31,47,47]]});
   frame(e,topic,{ts:102,data:{bids:[[17,53]],asks:[[31,0]]}});assert.equal(values.length,2);assert.equal(values[0],values[1]);assert.deepEqual(matrix(values[0]),{bids:[[19,43,43],[17,53,96]],asks:[]});cleanup();assert.deepEqual(matrix(values[0]),{bids:[],asks:[]});
  }else if(c.case==='retry'){first.reject(Error('transient'));await flush();assert.equal(timers.length,1);await fire(1000);assert.equal(requests.length,2);second.resolve(snapshot);await flush();assert.equal(values.length,1);assert.deepEqual(matrix(values[0]),{bids:[[19,23,23]],asks:[[31,37,37]]});cleanup();assert.equal(timers.length,0);}
  else if(c.case==='late-after-cleanup'){cleanup();first.resolve(snapshot);await flush();assert.equal(values.length,1);assert.deepEqual(matrix(values[0]),{bids:[[19,23,23]],asks:[[31,37,37]]});assert.equal(e.orderBookInnerState['source-inner'].isSnapshotLoaded,true);assert.equal(e.messageHandlers[topic].length,0);}
  else if(c.case==='connect-cleanup'){assert.equal(timers.length,1);cleanup();assert.equal(timers.length,0);}else{assert.equal(c.case,'disposed');assert.equal(requests.length,0);assert.equal(timers.length,0);}
  for(const url of requests)assert.equal(url,'/v1/public/orderbook/PERP_BTC_USDT');return {callbacks:values.length,sent:e.sent};
 }
 assert.equal(c.kind,'interpolation');const e=setup(true);
 if(c.case==='topic'){const opts={symbol:'BTCUSDT',interval:'old'};utils.reverseSymbol.cache.set('BTCUSDT',{toString(){opts.interval='5s';return 'CACHE';}});const cleanup=e.listenOHLCV(opts,()=>{});assert.equal(e.messageHandlers['CACHE@kline_5s'].length,1);cleanup();return {topic:'CACHE@kline_5s'};}
 assert.equal(c.case,'snapshot');constants.ENDPOINTS.ORDERBOOK={toString(){throw Error('endpoint conversion');}};let requests=0;e.parent.xhr={get:()=>{requests++;return Promise.resolve();}};const cleanup=e.listenOrderBook('BTCUSDT',()=>{});await flush();assert.equal(reverseCalls,1);assert.equal(requests,0);assert.equal(timers.length,1);cleanup();assert.equal(timers.length,0);return {reverseCalls};
}
(async()=>{const cases=JSON.parse(fs.readFileSync(0,'utf8')),rows=[];for(const c of cases)rows.push(await run(c));process.stdout.write(JSON.stringify(rows));})().catch(e=>{console.error(e);process.exitCode=1;});
