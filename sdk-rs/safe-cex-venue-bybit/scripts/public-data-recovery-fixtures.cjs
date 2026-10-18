const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH),root=path.resolve(__dirname,'../../..');
function ast(relative){const file=path.join(root,relative),text=fs.readFileSync(file,'utf8');return ts.createSourceFile(file,text,ts.ScriptTarget.Latest,true);}
function declarations(a,names){const ds=a.statements.filter(ts.isVariableStatement).flatMap(s=>Array.from(s.declarationList.declarations));return names.map(n=>{const hits=ds.filter(d=>d.name.getText(a)===n);assert.equal(hits.length,1,n);return 'const '+hits[0].getText(a)+';'}).join('\n');}
function methods(a,cls,names){const c=a.statements.find(s=>ts.isClassDeclaration(s)&&s.name.text===cls);assert(c);return names.map(n=>{const hits=c.members.filter(m=>m.name?.getText(a)===n);assert.equal(hits.length,1,n);return hits[0].getText(a);}).join('\n');}
const wa=ast('packages/safe-cex/src/exchanges/bybit/bybit.ws-public.ts'),ea=ast('packages/safe-cex/src/exchanges/bybit/bybit.exchange.ts'),ta=ast('packages/safe-cex/src/exchanges/bybit/bybit.types.ts');
const logAst=ast('packages/logging/src/LogSeverity.ts'); const enumText=logAst.statements.find(s=>ts.isEnumDeclaration(s)&&s.name.text==='LogSeverity').getText(logAst).replace(/^export\s+/,'');
const wsNames=['onOpen','clearOhlcvNoDataTimeout','armOhlcvNoDataTimeout','armExistingOhlcvNoDataTimeouts','maybeRecoverOhlcvStream','maybeRecoverOhlcvStreamFromTopics','onMarketsChanged'];
const exchangeNames=['fetchMarkets','fetchOptionMarkets','fetchFees','fetchTradingMarketsResponse'];
const dependencyRequire=require('node:module').createRequire(path.resolve(process.env.SAFE_CEX_TYPESCRIPT_PATH));
const {mapSeries}=dependencyRequire('p-iteration');
const getKey={exports:{}};vm.runInNewContext(ts.transpileModule(fs.readFileSync(path.join(root,'packages/safe-cex/src/utils/get-key.ts'),'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,{exports:getKey.exports,require:spec=>dependencyRequire(spec)});
const code=ts.transpileModule(`${enumText}\n${declarations(ta,['ENDPOINTS','INTERVAL'])}\n${declarations(wa,['OHLCV_NO_DATA_TIMEOUT_MS','OHLCV_RECOVERY_COOLDOWN_MS'])}\n${declarations(ea,['BYBIT_TRADING_MARKETS_RETRY_DELAY_MS'])}\nclass Ws {${methods(wa,'BybitPublicWebsocket',wsNames)}}\nclass Exchange {${methods(ea,'BybitExchange',exchangeNames)}}\nglobalThis.classes={Ws,Exchange}`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;
const decode=v=>v&&v.fixtureUndefined?undefined:Array.isArray(v)?v.map(decode):v&&typeof v==='object'?Object.fromEntries(Object.entries(v).map(([k,v])=>[k,decode(v)])):v;
const observe=v=>v===undefined?{undefined:true}:Array.isArray(v)?Array.from(v,observe):v&&typeof v==='object'?Object.fromEntries(Object.entries(v).map(([k,v])=>[k,observe(v)])):v;
async function run(input){const events=[],errors={},payloads=[],frames=[];let tickerReadValue;let api,callbacks=[],timerIndex=0,getIndex=0,handlerIndex=0;const event=(name,...args)=>{events.push([name,...args]);if(input.fail===name)throw errors[name]??=(Object.assign(new Error(name),{origin:name}));};
 let topics0={},topics1={},timers0={k:decode(Object.hasOwn(input,'timer')?input.timer:7)},timers1={k:'replacement'},last={k:Number(input.last??0)},handlers={k:[1],s:[1],...Object.fromEntries(Object.entries(input.handlers??{}).map(([k,n])=>[k,Array(n).fill(1)]))};
 const timerProxy=(obj,id)=>new Proxy(obj,{set(o,k,v){event('timerPut',id,k,observe(v));o[k]=v;return true},deleteProperty(o,k){event('delete',id,k);return delete o[k]}});
 const topicProxy=(obj,id)=>new Proxy(obj,{set(o,k,v){event('topicPut',id,k,observe(v));o[k]=v;return true}});
 const t0=topicProxy(topics0,0),t1=topicProxy(topics1,1),m0=timerProxy(timers0,0),m1=timerProxy(timers1,1);
 const oldTickers=decode(input.tickers??['old']);topics0.tickers=oldTickers;
 const added=(input.markets??[{symbol:'BTC',quote:'USDT'},{symbol:'ETH',quote:'USDC'},{symbol:'INV',quote:'BTC'}]).map(m=>({...m}));
 const feeData=decode(Object.hasOwn(input,'data')?input.data:{result:{list:[{symbol:'BTC',rate:'0.001'}]}}),cache=decode(input.cache??[{symbol:'CACHE'}]),replacement=[{symbol:'REPLACED'}],reply={data:{result:{list:[{symbol:'REPLY'}]}}};let store=cache;
 const context=vm.createContext({ENABLE_RECONNECTING_WEBSOCKET:!!input.reconnecting,chunk:(a,n)=>Array.from({length:Math.ceil(a.length/n)},(_,i)=>a.slice(i*n,(i+1)*n)),sleep:n=>{event('sleep',n);return input.failAsync==='sleep'?Promise.reject(errors.sleep??=Object.assign(new Error('sleep'),{origin:'sleep'})):Promise.resolve()},Date:{now:()=>{event('clock');return Number(input.now??10000)}},clearTimeout:id=>event('clear',observe(id)),setTimeout:(cb,delay)=>{event('set',delay);callbacks.push(cb);if(input.swapTimer)api.ohlcvNoDataTimeouts=m1;return 't'+timerIndex++},isRetryableBybitTradingMarketsFailure:e=>{event('retryable',e.origin);return !!input.retryable},describeBybitMarketsFailure:e=>{event('describe',e.origin);return 'why'},v:getKey.exports.v,mapSeries,mapBybitOptionInstrument:row=>{event('mapOption',observe(row));if(input.failMap&&row?.symbol===input.failMap)throw Object.assign(new Error('mapOption'),{origin:'mapOption'});return row?.status==='Trading'?{symbol:row.symbol,option:{strike:row.strike}}:null},BybitTradingMarketsRequestError:class Wrapped extends Error{constructor(cause,retry){super('wrapped');this.origin='wrapped';this.cause=cause;this.retry=retry;event('wrap',cause.origin,retry??null)}},});
 vm.runInContext(code,context);api=new (input.op.startsWith('fetch')?context.classes.Exchange:context.classes.Ws)();api.topics=t0;api.ohlcvNoDataTimeouts=m0;api.lastOhlcvRecoveryAtByTopic=last;api.messageHandlers=handlers;api.ws=input.ws!==false;api.isDisposed=!!input.disposed;api.parent={store:{markets:added},log:(m,s)=>event('log',m,s)};
 Object.defineProperty(api,'category',{get(){event('category');if(input.swapTopics)api.topics=t1;if(input.mutateMarket){added[0].symbol='MUTATED';added[0].quote='USDT';}return input.category??'linear'}});
 const emitter=id=>({off:(name,h)=>event('off',id,name,h),on:(name,h)=>event('on',id,name,h)});api.parent.emitter=emitter(0);
 const originalChanged=api.onMarketsChanged;Object.defineProperty(api,'onMarketsChanged',{get(){event('handler');if(input.swapEmitter)api.parent.emitter=emitter(++handlerIndex);return 'handler'+handlerIndex}});
 api.subscribe=()=>{event('subscribe');return {ignored:true}};api.ping=()=>event('ping');api.armExistingOhlcvNoDataTimeoutsHost=()=>event('existing');api.emitWsReconnected=()=>event('reconnected');
 Object.defineProperty(api,'isConnected',{get(){event('connected');return input.connected!==false}});api.reconnect=()=>event('reconnect');api.sendOhlcvDiagnosticPayload=p=>{event('diagnostic',p.op,p.args.slice());payloads.push(p);if(input.mutateArgs&&payloads.length===1)p.args.push('MUTATED')};api.send=frame=>{const decoded=JSON.parse(frame);event('send',decoded.op,decoded.args);frames.push(frame);if(input.disposeOnSend)api.isDisposed=true};
 api.log=(m,s)=>{event('log',m,s);if(input.replaceCache)store=replacement};api.store={};Object.defineProperty(api.store,'markets',{get(){event('stored');return store}});
 const request=asyncResult=>{if(input.failAsync===asyncResult)throw errors[asyncResult]??=(Object.assign(new Error(asyncResult),{origin:asyncResult}));return reply};
 api.xhr={get:(endpoint,{params}={})=>{const name='get'+getIndex++;event(name,endpoint,params);if(input.failAsync===name)return Promise.reject(errors[name]??=(Object.assign(new Error(name),{origin:name})));if(input.op==='fetchFees')return Promise.resolve({data:feeData});if(input.optionPages)return Promise.resolve({data:decode(input.optionPages[getIndex-1])});return Promise.resolve(reply)}};
 api.inverseEnabled=()=>{event('inverseEnabled');return !!input.inverse};api.optionsEnabled=()=>{event('optionsEnabled');return !!input.options};if(input.op==='fetchMarkets')api.fetchOptionMarkets=()=>{event('optionMarkets');if(input.failAsync==='optionMarkets')return Promise.reject(errors.optionMarkets??=Object.assign(new Error('optionMarkets'),{origin:'optionMarkets'}));return Promise.resolve([{symbol:'OPT'}])};
 api.fetchMarketsHelper=category=>{event('helper',category);if(input.brandError||input.failAsync==='helper'){const err=input.brandError?new context.BybitTradingMarketsRequestError({origin:'cause'}):Object.assign(new Error('helper'),{origin:'helper'});if(input.brandError)events.pop();if(input.syncBrandThrow)throw err;return Promise.reject(err)}return Promise.resolve(input.arrays?[{symbol:category}]:reply)};
 let result,before,error;try{let value;
 if(input.op==='onOpen'){api.armExistingOhlcvNoDataTimeouts=api.armExistingOhlcvNoDataTimeoutsHost;value=api.onOpen()}
 else if(input.op==='clear')value=api.clearOhlcvNoDataTimeout('k');
 else if(input.op==='arm'){api.clearOhlcvNoDataTimeout=t=>event('clearMethod',t);api.maybeRecoverOhlcvStreamFromTopics=(...a)=>event('recover',...a.map(x=>x??null));api.armOhlcvNoDataTimeoutHost=(...a)=>event('rearm',...a.map(x=>x??null));value=api.armOhlcvNoDataTimeout('k',input.secondary===false?undefined:'s');if(input.fire){api.armOhlcvNoDataTimeout=api.armOhlcvNoDataTimeoutHost;if(input.emptyOnFire)handlers.k=[];if(input.disposeOnFire)api.isDisposed=true;callbacks[0]?.();}}
 else if(input.op==='existing'){api.armOhlcvNoDataTimeout=(...a)=>{event('arm',...a);if(input.removeOnArm)handlers['kline.5.NEXT']=[]};value=api.armExistingOhlcvNoDataTimeouts()}
 else if(input.op==='wrapper'){const opts={symbol:'BTC',interval:input.interval??'1m'};Object.defineProperty(api,'maybeRecoverOhlcvStreamFromTopics',{get(){event('recoverGet');if(input.mutateSymbol)opts.symbol='CHANGED';return (...a)=>event('recover',...a)}});value=input.reason===null?api.maybeRecoverOhlcvStream(opts):api.maybeRecoverOhlcvStream(opts,input.reason)}
 else if(input.op==='recovery')value=api.maybeRecoverOhlcvStreamFromTopics('k',input.secondary===false?undefined:'s',input.reason??'reason');
 else if(input.op==='markets'){if(input.swapTickerRead)Object.defineProperty(topics0,'tickers',{get(){event('tickers');api.topics=t1;return tickerReadValue??oldTickers},set(v){tickerReadValue=v},configurable:true});value=originalChanged({added});}
 else if(input.op==='fetchFees')value=api.fetchFees(input.category??'linear');
 else if(input.op==='fetchMarkets')value=api.fetchMarkets();
 else if(input.op==='fetchOptionMarkets')value=api.fetchOptionMarkets();
 else if(input.op==='fetchRetry')value=api.fetchTradingMarketsResponse(input.category??'linear');
 else throw Error(input.op);
 before=events.slice();result=await value;
 }catch(e){error={origin:e.origin??null,cause:e.cause?.origin??null,retry:e.retry??null};}
 if(input.swapTickerRead)Object.defineProperty(topics0,'tickers',{value:tickerReadValue??oldTickers,configurable:true});
 return {input,events,before:before??events.slice(),result:observe(result),error:error??null,sameReply:result===reply,sameCache:result===cache,sameReplacement:result===replacement,sameFee:result===feeData?.result?.list,topics:[observe(topics0),observe(topics1)],timers:[observe(timers0),observe(timers1)],last:observe(last),payloadShared:payloads.length===2?payloads[0].args===payloads[1].args:null,payloadDistinct:payloads.length===2?payloads[0]!==payloads[1]:null,tickerFresh:topics0.tickers!==oldTickers,frames};
}
const cases=[
 ...[{}, {disposed:true}, {category:'inverse'}, {reconnecting:true}, {swapTopics:true,swapEmitter:true}, {mutateMarket:true}, {fail:'subscribe'}, {fail:'off'}].map(c=>({op:'onOpen',...c})),
 ...[{}, {timer:0},{timer:null},{timer:false},{timer:''},{timer:{fixtureUndefined:true}},{fail:'clear'},{fail:'delete'}].map(c=>({op:'clear',...c})),
 ...[{}, {handlers:{k:0}},{fire:true},{fire:true,disposeOnFire:true},{fire:true,emptyOnFire:true},{fire:true,secondary:false},{swapTimer:true,fire:true},{fire:true,fail:'recover'},{fail:'set'}].map(c=>({op:'arm',...c})),
 ...[{}, {removeOnArm:true},{fail:'arm'}].map(c=>({op:'existing',handlers:{k:0,s:0,'kline.1.BTC':1,'kline.5.NEXT':1,'kline.1.A.B':1,'kline.1.EMPTY':0},...c})),
 ...[{reason:null},{reason:''},{reason:'custom',interval:'1M'},{reason:null,interval:'bad'},{reason:'x',mutateSymbol:true}].map(c=>({op:'wrapper',...c})),
 ...[{}, {now:9999},{now:10000,last:1},{now:10001,last:1},{last:'NaN'},{handlers:{k:0}},{handlers:{s:0}},{secondary:false},{connected:false},{mutateArgs:true},{fail:'diagnostic'},{fail:'log'}].map(c=>({op:'recovery',...c})),
 ...[{}, {markets:[]},{ws:false},{disposed:true},{category:'inverse'},{swapTickerRead:true},{tickers:'A😀'},{swapTopics:true},{fail:'send'},{failAsync:'sleep'},{disposeOnSend:true,markets:Array.from({length:21},(_,i)=>({symbol:'M'+i,quote:'USDT'}))}].map(c=>({op:'markets',...c})),
 ...[{}, {data:null},{data:{result:{list:false}}},{data:{result:{list:[]}}},{data:{result:{list:'non-array'}}},{fail:'get0'},{failAsync:'get0'}].map(c=>({op:'fetchFees',...c})),
 ...[{}, {brandError:true},{brandError:true,cache:[]},{brandError:true,replaceCache:true},{failAsync:'helper'},{fail:'helper'},{brandError:true,fail:'log'},{brandError:true,syncBrandThrow:true},{arrays:true,inverse:true},{arrays:true,options:true},{arrays:true,inverse:true,options:true},{inverse:true,arrays:true,failAsync:'helper'},{arrays:true,options:true,failAsync:'optionMarkets'},{arrays:true,inverse:true,fail:'inverseEnabled'}].map(c=>({op:'fetchMarkets',...c})),
 ...[
  {optionPages:[{retCode:0,result:{list:[{baseCoin:'BTC'},{baseCoin:'ETH'}]}},{retCode:'0',result:{list:[{symbol:'B1',status:'Trading',strike:1},{symbol:'B2',status:'Closed'}],nextPageCursor:'p2'}},{retCode:0,result:{list:[{symbol:'B3',status:'Trading',strike:3}]}},{ret_code:0,result:{list:[{symbol:'E1',status:'Trading',strike:5}],nextPageCursor:''}}]},
  {optionPages:[{retCode:0,result:{}}]},
  {optionPages:[{retCode:0}]},
  {optionPages:[{retCode:10001,retMsg:'denied'}],cache:[{symbol:'L'},{symbol:'O',option:{strike:1}},{symbol:'Z',option:null}]},
  {optionPages:[{retCode:'x'}],cache:[{symbol:'O',option:{strike:1}}]},
  {optionPages:[{retCode:null,result:{list:[{baseCoin:'BTC'}]}},{retCode:1,retMsg:7}]},
  {optionPages:[{retCode:0,result:{list:[{baseCoin:'BTC'}]}},{retCode:0,result:{list:[{symbol:'B1',status:'Trading',strike:1}]}}],failMap:'B1',cache:[{symbol:'O',option:{strike:1}}]},
  {optionPages:[{retCode:0,result:{list:[null]}}]},
  {optionPages:[{retCode:0,result:{list:[{baseCoin:'BTC'}]}}],failAsync:'get1',cache:[{symbol:'O',option:{strike:1}}]},
  {optionPages:[{retCode:0,result:{list:[{baseCoin:'BTC'}]}},{retCode:0,result:{list:[{symbol:'B1',status:'Trading',strike:1}],nextPageCursor:null}}]},
  {optionPages:[{retCode:0,result:{list:[{baseCoin:'BTC'}]}}],failAsync:'get1',fail:'log'},
 ].map(c=>({op:'fetchOptionMarkets',...c})),
 ...[{}, {fail:'get0'},{failAsync:'get0'},{failAsync:'get0',retryable:true},{failAsync:'get0',retryable:true,fail:'get1'},{failAsync:'get0',retryable:true,fail:'describe'},{failAsync:'get0',retryable:true,fail:'log'},{failAsync:'get0',retryable:true,fail:'sleep'},{failAsync:'get0',retryable:true,category:'inverse'},{fail:'get0',retryable:true,failAsync:'get1'}].map(c=>({op:'fetchRetry',...c})),
];
assert.equal(cases.length,98,'RECOVERY independent scenario floor');
(async()=>{const rows=[];for(const input of cases)rows.push(await run(input));process.stdout.write(JSON.stringify(rows))})().catch(e=>{console.error(e);process.exitCode=1});
