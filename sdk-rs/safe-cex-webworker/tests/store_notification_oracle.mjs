// Live source capture: validated Node 26.7.0 on darwin/arm64; CI executes on its own host.
import fs from 'node:fs';
import vm from 'node:vm';
import crypto from 'node:crypto';
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const entry=require.resolve(process.env.SAFE_CEX_WORKER_LODASH_PATH?process.env.SAFE_CEX_WORKER_LODASH_PATH+'/lodash.js':'lodash');
assert.equal(crypto.createHash('sha256').update(fs.readFileSync(entry)).digest('hex'),'f5465f55566bf544aad0a31c6135889ca1ed81eea8f53ec61c6cbe86926f07cf');
const lodash=require(entry);assert.equal(lodash.VERSION,'4.18.1');
const root=new URL('../../../packages/safe-cex/src/',import.meta.url);
function ast(path){return ts.createSourceFile(path,fs.readFileSync(new URL(path,root),'utf8'),ts.ScriptTarget.Latest,true);}
function body(path){
 const a=ast(path);let source=a.statements.filter(n=>!ts.isImportDeclaration(n)).map(n=>n.getText(a)).join('\n').replace(/\bexport /g,'');
 if(path==='webworker/handlers/UiStateHandler.ts'&&process.env.SAFE_CEX_WORKER_LEVERAGE_SYMBOL_CONTROL==='replace-d800'){
  const before='pos.symbol === symbol';assert(source.includes(before),'LEVERAGE source mutation anchor');
  source=source.replace(before,"pos.symbol.replace(/\\uD800/g, '\\uFFFD') === symbol");
 }
 return source;
}
const types=ast('types.ts'),names=['OrderSide','OrderType','OrderStatus','PositionSide','PositionMode'];
const enums=types.statements.filter(n=>ts.isEnumDeclaration(n)&&names.includes(n.name.text));
assert.equal(enums.length,names.length);
let events=[],caught=[];
const context=vm.createContext({Error,structuredClone,uniqBy:lodash.uniqBy,isEqual:lodash.isEqual,Date:{now:()=>10000},console:{error(...args){caught.push(args.find(v=>v instanceof Error));},warn(){},log(){}}});
function run(source){vm.runInContext(ts.transpileModule(source,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);}
const clockAst=ast('utils/virtualClock.ts');
const clock=clockAst.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text==='VirtualClock');assert.ok(clock);
run(clock.getText(clockAst)+'\nconst virtualClock=new VirtualClock();\n'+enums.map(n=>n.getText(types).replace('export ','')).join('\n')+'\n'+['utils/clone.ts','store/store.base.ts'].map(body).join('\n')+'\nglobalThis.Store=DefaultStore;');
for(const name of ['BaseHandler','ThrottleManager','UiStateHandler','UpdatePositionsHandler','CallMethodHandler','OrderBookSubscriptionsHandler']){
 run(`(()=>{${body(`webworker/handlers/${name}.ts`)}\nglobalThis.${name}=${name};})();`);
}
const base=ast('exchanges/base.ts');const cls=base.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text==='BaseExchange');
assert.ok(cls,'actual Exchange class');
const remove=cls.members.find(n=>n.name?.getText(base)==='removeOrders');assert.ok(remove);
run(`class RemovalSlice { constructor(store){this.store=store;} ${remove.getText(base)} } globalThis.RemovalSlice=RemovalSlice;`);
const position={id:'p',symbol:'BTC',side:'long',entryPrice:100,contracts:2,notional:200,leverage:5,unrealizedPnl:10,liquidationPrice:0};
const ticker={id:'BTC',symbol:'BTC',bid:99,ask:101,last:100,mark:100,index:100,percentage:0,openInterest:0,fundingRate:0,fundingTime:0,volume:0,quoteVolume:0};
const order={id:'o',symbol:'BTC',status:'open',type:'limit',side:'buy',price:100,amount:1,filled:0,remaining:1,reduceOnly:false,lastUpdate:0};
function view(store){return {leverage:store.positions[0]?.leverage,orders:store.orders.map(o=>o.id),bid:store.tickers[0]?.bid,ask:store.tickers[0]?.ask};}
const rows=[];
for(const path of ['positions-full','positions-partial','positions-dispatch-full','positions-dispatch-partial','leverage','remove','bbo'])for(const [fail,dispose] of [[false,false],[true,false],[false,true]])for(const kind of ['Error','TypeError']){
 if(dispose&&!path.startsWith('positions'))continue;
 const store=new context.Store();store.update(structuredClone({positions:[position],tickers:[ticker],orders:[order]}),true);
 events=[];caught=[];const posts=[],error=kind==='TypeError'?new TypeError('subscriber failed'):new Error('subscriber failed');error.code=197;
 store.subscribe(()=>{events.push({listener:'first',state:view(store)});if(fail)throw error;if(dispose)exchange.isDisposed=true;});
 store.subscribe(()=>{events.push({listener:'second',state:view(store)});});
 const exchange={store,isDisposed:false,fetchPositions:async()=>[{...position,leverage:8}],removeOrders:new context.RemovalSlice(store).removeOrders};
 const ctx={postMessage(p){posts.push(p);}};const args=['a','bybit',exchange,ctx,{},{}];let thrown;
 try{
  if(path.startsWith('positions')){
   const handler=new context.UpdatePositionsHandler(...args),symbols=path.endsWith('partial')?['BTC']:undefined;
   if(path.includes('dispatch')){
    handler.handle({op:'updatePositions',data:{requestId:'r',symbols}});
    await new Promise(resolve=>setImmediate(resolve));
   }else await handler.handleUpdatePositions('r',symbols);
  }
  if(path==='leverage')new context.UiStateHandler(...args).updatePositionsWithLeverage('BTC',{buyLeverage:8,sellLeverage:9});
  if(path==='remove')await new context.CallMethodHandler(...args).handleCallMethod('r','removeOrders',[[{id:'o'}]]);
  if(path==='bbo')new context.OrderBookSubscriptionsHandler(...args)._sendOrderBookUpdate({symbol:'BTC'},{asks:[{price:102,amount:1}],bids:[{price:98,amount:1}]});
 }catch(e){thrown=e;}
 assert.equal(events.length,fail?1:2,'NOTIFICATION first throw skips later subscriber');
 assert.equal(events[0].listener,'first');assert.deepEqual(events[0].state,view(store),'NOTIFICATION mutation precedes subscriber');
 if(path==='remove')assert.deepEqual(view(store).orders,[],'NOTIFICATION removal committed');
 else if(path==='bbo'){assert.equal(view(store).bid,98);assert.equal(view(store).ask,102);}
 else assert.equal(view(store).leverage,8,'NOTIFICATION position mutation committed');
 if(['leverage','bbo'].includes(path))assert.equal(thrown,fail?error:undefined,'NOTIFICATION direct error identity');
 else{
  assert.equal(thrown,undefined);assert.equal(posts.length,1);assert.equal(posts[0].data.success,!fail,'NOTIFICATION handler reports failure after committed write');
  if(fail){
   assert.equal(caught[0],error,'NOTIFICATION caught error identity');
   if(path==='remove'){
    assert.equal(posts[0].data.result.name,kind,'NOTIFICATION caught error name');
    assert.equal(posts[0].data.result.message,error.message,'NOTIFICATION caught error message');
    assert.equal(posts[0].data.result.code,197,'NOTIFICATION caught error code');
   }else assert.equal(posts[0].data.reason,String(error),'NOTIFICATION refresh error string');
  }
 }
 if(path==='bbo')assert.equal(posts[0].op,'listenOrderBook','NOTIFICATION book post precedes ticker failure');
 assert.equal(exchange.isDisposed,dispose,'NOTIFICATION disposal occurred inside subscriber');
 rows.push({path,fail,kind,dispose,events,codeJson:JSON.stringify(error.code),posts:posts.map(p=>({op:p.op,success:p.data.success,result:p.data.result,reason:p.data.reason})),state:view(store),thrown:thrown?.message});
}
assert.equal(rows.length,36,'NOTIFICATION path inventory');
const leverageSymbols=[];
for(const [label,symbol,expectedUnits] of [
 ['lone',JSON.parse('"\\ud800"'),[0xd800]],
 ['replacement','\uFFFD',[0xfffd]],
 ['astral','\u{1F680}',[0xd83d,0xde80]],
 ['literal','\\ud800',[92,117,100,56,48,48]],
]){
 const units=s=>Array.from({length:s.length},(_,i)=>s.charCodeAt(i));
 assert.equal(typeof symbol,'string','LEVERAGE symbol is a materialized string');
 assert.deepEqual(units(symbol),expectedUnits,'LEVERAGE source input units');
 const store=new context.Store(),input={...position,symbol};
 const initialInput={...input};
 store.update({positions:[input]},true);
 const observed=[];
 store.subscribe(data=>observed.push({leverage:data.positions[0].leverage,units:units(data.positions[0].symbol)}));
 new context.UiStateHandler('a','bybit',{store},{postMessage(){}},{},{}).updatePositionsWithLeverage(symbol,{buyLeverage:8,sellLeverage:9});
 assert.equal(store.positions[0].leverage,8,'LEVERAGE source symbol match commits');
 assert.equal(typeof store.positions[0].symbol,'string');
 assert.deepEqual(units(store.positions[0].symbol),expectedUnits,'LEVERAGE source committed units');
 assert.equal(observed.length,1,'LEVERAGE source subscriber call count');
 assert.equal(observed[0].leverage,8);
 assert.deepEqual(observed[0].units,expectedUnits,'LEVERAGE source subscriber units');
 const symbolJson=JSON.stringify(symbol),positionJson=JSON.stringify(initialInput);
 leverageSymbols.push({label,symbolJson,positionJson,units:expectedUnits,committedLeverage:store.positions[0].leverage,observed});
}
assert.equal(leverageSymbols.length,4,'LEVERAGE source symbol witness floor');
const bboLeaf=[];
for(const scenario of ['numbers','post-error','book-mutation','metadata-alias']){
 const store=new context.Store();store.update({tickers:[{...ticker}]},true);
 const data={'42':'b','2':'a',symbol:'BTC',orderbook:null,extra:{x:1}},book={bids:[{price:Infinity,amount:-0,total:NaN}],asks:[{price:-Infinity,amount:NaN,total:-0}]};
 const failure=new TypeError('post failed');let calls=0,received,thrown;
 const ctx={postMessage(message){
  calls++;assert.equal(store.tickers[0].bid,99,'BBO source post precedes mutation');
  assert.equal(message.data.orderbook,book,'BBO source original book identity');
  assert.deepEqual(Object.keys(message.data),['2','42','symbol','orderbook','extra'],'BBO source own-key order');
  assert.equal(message.data.orderbook.bids[0].price,Infinity,'BBO source native Infinity');
  assert(Object.is(message.data.orderbook.bids[0].amount,-0),'BBO source signed zero');
  assert(Number.isNaN(message.data.orderbook.bids[0].total),'BBO source native NaN');
  assert.equal(message.data.orderbook.asks[0].price,-Infinity);
  assert(Number.isNaN(message.data.orderbook.asks[0].amount));
  assert(Object.is(message.data.orderbook.asks[0].total,-0));
  received=structuredClone(message);
  if(scenario==='post-error')throw failure;
  if(scenario==='book-mutation'){book.bids[0].price=101;book.asks[0].price=103;message.data.orderbook={bids:[],asks:[]};}
  if(scenario==='metadata-alias')message.data.extra.x=9;
 }};
 try{new context.OrderBookSubscriptionsHandler('a','bybit',{store},ctx,{},{})._sendOrderBookUpdate(data,book);}catch(error){thrown=error;}
 assert.equal(calls,1,'BBO source one post call');
 assert.equal(thrown,scenario==='post-error'?failure:undefined,'BBO source post error identity');
 assert.equal(data.extra.x,scenario==='metadata-alias'?9:1,'BBO source nested metadata alias');
 assert.equal(received.data.orderbook.bids[0].price,Infinity,'BBO source detached clone');
 assert(Object.is(received.data.orderbook.bids[0].amount,-0));
 const expectedBid=scenario==='post-error'?99:scenario==='book-mutation'?101:Infinity;
 const expectedAsk=scenario==='post-error'?101:scenario==='book-mutation'?103:-Infinity;
 assert.equal(store.tickers[0].bid,expectedBid,'BBO source post result drives BBO');
 assert.equal(store.tickers[0].ask,expectedAsk);
 bboLeaf.push({scenario,postCalls:calls,bid:String(expectedBid),ask:String(expectedAsk),extra:data.extra.x});
}
assert.equal(bboLeaf.length,4,'BBO source leaf floor');
const metadataData={symbol:'BTC',tag:JSON.parse('"\\ud800"')};let metadataCalls=0;
const metadataUnits=value=>Array.from({length:value.length},(_,index)=>value.charCodeAt(index));
assert.equal(typeof metadataData.tag,'string');assert.deepEqual(metadataUnits(metadataData.tag),[0xd800]);
new context.OrderBookSubscriptionsHandler('a','bybit',{store:new context.Store()},{postMessage(message){
 metadataCalls++;assert.equal(typeof message.data.tag,'string');
 assert.deepEqual(metadataUnits(message.data.tag),[0xd800],'BBO source materialized metadata units reach post');
}},{},{})._sendOrderBookUpdate(metadataData,{bids:[],asks:[]});
assert.equal(metadataCalls,1,'BBO source one metadata post');
const bboMetadata={inputJson:JSON.stringify(metadataData),units:[0xd800],postCalls:metadataCalls};
const bboNumericMetadata=[];
const numericCases=[['NaN',NaN],['Infinity',Infinity],['-Infinity',-Infinity],['-0',-0]];
for(const [kind,metric] of numericCases){let calls=0;
 new context.OrderBookSubscriptionsHandler('a','bybit',{store:new context.Store()},{postMessage(message){
  calls++;assert.equal(typeof message.data.metric,'number','BBO source metadata number type');
  assert(Object.is(message.data.metric,metric),'BBO source metadata binary64 before clone');
  const received=structuredClone(message);assert(Object.is(received.data.metric,metric),'BBO source metadata binary64 after clone');
 }},{},{})._sendOrderBookUpdate({symbol:'BTC',metric},{bids:[],asks:[]});
 assert.equal(calls,1,'BBO source metadata number post count');bboNumericMetadata.push({kind,calls});
}
assert.equal(bboNumericMetadata.length,4);
context.throttle=lodash.throttle;context.exchangeMetadata={bybit:{orderbookTickDelay:100}};
let parentCallback,parentPosts=0;
const parentExchange={name:'bybit',options:{},store:{markets:[{symbol:'BTC'}],tickers:[]},listenOrderBook(symbol,callback){assert.equal(symbol,'BTC');parentCallback=callback;return()=>{};}};
const parentHandler=new context.OrderBookSubscriptionsHandler('a','bybit',parentExchange,{postMessage(message){
 parentPosts++;assert.equal(typeof message.data.metric,'number');assert(Number.isNaN(message.data.metric),'BBO source parent preserves structured-clone NaN metadata');
}},{},{});
parentHandler.handle({op:'subscribeOrderBook',data:{symbol:'BTC',subId:'metadata',metric:NaN}});
parentCallback({bids:[],asks:[]});parentHandler.handle({op:'unsubscribeOrderBook',data:{subId:'metadata'}});
assert.equal(parentPosts,1,'BBO source parent metadata post count');
process.stdout.write(JSON.stringify({rows,leverageSymbols,bboLeaf,bboMetadata,bboNumericMetadata,bboParentMetadata:{kind:'NaN',posts:parentPosts}}));
