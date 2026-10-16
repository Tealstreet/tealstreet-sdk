const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm'), assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH ?? 'typescript');
const {captureHeader, numberBits} = require('./special-numbers.cjs');
const root = path.resolve(__dirname, '../../..');
function source(rel) { const p = path.join(root, 'packages', rel); return ts.createSourceFile(p, fs.readFileSync(p, 'utf8'), ts.ScriptTarget.Latest, true); }
function variable(file, name) { const node = file.statements.filter(ts.isVariableStatement).flatMap(node => [...node.declarationList.declarations]).find(node => node.name.getText(file) === name); assert(node?.initializer, `live ${name}`); return `const ${name}=${node.initializer.getText(file)};`; }
function klass(file, name) { const node = file.statements.find(node => ts.isClassDeclaration(node) && node.name.text === name); assert(node, `live ${name}`); return node; }
function member(file, name, property) { const node = klass(file, name).members.find(node => node.name?.getText(file) === property); assert(node, `live ${name}.${property}`); return node.getText(file); }
const ws = source('safe-cex/src/exchanges/coinbase/coinbase.ws-public.ts');
const base = source('safe-cex/src/exchanges/base.ws.ts');
const util = source('safe-cex/src/exchanges/coinbase/coinbase.utils.ts');
const math = source('safe-cex/src/utils/safe-math.ts');
const types = source('safe-cex/src/exchanges/coinbase/coinbase.types.ts');
const body = [
 variable(types, 'BASE_WS_URL'), variable(source('safe-cex/src/types.ts'), 'timeframeToSeconds'),
 ...['COINBASE_USDC_ALIAS_EXCEPTIONS', 'normalizeCoinbaseUsdEquivalentProductId'].map(name => variable(util, name)),
 variable(source('math/src/isNotNumber.ts'), 'isNotNumber'),
 math.statements.filter(node => !ts.isImportDeclaration(node)).map(node => node.getText(math)).join('\n'),
 ...['orderbook.ts', 'updateOrderBook.ts'].map(name => source('safe-cex/src/utils/' + name)).map(file => file.statements.filter(node => !ts.isImportDeclaration(node)).map(node => node.getText(file)).join('\n')),
 `class BaseWebSocket {
 constructor(parent) { this.parent=parent; this.trace=parent.trace; }
 ${member(base, 'BaseWebSocket', 'orderBookInnerState')}
 ${member(base, 'BaseWebSocket', 'resetOrderbook')}
 ${member(base, 'BaseWebSocket', 'isConnected')}
 get isDisposed(){this.parent.disposedReads++;return this.parent.isDisposed;}
 createWebsocket(url){this.trace.push({kind:'create',url});return {readyState:0,addEventListener:(event,callback)=>this.trace.push({kind:'listener',event,bound:callback===this['on'+event[0].toUpperCase()+event.slice(1)]})};}
 onClose=()=>{};
 send(raw){this.trace.push({kind:'send',payload:JSON.parse(raw)});}
 trackLastReceived(){this.trace.push({kind:'received'});}
 recordMessageHandlerStats(topic){this.trace.push({kind:'stats',topic});}
 cleanupMessageHandlerStats(topic){this.trace.push({kind:'cleanup',topic});}
 emitWsReconnected(){this.trace.push({kind:'reconnected'});}
 handlePongEvent(){this.trace.push({kind:'pong'});}
 }`,
 klass(ws, 'CoinbasePublicWebsocket').getText(ws),
 'export {CoinbasePublicWebsocket};',
].join('\n');
let activeTimers, activeTrace;
const realDayjs = require('dayjs'); realDayjs.extend(require('dayjs/plugin/utc'));
const dayjs = (...args) => realDayjs(args.length ? args[0] : 1700000000000);
const output = {};
vm.runInNewContext(ts.transpileModule(body, {compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText, {
 exports: output, BigNumber: require('bignumber.js'), dayjs, WebSocket:{OPEN:1}, ENABLE_RECONNECTING_WEBSOCKET:true,
 nanoid:()=> 'inner', jsonParse: raw=>{try{return JSON.parse(raw);}catch{return null;}},
 setTimeout:(callback,delay)=>{activeTimers.push(callback);activeTrace.push({kind:'timer',delay});},
 console:{warn:(message,value)=>activeTrace.push({kind:'warn',message,value}),error:message=>activeTrace.push({kind:'error',message})},
});
function setup() {
 const trace=[],timers=[],parent={trace,isDisposed:false,disposedReads:0,options:{testnet:false,publicDataOnly:false},store:{markets:[{symbol:'BTC-USD'}]},dailyCandles:{},emitter:{emit(){throw Error('unexpected error emitter');}}};
 parent.fetchOHLCV=opts=>{trace.push({kind:'fetch',opts});return Promise.resolve([]);};
 activeTimers=timers;activeTrace=trace;
 const socket=new output.CoinbasePublicWebsocket(parent);
 return {socket,parent,trace,timers};
}
function capture(value) {
 if(value===undefined)return {$undefined:true};
 if(typeof value==='number')return {$bits:numberBits(value)};
 if(Array.isArray(value))return value.map(capture);
 if(value&&typeof value==='object')return Object.fromEntries(Object.entries(value).map(([key,value])=>[key,capture(value)]));
 return value;
}
function candle(socket, product='BTC-USD', close='2', volume='5') { socket.onMessage({data:JSON.stringify({channel:'candles',events:[{type:'update',candles:[{product_id:product,start:'60',open:'1',high:'3',low:'0.5',close,volume}]}]})}); }
function book(socket,type,price,amount) { socket.onMessage({data:JSON.stringify({channel:'l2_data',events:[{type,product_id:'BTC-USD',updates:[{side:'bid',price_level:price,new_quantity:amount}]}]})}); }
const result={...captureHeader()};
{
 const {socket,parent,trace,timers}=setup(),first=[],second=[];
 const cleanup1=socket.listenOHLCV({symbol:'BTC-USDC',interval:'5m'},value=>{first.push(value);value.high=41;});
 assert.equal(timers.length,1,'one initial disconnected retry');
 socket.connectAndSubscribe(); timers.shift()();
 assert.equal(timers.length,1,'actual closed socket retries again');
 socket.ws.readyState=1;timers.shift()();
 const cleanup2=socket.listenOHLCV({symbol:'BTC-USDC',interval:'5m'},value=>second.push(value));
 candle(socket);
 assert.equal(first.length,1,'first class candle callback');assert.equal(second.length,1,'second class candle callback');
 assert.equal(first[0],second[0],'class callbacks share formatted native candle');assert.equal(second[0].high,41,'later callback sees earlier mutation');
 const primary=capture(second[0]);
 socket.onMessage({data:JSON.stringify({channel:'market_trades',events:[{type:'update',trades:[{product_id:'BTC-USD',price:'4',size:'8',time:'1970-01-01T00:01:01.000Z'}]}]})});
 assert.equal(first.length,2,'class trade first callback');assert.equal(second.length,2,'class trade second callback');
 const trade=capture(second[1]); cleanup1(); candle(socket);assert.equal(first.length,2,'removed class callback stays absent');assert.equal(second.length,3,'surviving class callback runs');
 cleanup2();candle(socket);assert.equal(second.length,3,'last cleanup stops delivery');
 result.routing={primary,trade,final:capture(second[2]),disposedReads:parent.disposedReads,firstCount:first.length,secondCount:second.length,topics:capture(socket.topics),trace:capture(trace)};
}
{
 const {socket,parent,trace}=setup(),roots=[],captures=[];
 socket.connectAndSubscribe();socket.ws.readyState=1;
 const cleanup=socket.listenOrderBook('BTC-USDC',value=>{roots.push(value);captures.push(capture(value));value.bids[0].amount=17;});
 book(socket,'snapshot','10','2');
 const originalBook=roots[0],oldBids=originalBook.bids,oldLevel=oldBids[0];
 socket.orderBookInnerState.inner.updates.push({sentinel:true});socket.onOpen();
 assert.equal(socket.orderBookInnerState.inner.isSnapshotLoaded,false,'class reconnect resets actual listener loaded flag');
 assert.equal(socket.orderBookInnerState.inner.updates.length,0,'class reconnect replaces actual pending updates');
 assert.equal(originalBook.bids,oldBids,'reconnect retains old side array');assert.equal(oldBids[0],oldLevel,'reconnect retains old level root');assert.equal(oldLevel.amount,17,'reconnect retains callback mutation');
 book(socket,'update','10','99');assert.equal(roots.length,2,'pre-snapshot update can publish retained book');assert.equal(roots[1],originalBook,'pre-snapshot callback keeps book root');assert.equal(oldLevel.amount,17,'pre-snapshot delta does not apply');
 book(socket,'snapshot','23','4');assert.equal(roots.length,3,'new snapshot publishes');assert.equal(roots[2],originalBook,'new snapshot keeps book root');assert.notEqual(originalBook.bids,oldBids,'fresh snapshot replaces side array');assert.equal(oldBids[0],oldLevel,'old side survives replacement');
 parent.isDisposed=true;const before=trace.length;socket.onOpen();assert.equal(trace.length,before,'disposed class onOpen has no subscriptions');assert.equal(socket.orderBookInnerState.inner.isSnapshotLoaded,true,'disposed class onOpen does not reset state');
 cleanup();book(socket,'update','23','8');assert.equal(roots.length,3,'disposed/cleaned class has no delivery');
 result.reconnect={captures,disposedReads:parent.disposedReads,oldAmount:capture(oldLevel.amount),loaded:socket.orderBookInnerState.inner.isSnapshotLoaded,topics:capture(socket.topics),trace:capture(trace)};
}
{
 const {socket,parent,trace}=setup(),values=[],snapshots=[];socket.connectAndSubscribe();socket.ws.readyState=1;
 const opts={symbol:'ETH-USDC',interval:'1d'};const cleanup=socket.listenOHLCV(opts,value=>{values.push(value);snapshots.push(capture(value));value.volume=99;});
 candle(socket,'ETH-USD','24','3');assert.equal(values.length,0,'daily waits for bound parent cache');
 const request=trace.find(value=>value.kind==='fetch').opts;assert.equal(request.symbol,opts.symbol,'daily parent original alias symbol');assert.equal(request.interval,'1d');assert.equal(request.limit,2);
 const previous={timestamp:0,open:10,high:20,low:5,close:10,volume:11};parent.dailyCandles[opts.symbol]=previous;
 candle(socket,'ETH-USD','24','3');assert.equal(values.length,1);assert.equal(parent.dailyCandles[opts.symbol],values[0],'daily callback and parent cache are same native root');assert.notEqual(previous,values[0],'daily formatting replaces source record');
 candle(socket,'ETH-USD','25','4');assert.equal(values.length,2);assert.equal(parent.dailyCandles[opts.symbol],values[1],'later daily result replaces same parent slot');
 assert.equal(values[1].close,25);assert.deepEqual(snapshots[1].volume,{$bits:numberBits(103)},'daily callback mutation feeds next parent cache read');
 cleanup();result.daily={disposedReads:parent.disposedReads,request:capture(request),values:values.map(capture),previous:capture(previous),snapshots,trace:capture(trace)};
}
{
 const left=setup();left.socket.connectAndSubscribe();left.socket.ws.readyState=1;
 let first=0;const a=left.socket.listenOHLCV({symbol:'BTC-USDC',interval:'5m'},()=>first++);
 const right=setup();right.socket.connectAndSubscribe();right.socket.ws.readyState=1;
 let second=0;const b=right.socket.listenOHLCV({symbol:'BTC-USDC',interval:'5m'},()=>second++);
 assert.notEqual(left.socket.messageHandlers,right.socket.messageHandlers);assert.notEqual(left.socket.topics,right.socket.topics);
 candle(left.socket);assert.equal(first,1);assert.equal(second,0);
 a();candle(right.socket);assert.equal(first,1);assert.equal(second,1);b();
 result.isolation={first,second,leftDisposedReads:left.parent.disposedReads,rightDisposedReads:right.parent.disposedReads};
}
process.stdout.write(JSON.stringify(result));
