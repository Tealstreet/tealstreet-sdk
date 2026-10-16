// Live source capture: Node 26.7.0 on darwin/arm64; runtime clocks are injected per corpus.
import fs from 'node:fs';
import vm from 'node:vm';
import {createRequire} from 'node:module';
const require = createRequire(import.meta.url);
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const root = new URL('../../../', import.meta.url);
function body(path) {
  const source = fs.readFileSync(new URL(path, root), 'utf8');
  const ast = ts.createSourceFile(path, source, ts.ScriptTarget.Latest, true);
  return ast.statements.filter(n => !ts.isImportDeclaration(n) && !ts.isExportDeclaration(n)).map(n => n.getText(ast)).join('\n').replace(/\bexport /g, '');
}
let now = 100_000, clocks, clockCalls=0;
const context = vm.createContext({Date:{now:()=>{const index=clockCalls++;if(clocks){if(index>=clocks.length)throw new Error('PNL-CLOCK unexpected source read');return Number(clocks[index]);}return now;}}, exchangeMetadata:{bybit:{}}, console});
const paths = ['packages/safe-cex/src/types.ts', 'packages/safe-cex/src/webworker/twap/TwapStatus.ts', 'packages/data-structures/src/Queue.ts', 'packages/data-structures/src/QueueWithMaxSize.ts', 'packages/data-structures/src/TimeLimitedQueue.ts', 'packages/safe-cex/src/webworker/postProcessors/BasePostprocessor.ts', 'packages/safe-cex/src/webworker/postProcessors/DisplayedPositionsPostprocessor.ts'];
vm.runInContext(ts.transpileModule(paths.map(body).join('\n')+'\nglobalThis.Processor=DisplayedPositionsPostprocessor;', {compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText, context);
const market = {id:'BTC',symbol:'BTC',base:'BTC',quote:'USD',active:true,precision:{amount:0.01,price:0.01},limits:{amount:{min:0,max:100},leverage:{min:1,max:100}}};
const ticker = {id:'BTC',symbol:'BTC',bid:99,ask:101,last:110,mark:105,index:100,percentage:0,openInterest:0,fundingRate:0,fundingTime:0,volume:0,quoteVolume:0};
const position = {id:'p',symbol:'BTC',side:'long',entryPrice:100,contracts:2,notional:200,leverage:5,unrealizedPnl:0,liquidationPrice:0};
const chaser = {id:'c',symbol:'BTC',size:3,side:'buy',maxAttempts:5,attempts:1,isStopped:false,stopReason:''};
const order = {id:'o',symbol:'BTC',status:'open',type:'stop_market',side:'sell',price:90,amount:1,filled:0,remaining:1,reduceOnly:false,lastUpdate:0};
const twap = {id:'t',symbol:'BTC',accountName:'acct',activeMarket:market,size:3,duration:100,ordersCount:3,side:'buy',chaserDelay:1,reduceOnly:false,randomnessPercentage:0,useLimitOrders:false,useChaserOrders:false,status:'running',filledSize:0,remainingSize:3,completedOrders:0,orders:[],startTime:0,ordersSizes:[1,1,1],ordersExecuted:0,nextOrderAt:0};
const base = {positions:[position],tickers:[ticker],markets:[market],orders:[],activeChasers:[],activeTWAPs:[],balance:{used:0,free:100,total:100,upnl:0}};
const scenarios=[];
for (const side of ['long','short']) for (const contracts of [-2,0,2]) for (const upnl of [0,7]) for (const last of [0,110]) for (const inverse of [false,true]) for (const force of [false,true]) {
 scenarios.push({force,steps:[{now:100_000,store:{...base,positions:[{...position,side,contracts,unrealizedPnl:upnl,extra:{contractsNative:200}}],tickers:[{...ticker,last}],markets:[{...market,isInverse:inverse,contractSize:-10}]}}]});
}
for (const chasers of [[chaser],[chaser,{...chaser,id:'c2'}],[{...chaser,reduceOnly:true}],[{...chaser,isStopped:true}],[{...chaser,side:'sell'}]]) {
 for(const positions of [[],[{...position,contracts:0}],[position]]) scenarios.push({steps:[{now:100_000,store:{...base,positions,activeChasers:chasers,activeTWAPs:[twap]}}]});
}
for(const amount of [0,1,2,-1]) for(const type of ['stop_market','trailing_stop_market','take_profit_market','limit','market']) for(const status of ['open','closed']) scenarios.push({steps:[{now:100_000,store:{...base,orders:[{...order,amount,type,status}],positions:[position,{...position,id:'short',side:'short'}]}}]});
scenarios.push({steps:[0,999,1000,1999,2000,60999,61000,300999,301000].map((now,i)=>({now,store:{...base,positions:[{...position,unrealizedPnl:i+1}]}}))});
scenarios.push({steps:[{now:100_000,store:base},{now:101_000,store:{...base,tickers:[]}},{now:102_000,store:{...base,positions:[{...position,unrealizedPnl:50}]}},{now:103_000,store:{...base,positions:[]}},{now:104_000,store:{...base,positions:[{...position,unrealizedPnl:60}]}}]});
scenarios.push({steps:[{now:100_000,store:{...base,activeChasers:[chaser],positions:[]}},{now:101_000,store:base},{now:102_000,store:{...base,activeChasers:[chaser],positions:[]}}]});
scenarios.push({steps:[1000,2000,61000,62000,301000,302000].map((now,i)=>({now,store:{...base,positions:[{...position,unrealizedPnl:i+1}]}}))});
for (const policy of ['tuning','metadata']) scenarios.push({steps:[false,true,false].map((on,i)=>({now:100_000+i*1000, ...(policy==='tuning'?{options:{tuning:{calculateUpnlFromRealtimeLastPrice:on}}}:{force:on}), store:{...base,positions:[{...position,unrealizedPnl:7}]}}))});
scenarios.push({force:true,options:{tuning:{calculateUpnlFromRealtimeLastPrice:false}},steps:[{now:100_000,store:{...base,positions:[{...position,unrealizedPnl:7}]}}]});
for (const field of ['entryPrice','contracts','unrealizedPnl']) for (const number of ['NaN','Infinity','-Infinity']) scenarios.push({force:true,steps:[{now:100_000,store:base,special:{field,number}}]});
const prototypeFunctions=Object.getOwnPropertyNames(Object.prototype).filter(name=>typeof Object.getOwnPropertyDescriptor(Object.prototype,name).value==='function');
for (const symbol of ['__proto__',...prototypeFunctions]) scenarios.push({steps:[{now:100_000,store:{...base,positions:[{...position,symbol}],tickers:[{...ticker,symbol}],markets:[{...market,symbol}],activeChasers:[{...chaser,symbol}]}}]});
scenarios.push({divergence:'absent-prototype-fake',steps:[{now:100_000,store:{...base,positions:[],tickers:[],markets:[],activeChasers:[{...chaser,symbol:'__proto__'}]}}]});
for(const symbol of prototypeFunctions)scenarios.push({divergence:'absent-function-fake',steps:[{now:100_000,store:{...base,positions:[],tickers:[],markets:[],activeChasers:[{...chaser,symbol}]}}]});
scenarios.push({divergence:'inherited-scalar-ticker',steps:[{now:100_000,store:{...base,positions:[{...position,symbol:'last'}],tickers:[{...ticker,symbol:'__proto__'}],markets:[]}}]});
scenarios.push({identity:true,steps:[{now:100_000,store:{...base,
 positions:[{...position,id:'position-id',entryPrice:31,contracts:32,notional:33,leverage:34,unrealizedPnl:35,liquidationPrice:36}],
 tickers:[{...ticker,id:'ticker-id',bid:41,ask:42,last:43,mark:44,index:45,percentage:46,openInterest:47,fundingRate:48,fundingTime:49,volume:50,quoteVolume:51}],
 markets:[{...market,id:'market-id',base:'BASE',quote:'QUOTE',precision:{amount:0.01,price:0.02},limits:{amount:{min:0.1,max:100},leverage:{min:1,max:1000}}}],
 balance:{used:1.25,free:2.5,total:3.75,upnl:5}
}}]});
for(const values of [[999,1000],[1000,999],['NaN','NaN'],[1000,2000,3000,4000]]) {
 const positions=values.length===4?[{...position,unrealizedPnl:7},{...position,id:'second',unrealizedPnl:9}]:[{...position,unrealizedPnl:7}];
 scenarios.push({clockIdentity:true,steps:[{now:values[0]==='NaN'?0:values[0],clocks:values,store:{...base,positions}}]});
}
scenarios.push({clockIdentity:true,steps:[{now:999,clocks:[999,1000],store:{...base,positions:[],activeChasers:[chaser]}}]});
scenarios.push({clockIdentity:true,steps:[{now:999,clocks:[],store:{...base,tickers:[]}}]});
for (const scenario of scenarios) {
 context.exchangeMetadata.bybit.alwaysCalculateUnrealizedPnlFromLastPrice=!!scenario.force;
 const options=structuredClone(scenario.options||{});
 const processor=new context.Processor('acct','bybit',options);
 for (const step of scenario.steps) { now=step.now;clocks=step.clocks;clockCalls=0;if(step.options)Object.assign(options,step.options);if('force' in step)context.exchangeMetadata.bybit.alwaysCalculateUnrealizedPnlFromLastPrice=step.force;const data=structuredClone(step.store);if(step.special)data.positions[0][step.special.field]=Number(step.special.number);processor.process(data);step.clockCalls=clockCalls;step.expected=data.displayedPositions;if(scenario.divergence==='absent-function-fake')step.lookupKinds=data.displayedPositions.map(p=>[typeof p.ticker,typeof p.market]); }
}
process.stdout.write(JSON.stringify(scenarios));
