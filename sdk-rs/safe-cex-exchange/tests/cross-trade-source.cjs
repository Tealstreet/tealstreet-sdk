const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const {root,ts,dependencies}=require('../../safe-cex-types/scripts/accounts-source.cjs');
const fixtures=require('./cross-trade-fixtures.cjs');
let clock=1000,schedules=[],consoleEvents=[];
const cache=new Map();
const fakeConsole={log(){},error:(...args)=>consoleEvents.push(args)};
class ClockDate extends Date {static now(){return clock;}}
function load(file){
 file=path.normalize(file);if(cache.has(file))return cache.get(file);
 const exports={};cache.set(file,exports);
 const source=fs.readFileSync(path.join(root,file),'utf8');
 const js=ts.transpileModule(source,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS,esModuleInterop:true}}).outputText;
 new Function('exports','require','Date','console',js)(exports,name=>{
  if(name.endsWith('/utils/loop')||name==='../../utils/loop')return{loop:(fn,ms)=>{schedules.push({fn,ms});return Promise.resolve();}};
  if(name.startsWith('.'))return load(path.join(path.dirname(file),name+'.ts'));
  if(name.startsWith('@tealstreet/'))return load('packages/'+name.slice('@tealstreet/'.length)+'.ts');
  return dependencies(name);
 },ClockDate,fakeConsole);return exports;
}
const dir='packages/safe-cex/src/crossTrade/';
const defaults=load(dir+'types.ts'),forward=load(dir+'getCopierSymbolFromMaster.ts').getCopierSymbolFromMaster,reverse=load(dir+'getMasterSymbolFromCopier.ts').getMasterSymbolFromCopier,weightRatio=load(dir+'getCopierWeightRatio.ts').getCopierWeightRatio;
const {createCrossTradePlaceOrder}=load(dir+'runtime/crossTradePlaceOrder.ts'),{CrossTradeManager}=load(dir+'runtime/CrossTradeManager.ts');
const num=value=>value==='undefined'?undefined:value==='NaN'?NaN:value==='Infinity'?Infinity:value==='-0'?-0:Number(value);
const tag=value=>value===undefined?'undefined':Object.is(value,-0)?'-0':String(value);
const exchange=store=>({name:'bybit',store:{...store,loaded:{positions:store.positionsLoaded}}});
const request=(args)=>({symbol:args[1].symbol,reference:tag(args[2]),account:args[3],side:args[4],amount:tag(args[5]),reduceOnly:args[6]});
(async()=>{
 assert.equal(defaults.defaultCrossTradeCopier.enabled,false);assert.equal(defaults.defaultCrossTradeCopier.account,undefined);assert.equal(defaults.defaultCrossTradeCopier.weight,1);assert.equal(defaults.defaultCrossTradeCopier.execution,'LimitSlippage');assert.equal(defaults.defaultCrossTradeCopier.weightMode,'Absolute');assert.deepEqual(defaults.defaultCrossTradeCopier.symbolMappings,{});assert.equal(defaults.defaultCrossTradeConfig.enabled,false);assert.equal(defaults.defaultCrossTradeConfig.masterAccount,undefined);assert.equal(defaults.defaultCrossTradeConfig.masterSymbol,undefined);assert.equal(defaults.defaultCrossTradeConfig.masterWeight,1);assert.deepEqual(defaults.defaultCrossTradeConfig.copiers,[]);
 assert.ok(Object.isFrozen(defaults.defaultCrossTradeCopier));assert.ok(Object.isFrozen(defaults.defaultCrossTradeConfig));assert.equal(Object.isFrozen(defaults.defaultCrossTradeCopier.symbolMappings),false);assert.equal(Object.isFrozen(defaults.defaultCrossTradeConfig.copiers),false);
 assert.equal(forward({copier:fixtures.copier({symbolMappings:{constructor:''}}),masterExchangeName:'bybit',masterSymbol:'constructor'}),'constructor','source own empty shadow');
 assert.equal(forward({copier:fixtures.copier({symbolMappings:{constructor:'COPY'}}),masterExchangeName:'bybit',masterSymbol:'constructor'}),'COPY','source own nonempty shadow');
 assert.equal(reverse({config:fixtures.config({masterSymbol:'FIXED'}),copier:fixtures.copier({symbolMappings:{BTCUSDT:'__proto__'}}),copierExchangeName:'bybit',copierSymbol:'__proto__'}),'FIXED','source fixed precedence');
 const modes=[];const source=ts.createSourceFile('types.ts',fs.readFileSync(root+'/'+dir+'types.ts','utf8'),ts.ScriptTarget.Latest,true);
 for(const statement of source.statements)if(ts.isTypeAliasDeclaration(statement)&&['CrossTradWeightMode','CrossTradeExecution'].includes(statement.name.text))modes.push({name:statement.name.text,values:statement.type.types.map(type=>type.literal.text)});
 assert.equal(modes[0].values.length,2);assert.equal(modes[1].values.length,3);
 const mappings=fixtures.mappings.map(input=>{const copier=fixtures.copier({symbolMappings:input.mappings});const config=fixtures.config({masterSymbol:input.fixed});const result=input.forward?forward({copier,masterExchangeName:'binance',copierExchangeName:input.noExchange?undefined:'hyperliquid',masterSymbol:input.symbol}):reverse({config,copier,copierExchangeName:'bybit',masterExchangeName:input.noExchange?undefined:'binance',copierSymbol:input.symbol});assert.equal(typeof result,'string');return {...input,result};});assert.equal(mappings.length,12);
 // Assert inherited identity before the JSON result projection drops it.
 assert.equal(typeof forward({copier:fixtures.copier(),masterExchangeName:'bybit',masterSymbol:'constructor'}),'function');
 assert.equal(reverse({config:fixtures.config(),copier:fixtures.copier({symbolMappings:{BTCUSDT:'__proto__'}}),copierExchangeName:'bybit',copierSymbol:'__proto__'}),Object.prototype);
 const weights=fixtures.weights.map(input=>{const result=weightRatio(fixtures.config({masterWeight:num(input.masterWeight)}),fixtures.copier({weight:num(input.weight),weightMode:input.mode}),exchange(input.master),exchange(input.copy),input.masterMarket,input.copierMarket);assert.ok(Number.isFinite(result)&&result>=0);return {...input,result:tag(result)};});assert.equal(weights.length,19);
 const placeCases=[];
 for(const input of fixtures.placeCases){
  let events=[];consoleEvents=[];const original=new Error('order refused'),replacement=new Error('warning refused'),sentinel={id:'native-sentinel'};
  const ex={store:{activeChasers:input.activeChasers},emitter:{emit:(kind,data)=>{if(kind==='warning'){events.push({kind:'warning',message:data});if(input.fail==='warning')throw replacement;}else if(kind==='chaser:terminate')events.push({kind:'terminate',id:data});else events.push({kind:'start',...data});}},placeOrder:order=>{events.push({kind:'order',symbol:order.symbol,side:order.side,amount:tag(order.amount),price:tag(order.price),reduceOnly:order.reduceOnly,type:order.type});return input.fail?Promise.reject(original):Promise.resolve(sentinel);}};
  const place=createCrossTradePlaceOrder({getExchange:()=>input.absent?undefined:ex});
  const pending=place(input.copier,input.market,num(input.reference),'c',input.side,num(input.amount),input.reduceOnly);
  const eagerEvents=events.length;let result,error;try{result=await pending;}catch(value){error=value;}
  if(error)assert.equal(error,input.fail==='warning'?replacement:original,'source preserves the original error unless warning replaces it');
  if(result)assert.equal(result,sentinel,'source preserves order result identity');
  if(input.label==='chase zero terminates')assert.deepEqual(events,[{kind:'terminate',id:'chaser'}]);
  placeCases.push({...input,events,eagerEvents,error:error?.message,returned:result===sentinel,errorLogs:consoleEvents.length});
 }assert.equal(placeCases.length,22);
 const audits=[];
 for(const input of fixtures.audits){const calls=[];const manager=new CrossTradeManager({getExchange:()=>undefined,placeOrder:(...args)=>{calls.push(request(args));return Promise.resolve();}});const pending=manager[input.kind==='copier'?'_auditCopierPositions':'_auditMasterPositions'](input.config,input.copier,exchange(input.master),exchange(input.copy));const eagerCalls=calls.length;if(input.expectedCalls!==undefined)assert.equal(eagerCalls,input.expectedCalls,'source stock prototype branch '+input.label);const result=await pending;assert.equal(result?.length??0,input.slots,'independent audit slot floor '+input.label);audits.push({...input,calls,eagerCalls});}assert.equal(audits.length,26);
 const calls=[];let fail=true;const manager=new CrossTradeManager({getExchange:()=>undefined,placeOrder:async()=>{calls.push('place');if(fail)throw new Error('failed');return undefined;}});
 const c=fixtures.copier(),m=fixtures.market();const states=[];
 async function step(now,amount,reduceOnly){clock=now;await manager._safePlaceOrder(c,m,100,'c','buy',amount,reduceOnly);states.push({calls:calls.length,entry:manager._failures.has('c:BTCUSDT')?{...manager._failures.get('c:BTCUSDT')}:null,muted:manager._isMuted('c','BTCUSDT')});}
 await step(1000,1,false);await step(1000,1,false);await step(30999,1,false);await step(31000,1,false);fail=false;await step(60999,1,false);await step(61000,1,false);await step(62000,0.001,false);await step(62000,0.001,true);
 assert.equal(states[2].calls,2);assert.equal(states[3].entry.failures,3);assert.equal(states[3].entry.mutedUntil,61000);assert.equal(states[5].entry,null);assert.equal(states[7].calls,5);
 manager._recordFailure('other','ETHUSDT');manager._recordFailure('c','BTCUSDT');manager._recordSuccess('c','BTCUSDT');assert.equal(manager._failures.get('other:ETHUSDT').failures,1);assert.equal(manager._failures.has('c:BTCUSDT'),false);manager._recordFailure('c','BTCUSDT');assert.equal(manager._failures.get('c:BTCUSDT').failures,1);
 let reject;const late=new CrossTradeManager({getExchange:()=>undefined,placeOrder:()=>new Promise((_,r)=>reject=r)});const pending=late._safePlaceOrder(c,m,100,'c','buy',1,false);late.stop();reject(new Error('late'));await pending;assert.equal(late._failures.get('c:BTCUSDT').failures,1);
 const lifecycle=[];schedules=[];const lookups=[],orders=[];const ms=exchange(fixtures.store([fixtures.position()])),cs=exchange(fixtures.store([]));const looping=new CrossTradeManager({getExchange:account=>{lookups.push(account);return account==='m'?ms:cs},placeOrder:async(...args)=>orders.push(request(args))});
 looping.start([fixtures.config({enabled:false,copiers:[fixtures.copier()]})]);assert.equal(orders.length,0);const first=looping._loop;assert.equal(schedules[0].ms,3000);await schedules.shift().fn();assert.equal(orders.length,1,'source ignores config.enabled');assert.equal(schedules.length,1);looping.start([]);assert.equal(first.isDisposed,true);const second=looping._loop;looping.stop();assert.equal(second.isDisposed,true);await first();await second();assert.equal(schedules.length,2);lifecycle.push({lookups,orders,scheduled:schedules.map(v=>v.ms),firstDisposed:first.isDisposed,secondDisposed:second.isDisposed});
 schedules=[];const parallelCalls=[];let resolveClose;
 const masterParallel=exchange(fixtures.store([fixtures.position()],{markets:[fixtures.market(),fixtures.market('ETHUSDT','ETH')]}));
 const copyParallel=exchange(fixtures.store([fixtures.position(1,'long','ETHUSDT')],{markets:[fixtures.market(),fixtures.market('ETHUSDT','ETH')],tickers:[fixtures.ticker(),fixtures.ticker('ETHUSDT')]}));
 const parallel=new CrossTradeManager({getExchange:account=>account==='m'?masterParallel:copyParallel,placeOrder:(...args)=>{parallelCalls.push(request(args));return args[3]==='c1'&&args[6]?new Promise(resolve=>resolveClose=resolve):Promise.resolve();}});
 parallel.start([fixtures.config({copiers:[fixtures.copier({account:'c1'}),fixtures.copier({account:'c2'})]})]);
 const parallelPending=schedules.shift().fn();const eager=parallelCalls.slice();assert.equal(eager.length,2,'source launches both copier audits before waiting');
 await new Promise(resolve=>setImmediate(resolve));assert.equal(parallelCalls.length,3,'source second copier begins master audit while first copier is pending');assert.equal(parallelCalls[2].account,'c2');assert.equal(parallelCalls[2].reduceOnly,false);
 parallel.stop();resolveClose();await parallelPending;assert.equal(parallelCalls.length,3,'source disposed guard suppresses first copier master audit');assert.equal(schedules.length,0,'source disposed guard suppresses reschedule');
 const concurrency={master:masterParallel.store,copy:copyParallel.store,eager,calls:parallelCalls};
 const reentrant=new CrossTradeManager({getExchange:()=>undefined,placeOrder:async()=>{}});reentrant.start([]);const savedLog=fakeConsole.log;let reentered=false;fakeConsole.log=(...args)=>{if(args[1]==='Stopping cross trade manager'&&!reentered){reentered=true;reentrant.start([]);}};reentrant.stop();fakeConsole.log=savedLog;assert.equal(reentrant._loop.isDisposed,true,'source stop rereads reentrant replacement loop');
 process.stdout.write(JSON.stringify({defaults,modes,mappings,weights,placeCases,audits,states,lifecycle,concurrency}));
})().catch(error=>{console.error(error);process.exitCode=1;});
