// Live source capture: validated Node 26.7.0 on darwin/arm64; CI executes on its own host.
import assert from 'node:assert/strict';
import {sourceStore} from '../../safe-cex-store/tests/store_source.mjs';
import fs from 'node:fs';import vm from 'node:vm';import {createRequire} from 'node:module';
const require=createRequire(import.meta.url),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const throttle=require(process.env.SAFE_CEX_WORKER_LODASH_PATH ? process.env.SAFE_CEX_WORKER_LODASH_PATH+'/throttle.js' : 'lodash/throttle');
const root=new URL('../../../',import.meta.url);
function source(name){const text=fs.readFileSync(new URL('packages/safe-cex/src/webworker/handlers/'+name+'.ts',root),'utf8');const ast=ts.createSourceFile(name,text,ts.ScriptTarget.Latest,true);return ast.statements.filter(s=>!ts.isImportDeclaration(s)).map(s=>s.getText(ast)).join('\n').replace(/\bexport /g,'');}
const market={id:'BTC',symbol:'BTC',base:'BTC',quote:'USD',active:true,precision:{amount:0.01,price:0.01},limits:{amount:{min:0,max:100},leverage:{min:1,max:100}}};
const ticker={id:'BTC',symbol:'BTC',bid:99,ask:101,last:100,mark:100,index:100,percentage:0,openInterest:0,fundingRate:0,fundingTime:0,volume:0,quoteVolume:0};
const sub=(id,tag='first',symbol='BTC',interval='1m')=>({op:'sub',data:{subId:id,symbol,interval,tag}}),unsub=id=>({op:'unsub',data:{subId:id}}),emit=(n,timestamp=1)=>({op:'emit',n,timestamp}),advance=t=>({op:'advance',t});
const traces=[
 [sub('a'),sub('b'),{op:'reentrant'},emit(1),advance(100),emit(2)],
 [{op:'markets',loaded:false},sub('a'),advance(100),sub('b'),{op:'markets',loaded:true},advance(500),emit(1),unsub('a'),unsub('b')],
 [{op:'markets',loaded:false},sub('a'),{op:'dispose'},{op:'markets',loaded:true},advance(500),emit(1)],
 [sub('a'),advance(150),emit(1),advance(100),emit(2),advance(160),emit(3),advance(200),advance(250)],
 [sub('a'),sub('a','duplicate'),sub('b','second'),emit(1),advance(10),emit(2),advance(20),emit(3),advance(60),advance(100),unsub('a'),emit(4),advance(200),unsub('b'),emit(5)],
 [sub('a'),emit(1),advance(10),emit(2),advance(20),emit(3,2),advance(40),emit(4,2),advance(60),advance(80),advance(100),advance(160),unsub('a')],
 [sub('a'),emit(1),advance(10),emit(2),unsub('a'),advance(100),sub('a'),emit(3),{op:'dispose'},emit(4),advance(200),unsub('a')],
 [sub('a'),sub('a','duplicate'),unsub('a'),sub('x','unknown','MISSING'),emit(1),advance(200)],
 [sub('same'),sub('same','other','ETH'),unsub('same'),emit(1),advance(100),unsub('same')],
 [sub('a'),emit(1),advance(90),emit(2),advance(100),emit(3),advance(120),emit(4),advance(180),advance(220)],
];
if(process.argv.includes('--nonfinite-witness'))traces.push([sub('a'),{op:'poisonTicker'},emit(1),emit(2)],[sub('a'),{op:'poisonBook'},emit(1)]);
// Exercise the maintained handler and DefaultStore together, before projection.
if (process.argv.includes('--subscriber-identity')) {
 const {Store}=sourceStore();
 const rows=[];
 for (const name of ['Error','TypeError']) {
  const store=new Store();store.update({markets:[market],tickers:[{...ticker}]},true);
  const original=new (name==='TypeError'?TypeError:Error)('bbo subscriber failed');
  let posts=0,first=0,later=0,callback;
  const exchange={name:'bybit',options:{},store,listenOrderBook(symbol,fn){assert.equal(symbol,'BTC');callback=fn;return ()=>{};}};
  const context=vm.createContext({throttle,exchangeMetadata:{bybit:{orderbookTickDelay:100}},console});
  vm.runInContext(ts.transpileModule(source('BaseHandler')+'\n'+source('OrderBookSubscriptionsHandler')+'\nglobalThis.Handler=OrderBookSubscriptionsHandler;', {compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
  const handler=new context.Handler('a','bybit',exchange,{postMessage(){assert.equal(store.tickers[0].bid,99);posts++;}},{},{});
  handler.handle({op:'subscribeOrderBook',data:{subId:'identity',symbol:'BTC'}});
  store.subscribe(()=>{first++;assert.equal(posts,1);assert.equal(store.tickers[0].bid,101);assert.equal(store.tickers[0].ask,103);throw original;});
  store.subscribe(()=>later++);
  let caught;try{callback({bids:[{price:101,amount:2,total:7}],asks:[{price:103,amount:3,total:11}]});}catch(error){caught=error;}
  assert.equal(caught,original,'exact original subscriber Error survives committed Store write');
  assert.equal(first,1);assert.equal(later,0);
  rows.push({name:caught.name,message:caught.message,identity:caught===original,posts,first,later,bid:store.tickers[0].bid,ask:store.tickers[0].ask});
 }
 process.stdout.write(JSON.stringify(rows));
 process.exit(0);
}
const scenarios=[];
const numberKind=n=>Number.isNaN(n)?'NaN':n===Infinity?'Infinity':n===-Infinity?'-Infinity':Object.is(n,-0)?'-0':String(n);
for(const kind of ['orderbook','ohlcv'])for(const multiplier of [0,0.5,1,2,-1])for(const actions of traces){
 let now=0,next=0,reentrant=false,nonfiniteBook=false;const timers=new Map(),callbacks=new Map(),log=[];
 Date.now=()=>now;globalThis.setTimeout=(fn,delay)=>{timers.set(++next,{fn,at:now+Math.max(0,delay|0)});return next};globalThis.clearTimeout=id=>timers.delete(id);
 const store={markets:[market,{...market,id:'ETH',symbol:'ETH'}],tickers:[{...ticker}],updateTicker(t,c){Object.assign(t,c)}};
 const exchange={name:'bybit',options:{tuning:{dataUpdateThrottleMultiplier:multiplier}},store};
 exchange.listenOrderBook=(symbol,callback)=>{log.push(['subscribe',symbol]);callbacks.set(symbol,callback);return()=>{log.push(['unsubscribe',symbol]);callbacks.delete(symbol)}};
 exchange.listenOHLCV=(opts,callback)=>{const key=opts.symbol+':'+opts.interval;log.push(['subscribe',key]);callbacks.set(key,callback);return()=>{log.push(['unsubscribe',key]);callbacks.delete(key)}};
 const context=vm.createContext({Date,setTimeout:globalThis.setTimeout,clearTimeout:globalThis.clearTimeout,throttle,exchangeMetadata:{bybit:{orderbookTickDelay:100}},console:{warn(){}}});
 const name=kind==='orderbook'?'OrderBookSubscriptionsHandler':'OHLCVSubscriptionsHandler';
 vm.runInContext(ts.transpileModule(source('BaseHandler')+'\n'+source(name)+'\nglobalThis.Handler='+name,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
 const handler=new context.Handler('acct','bybit',exchange,{postMessage:message=>{
 log.push(['post',now,message,store.tickers[0].bid,store.tickers[0].ask]);
 if(reentrant){reentrant=false;handler.handle({op:'unsubscribeOHLCV',data:{subId:'b'}});handler.handle({op:'subscribeOHLCV',data:{subId:'c',symbol:'BTC',interval:'1m',tag:'during-post'}});}
 }},{},{});
 for(const action of actions){
  if(action.op==='sub'||action.op==='unsub')handler.handle({op:(action.op==='sub'?'subscribe':'unsubscribe')+(kind==='orderbook'?'OrderBook':'OHLCV'),data:action.data});
  else if(action.op==='advance'){now=action.t;for(const [id,timer] of [...timers])if(timer.at<=now){timers.delete(id);timer.fn()}}
  else if(action.op==='poisonBook')nonfiniteBook=true;
  else if(action.op==='poisonTicker')store.tickers[0].last=NaN;
  else if(action.op==='reentrant')reentrant=kind==='ohlcv';
  else if(action.op==='markets')store.markets=action.loaded?[market,{...market,id:'ETH',symbol:'ETH'}]:[];
  else if(action.op==='dispose')handler.dispose();
  else if(action.op==='emit')for(const callback of [...callbacks.values()])callback(kind==='orderbook'?{bids:[{price:nonfiniteBook?Infinity:100+action.n,amount:2,total:7}],asks:[{price:nonfiniteBook?NaN:102+action.n,amount:3,total:11}]}:{timestamp:action.timestamp,open:action.n,high:action.n+3,low:action.n-2,close:action.n+1,volume:action.n+17});
 }
 scenarios.push({kind,multiplier,actions,market,ticker,finalTicker:store.tickers[0],finalTickerNumbers:Object.fromEntries(["last","bid","ask"].map(k=>[k,numberKind(store.tickers[0][k])])),sentBookNumbers:log.filter(r=>r[0]==="post"&&r[2].data.orderbook).map(r=>({bid:numberKind(r[2].data.orderbook.bids[0].price),ask:numberKind(r[2].data.orderbook.asks[0].price)})),expected:log});
}
process.stdout.write(JSON.stringify(scenarios));
