const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=path.resolve(__dirname,'../../..');
function members(file,klass,names){const text=fs.readFileSync(path.join(root,file),'utf8'),ast=ts.createSourceFile(file,text,ts.ScriptTarget.Latest,true),c=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text===klass);assert(c);return names.map(name=>{const m=c.members.find(m=>m.name?.getText(ast)===name);assert(m,name);return m.getText(ast);}).join('\n');}
const exchange=members('packages/safe-cex/src/exchanges/woo/woo.exchange.ts','WOOXExchange',['start','tick','fetchHistoricalPositions']);
const book=members('packages/safe-cex/src/exchanges/woo/woo.ws-public.ts','WooPublicWebsocket',['processOrderBookUpdate']);
const utility={};vm.runInNewContext(ts.transpileModule(fs.readFileSync(path.join(root,'packages/safe-cex/src/utils/updateOrderBook.ts'),'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,{exports:utility});
let trace,mode,intervals;
const context={Date:{now:()=>{trace.push('now');return 3000000000;}},loop:(cb,interval)=>{trace.push('loop');assert.equal(interval,137);if(mode==='loop-throw')throw Error('loop failed');},wooOrdersToExecutions:orders=>{assert.deepEqual(orders,['order-axis']);trace.push('executions');return orders;},reconstructPositionIntervals:()=>{trace.push('intervals');return intervals;},mapFoldedInterval:(row,options)=>{trace.push('fold:'+row.id);assert.equal(options.account,'account-41');const settle=options.resolveSettleCurrency(row.id==='next'?'UNKNOWN':'BTC');assert.equal(settle,row.id==='next'?'USDT':'USDC');return row.id==='empty'?null:{id:row.id,settle};},updateOrderBookByPrice:()=>{throw Error('unbound book fixture');}};
vm.createContext(context);vm.runInContext(ts.transpileModule(`class Lifecycle{${exchange}\n${book}};globalThis.Lifecycle=Lifecycle;`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
const flush=async()=>{for(let i=0;i<32;i++)await Promise.resolve();};
const deferred=()=>{let resolve,reject;const promise=new Promise((a,b)=>{resolve=a;reject=b;});return {promise,resolve,reject};};
function setup(){const e=new context.Lifecycle();e.isDisposed=false;e.isPublicDataOnly=false;const store=e.store={loaded:{old:true}};store.update=patch=>{trace.push('update:'+Object.keys(patch).join(','));Object.assign(store,patch);};store.addOrUpdatePositionsFull=()=>trace.push('positions:update');e.publicWebsocket={connectAndSubscribe:()=>trace.push('public')};e.privateWebsocket={connectAndSubscribe:()=>trace.push('private')};e.startFundingRatesRefresh=()=>trace.push('funding');e.sourceTick=e.tick;e.tick=()=>{trace.push('tick');return Promise.resolve();};e.log=value=>{assert.equal(value,'Ready to trade on WOO X');trace.push('log');};e.getTickInterval=()=>{trace.push('interval');return 137;};e.emitAndLogError=value=>{trace.push('emit');assert.equal(value,'fetch failed');if(mode==='emit-throw')throw Error('emit failed');};return e;}
async function run(c){trace=[];mode=c.case;const e=setup();
 if(c.kind==='start'){
  const markets=deferred(),raw=deferred(),balance=deferred(),positions=deferred(),orders=deferred();
  e.initExternalPublicData=()=>{trace.push(c.case==='parallel'?'initExternalPublicData':'init');return Promise.resolve(c.case==='external-public-only');};
  e.fetchMarkets=()=>{trace.push(c.case==='parallel'?'fetchMarkets':'markets');return markets.promise;};e.fetchTickersRaw=()=>{trace.push(c.case==='parallel'?'fetchTickersRaw':'raw');return raw.promise;};
  e.parseTickers=rows=>{trace.push('parse');return rows;};
  for(const[key,gate]of[['fetchBalance',balance],['fetchPositions',positions],['fetchOrders',orders]])e[key]=()=>{trace.push(key);return gate.promise;};
  e.isPublicDataOnly=c.case!=='parallel';let error;const p=context.Lifecycle.prototype;void p;
  const start=e.start();const guarded=start.catch(e=>{error=e.message;});
  assert.deepEqual(trace,[c.case==='parallel'?'initExternalPublicData':'init']);await flush();
  if(c.case==='parallel'){
   assert.deepEqual(trace,['initExternalPublicData','fetchMarkets','fetchTickersRaw']);markets.resolve([{id:'market-11'}]);await flush();assert.deepEqual(trace,['initExternalPublicData','fetchMarkets','fetchTickersRaw','update:markets,loaded','public']);
   e.isPublicDataOnly=true;raw.resolve([{id:'ticker-13'}]);await flush();assert(trace.includes('private'));
   orders.resolve([{id:'order-17'}]);await flush();balance.resolve({free:19});await flush();positions.resolve([{id:'position-23'}]);await guarded;
  }else if(c.case==='rejected'){
   raw.reject(Error('raw rejected'));await guarded;assert.equal(error,'raw rejected');markets.resolve(['late']);await flush();
  }else if(c.case==='disposed'){
   e.isDisposed=true;markets.resolve([]);raw.resolve([]);await guarded;
  }else{assert.equal(c.case,'external-public-only');await guarded;}
 }else if(c.kind==='tick'){
  const positions=[{id:'position-29'}],balance={free:31},gate=deferred();
  e.fetchPositionsAndBalance=()=>{trace.push('full');return ['disposed-after-await','fetch-reject','emit-throw'].includes(c.case)?gate.promise:Promise.resolve({positions,balance});};
  e.fetchPositions=()=>{trace.push('positions');return Promise.resolve(positions);};e.fetchOrders=()=>{trace.push('orders');return Promise.resolve([{id:'order-37'}]);};
  e.tick=e.sourceTick;const count=c.case==='one'?1:c.case==='five'?5:c.case==='string-five'?'5':undefined;let error;const pending=e.tick(count).catch(e=>{error=e.message;});
  assert.deepEqual(trace,[c.case==='one'?'positions':'full']);
  if(c.case==='disposed-after-await'){e.isDisposed=true;gate.resolve({positions,balance});}else if(['fetch-reject','emit-throw'].includes(c.case))gate.reject(Error('fetch failed'));
  await pending;if(c.case==='loop-throw'||c.case==='emit-throw')assert.equal(error,c.case==='loop-throw'?'loop failed':'emit failed');else assert.equal(error,undefined);
 }else if(c.kind==='history'){
  const threshold=c.case==='default'?408000000:11;intervals=[{id:'open'},{id:'old',closedAt:threshold-1},{id:'keep',closedAt:threshold},{id:'empty',closedAt:threshold+1},{id:'next',closedAt:threshold+2}];
  e.store.marketsBySymbol={BTC:{quote:'USDC'}};e.fetchHistoricalOrders=opts=>{trace.push('orders');assert.equal(opts.from,threshold);assert.equal(opts.to,c.case==='default'?3000000000:19);return c.case==='reject'?Promise.reject(Error('history denied')):Promise.resolve(['order-axis']);};
  let error;const result=await e.fetchHistoricalPositions(c.case==='default'?{account:'account-41',to:null}:{account:'account-41',from:11,to:19}).catch(e=>{error=e.message;});
  if(c.case==='reject')assert.equal(error,'history denied');else assert.deepEqual(JSON.parse(JSON.stringify(result)),[{id:'keep',settle:'USDC'},{id:'next',settle:'USDT'}]);
 }else if(c.kind==='book'){
  const orderBook={bids:[{price:19,amount:23,total:29}],asks:[]};let reads=0;const first=[[19,31]],bids=new Proxy(first,{get(target,key,receiver){if(key==='0'&&++reads===2)return[997,37];return Reflect.get(target,key,receiver);}});
  context.updateOrderBookByPrice=(book,side,price,amount)=>{assert.equal(book,orderBook);trace.push(`${side}:${price}:${amount}`);utility.updateOrderBookByPrice(book,side,price,amount);if(side==='bids'&&price===19)first.push([17,47]);};
  e.processOrderBookUpdate(orderBook,{data:{bids,asks:[[41,43]]}});assert.equal(reads,2);assert.deepEqual(orderBook.bids.map(r=>[r.price,r.amount,r.total]),[[19,37,29],[17,47,0]]);assert.deepEqual(orderBook.asks.map(r=>[r.price,r.amount,r.total]),[[41,43,0]]);
 }else throw Error('unaccounted case');return {trace};
}
(async()=>{const cases=JSON.parse(fs.readFileSync(0,'utf8'));const out=[];for(const c of cases)out.push(await run(c));process.stdout.write(JSON.stringify(out));})().catch(e=>{console.error(e);process.exitCode=1;});
