const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const {root,ts,load,capture}=require('./types-oracle.cjs');
const types=load(path.join(root,'packages/safe-cex/src/types.ts'));
const venue=load(path.join(root,'packages/safe-cex/src/exchanges/weex/weex.types.ts'));
const parser=load(path.join(root,'packages/json-parse/src/jsonParse.ts'));
const update=load(path.join(root,'packages/safe-cex/src/utils/updateOrderBook.ts'));
const totals=load(path.join(root,'packages/safe-cex/src/utils/orderbook.ts'));
const source=path.join(root,'packages/safe-cex/src/exchanges/weex/weex.ws-public.ts');
const tree=ts.createSourceFile(source,fs.readFileSync(source,'utf8'),ts.ScriptTarget.Latest,true);
const cls=tree.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text==='WeexPublicWebsocket');
assert.ok(cls,'public_feed::source_class');
assert.equal(cls.members.filter(n=>ts.isPropertyDeclaration(n)&&n.initializer&&ts.isArrowFunction(n.initializer)).length,20,'public_feed::source_arrow_fields');
const utilFile=path.join(root,'packages/safe-cex/src/exchanges/weex/weex.utils.ts');
const utilTree=ts.createSourceFile(utilFile,fs.readFileSync(utilFile,'utf8'),ts.ScriptTarget.Latest,true);
const utilities=utilTree.statements.filter(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>['toNumber','toIntegerString'].includes(d.name.getText(utilTree))));
assert.equal(utilities.length,2,'public_feed::source_utilities');
const compiled=ts.transpileModule(utilities.map(n=>n.getText(utilTree)).join('\n')+'\n'+cls.getText(tree),{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022},reportDiagnostics:true});
assert.equal(compiled.diagnostics.filter(d=>d.category===ts.DiagnosticCategory.Error).length,0);
const units=s=>Array.from({length:s.length},(_,i)=>s.charCodeAt(i));
function encode(v){if(v===undefined)return{kind:'undefined'};if(v===null)return{kind:'null'};
 if(typeof v==='number')return{kind:'number',bits:capture(v).bits};if(typeof v==='string')return{kind:'string',units:units(v)};
 if(typeof v==='boolean')return{kind:'boolean',value:v};if(typeof v==='function')return{kind:'function',name:v.label??v.name};
 if(v instanceof Set)return{kind:'set',items:Array.from(v,encode)};
 if(Array.isArray(v))return{kind:'array',items:Array.from({length:v.length},(_,i)=>i in v?encode(v[i]):{kind:'hole'})};
 return{kind:'object',fields:Object.entries(v).map(([k,x])=>[k,encode(x)])};}
const level=(price,amount)=>({price,amount,total:0});
const book={bids:[level(100,2),level(99,3)],asks:[level(101,4)],lastUpdateId:'10'};
const ticker={last:1,mark:2,index:3,percentage:4,volume:5,quoteVolume:6};
const cases=[];function add(id,method,patch={}){cases.push({id,input:{method,disposed:false,fail:'',testnet:false,data:'{}',json:{},symbol:'BTC',opts:{symbol:'BTC',interval:'1m'},book:structuredClone(book),snapshot:structuredClone({...book,lastUpdateId:'20'}),ticker:structuredClone(ticker),settle:true,unsubscribe:false,dispatch:false,holes:[],...patch}});}
add('constructor_defaults','constructor');add('constructor_parent_failure','constructor',{fail:'event:on'});
add('connect_basic','connect');add('connect_testnet','connect',{testnet:true});add('connect_disposed','connect',{disposed:true});add('connect_listener_failure','connect',{fail:'listener:message'});add('connect_creation_failure','connect',{fail:'create'});
add('message_disposed','message',{disposed:true,data:'garbage'});add('message_invalid','message',{data:'garbage'});add('message_null','message',{data:'null'});add('message_false','message',{data:'false'});
add('message_ping','message',{data:'{"event":"ping","e":"ticker"}'});add('message_ping_failure','message',{data:'{"event":"ping"}',fail:'sendPong'});
add('message_result_false','message',{data:'{"result":false,"msg":"denied"}'});add('message_result_fallback','message',{data:'{"result":false,"msg":0}'});add('message_result_null','message',{data:'{"result":null,"e":"trade"}'});
add('message_no_event','message');add('message_unknown','message',{data:'{"e":"unknown","s":"BTC"}'});
add('message_kline','message',{data:'{"e":"kline","s":"BTC","d":[{"i":"1"}]}'});add('message_kline_missing','message',{data:'{"e":"kline"}'});add('message_trade','message',{data:'{"e":"trade","s":"\\ud800"}'});
add('message_bigint','message',{data:'{"e":"trade","s":1234567890123456789}'});add('message_array_string','message',{data:['{"e":"trade","s":"BTC"}']});
add('message_ticker','message',{data:'{"e":"ticker","s":"BTC","d":{"c":"7","m":"8","i":"9","P":".25","v":"10","q":"11"}}'});
add('message_depth','message',{data:'{"e":"depth","s":"BTC","U":11,"u":12,"b":[[100,"4"]],"a":[]}'});
add('message_dynamic_ticker','message',{data:'{"e":"ticker","s":"BTC","d":{"c":"77","m":"88","i":"99"}}',override:'ticker'});add('message_dynamic_depth','message',{data:'{"e":"depth","s":"BTC"}',override:'depth'});
add('message_coercion_order','message',{json:{e:'kline',s:'BTC',d:[{i:'1'}]},customSymbol:true,replaceRoute:true});
add('message_data_get_failure','message',{disposed:true,fail:'data:get'});
add('candles_supported','candle',{dispatch:true,json:{d:[{normalized:{timestamp:1,close:2}},{normalized:null},{normalized:{timestamp:3,close:4}}]},unsubscribe:true});
add('candles_unsupported','candle',{opts:{symbol:'BTC',interval:'10m'}});add('candles_empty','candle',{dispatch:true,json:{d:null}});
add('candles_sparse','candle',{dispatch:true,json:{d:[{normalized:{close:1}},{normalized:{close:2}}]},holes:[0]});
add('candles_callback_failure','candle',{dispatch:true,json:{d:[{normalized:{close:1}}]},fail:'callback',customIterator:true});
add('candles_live_append','candle',{dispatch:true,json:{d:[{normalized:{close:1}}]},append:true});
add('candles_subscribe_failure','candle',{fail:'subscribe'});add('candles_unsubscribe_failure','candle',{unsubscribe:true,fail:'unsubscribe'});
add('book_snapshot','book',{dispatch:true,unsubscribe:true});add('book_disposed_after_fetch','book',{disposeAfterStart:true});add('book_unsubscribe_before_fetch','book',{unsubscribeBefore:true});
add('book_fetch_rejection','book',{fail:'fetch:reject'});add('book_sync_fetch_throw','book',{fail:'fetch'});add('book_callback_rejection','book',{fail:'callback'});add('book_subscribe_rejection','book',{fail:'subscribe'});
add('book_missing','book',{settle:false,dispatch:true,book:null});add('book_failure_coercion_order','book',{fail:'fetch:reject',customSymbol:true});
add('dispose_basic','dispose');add('dispose_off_failure','dispose',{fail:'event:off'});
add('ticker_object','ticker',{json:{s:'BTC',d:{c:'7tail',m:'8',i:'9',P:'.25',v:'10',q:'11'}}});add('ticker_array','ticker',{json:{s:'BTC',d:[{c:'12',P:'-.5'}]}});
add('ticker_fallback','ticker',{json:{s:'BTC',d:{c:null,m:'Infinity',i:'',P:'bad',v:false,q:{}}}});
add('ticker_percentage_boundary','ticker',{json:{s:'BTC',d:{P:'1'}}});add('ticker_percentage_above','ticker',{json:{s:'BTC',d:{P:'1.01'}}});add('ticker_percentage_string_fallback','ticker',{ticker:{...ticker,percentage:'2'},json:{s:'BTC',d:{}}});
add('ticker_signed_zero','ticker',{json:{s:'BTC',d:{P:'-0',c:'-0'}}});add('ticker_missing','ticker',{ticker:null,json:{s:'BTC',d:{}}});add('ticker_empty_array','ticker',{json:{s:'BTC',d:[]}});add('ticker_update_failure','ticker',{json:{s:'BTC',d:{}},fail:'updateTicker'});
add('depth_delta','depth',{json:{s:'BTC',U:11,u:12,b:[[100,'5'],[98,'2'],[99,0]],a:[[102,'3']]}});
add('depth_missing','depth',{book:null,json:{s:'BTC',U:11,u:12}});add('depth_no_previous','depth',{book:{...book,lastUpdateId:'0'},json:{s:'BTC',U:99,u:100,b:[],a:[]}});
add('depth_boundary','depth',{json:{s:'BTC',U:11,u:12}});add('depth_gap','depth',{json:{s:'BTC',U:12,u:14,b:[[100,99]]}});
add('depth_gap_disposed','depth',{json:{s:'BTC',U:12,u:14},disposeAfterStart:true});add('depth_gap_reject','depth',{json:{s:'BTC',U:12,u:14},fail:'fetch:reject'});
add('depth_gap_route_reject','depth',{json:{s:'BTC',U:12,u:14},fail:'route'});
add('depth_zero_id_fallback','depth',{json:{s:'BTC',U:0,u:'bad',b:[],a:[]}});add('depth_invalid_price','depth',{json:{s:'BTC',U:11,u:12,b:[['bad','2']],a:[]}});
add('depth_update_failure_close','depth',{json:{s:'BTC',U:11,u:12,b:[[100,2]],a:[]},fail:'updatePrice:bids',customIterator:true});
add('depth_pair_error_close','depth',{json:{s:'BTC',U:11,u:12,b:[null],a:[]},customIterator:true});
add('depth_live_append','depth',{json:{s:'BTC',U:11,u:12,b:[[100,5]],a:[]},append:true});
async function run(c){const input=structuredClone(c.input),events=[],parsed=[],parseArgs=[],warnings=[],callbacks=[],routes=[],stats=[],created=[],listeners=[],subscriptions=[],handlerOps=[],tickerUpdates=[],priceUpdates=[],parentEvents=[],reads=[],jobs=[];
 let disposed=input.disposed,instance,snapshotPromise,constructorValue=null,error=null,primary,callbackCount=0,symbolReads=0;
 const check=s=>{if(input.fail===s)throw new RangeError('fail:'+s);};const record=s=>{events.push(s);check(s);};
 function route(key,json){record('route');routes.push({key,json:structuredClone(json)});}route.label='route';
 function callback(value){record('callback');callbacks.push(structuredClone(value));if(input.append&&++callbackCount===1)input.json.d.push({normalized:{close:9}});}
 const symbol=input.customSymbol?{[Symbol.toPrimitive](hint){reads.push('symbol:'+hint);if(input.replaceRoute&&++symbolReads===2)instance.handleRoutedMessage=Object.assign(()=>{record('overrideRoute');},{label:'overrideRoute'});return input.symbol;}}:input.symbol;
 const parent={options:{testnet:input.testnet},on(event,handler){record('event:on');parentEvents.push({event,handler:handler.name});},off(event,handler){record('event:off');parentEvents.push({event,handler:handler.name});},log(message,severity){assert.equal(severity,types.LogSeverity.Warning);record('warning');warnings.push(message);},fetchOrderBook(s,limit){record('fetch');assert.strictEqual(s,symbol);assert.equal(limit,200);return snapshotPromise;}};
 class Base{constructor(p){record('baseConstruct');this.parent=p;this.onClose=Object.assign(()=>{},{label:'onClose'});}dispose(){record('baseDispose');disposed=true;}}
 const scope={exports:{},...types,...venue,Set,BaseWebSocket:Base,MAX_WS_CHANNELS:100,MAX_TICKER_CHANNELS:90,MAX_SUBSCRIPTION_OPERATIONS_PER_HOUR:240,SUBSCRIPTION_OPERATION_WINDOW_MS:3600000,
 bigIntAsStringJsonParse(s){record('parse');parseArgs.push(s);const value=input.customSymbol?input.json:parser.bigIntAsStringJsonParse(s);if(input.customSymbol){value.s=symbol;Object.defineProperty(value,'d',{get(){reads.push('d:get');return c.input.json.d;}});}parsed.push(encode(input.customSymbol?{...c.input.json,s:input.symbol}:value));return value;},
 normalizeCandle(row){record('normalizeCandle');return row?.normalized??null;},
 updateOrderBookByPrice(b,side,price,amount){record('updatePrice:'+side);priceUpdates.push({side,price,amount});update.updateOrderBookByPrice(b,side,price,amount);if(input.append&&priceUpdates.length===1)input.json.b.push([98,9]);},
 calcOrderBookTotal(b){record('calcTotals');totals.calcOrderBookTotal(b);}};
 vm.runInNewContext(compiled.outputText,scope);
 // The constructor's parent event is observed before the fixture overrides other methods.
 try{instance=new scope.exports.WeexPublicWebsocket(parent);if(input.method==='constructor')constructorValue=Object.fromEntries(Object.entries(instance).filter(([k])=>k!=='parent'&&k!=='onClose'));}catch(e){error={name:e.name,message:e.message};}
 if(instance&&input.method!=='constructor'){
 events.length=0;parentEvents.length=0;
 Object.defineProperty(instance,'isDisposed',{get(){record('disposed');return disposed;}});
 instance.orderbooks={};if(input.book)instance.orderbooks[input.symbol]=input.book;
 instance.store={tickersBySymbol:input.ticker?{[input.symbol]:input.ticker}:{},updateTicker(ticker,patch){record('updateTicker');tickerUpdates.push({ticker:structuredClone(ticker),patch:structuredClone(patch)});Object.assign(ticker,patch);}};
 instance.createWebsocket=(url,protocols,options)=>{record('create');created.push({url,protocols,options});return{addEventListener(event,handler){record('listener:'+event);listeners.push({event,handler:handler.name||handler.label});}};};instance.trackLastReceived=()=>record('track');
 instance.send=s=>{record('sendPong');primary=JSON.parse(s);};instance.handlePongEvent=delay=>{record('pong');reads.push('pong:'+delay);};instance.recordMessageHandlerStats=topic=>{record('stats');stats.push(topic);};
 instance.handleRoutedMessage=route;
 if(input.override==='ticker')instance.handleTicker=()=>record('overrideTicker');if(input.override==='depth')instance.handleDepth=()=>record('overrideDepth');
 instance.addHandler=(key,handler)=>{record('addHandler');handlerOps.push({op:'add',key});instance.messageHandlers[key]=handler;};
 instance.removeHandler=(key,handler)=>{record('removeHandler');handlerOps.push({op:'remove',key});assert.strictEqual(instance.messageHandlers[key],handler);delete instance.messageHandlers[key];};
 instance.subscribeTopic=topic=>{record('subscribe');subscriptions.push({op:'subscribe',topic});};instance.unsubscribeTopic=topic=>{record('unsubscribe');subscriptions.push({op:'unsubscribe',topic});};
 snapshotPromise=new Promise((resolve,reject)=>jobs.push(()=>{if(input.fail==='fetch:reject'){const e=new RangeError('fail:fetch:reject');Object.defineProperty(e,'message',{get(){reads.push('error:message');return 'fail:fetch:reject';}});reject(e);}else resolve(input.snapshot);}));
 for(const index of input.holes)delete input.json.d[index];
 if(input.customIterator){
  const list=input.method==='candle'?input.json.d:input.json.b;
  Object.defineProperty(list,Symbol.iterator,{value(){
   let index=0;
   return {next(){return index<list.length?{value:list[index++],done:false}:{done:true};},return(){record('iteratorClose');return {done:true};}};
  }});
 }
 let disposer;
 try{switch(input.method){case'connect':instance.connectAndSubscribe();break;case'message':instance.onMessage({get data(){record('data:get');return input.data;}});break;
 case'candle':disposer=instance.listenOHLCV(input.opts,callback);if(input.dispatch)instance.messageHandlers[`kline.${input.opts.symbol}.${venue.INTERVAL[input.opts.interval]}`](input.json);break;
 case'book':disposer=instance.listenOrderBook(symbol,callback);break;case'ticker':instance.handleTicker(input.json);break;case'depth':instance.handleDepth(input.json);break;case'dispose':instance.dispose();break;}
 if(input.unsubscribeBefore)disposer();if(input.disposeAfterStart)disposed=true;
 if(input.settle){for(const job of jobs)job();await new Promise(resolve=>setImmediate(resolve));}
 if(input.method==='book'&&input.dispatch)instance.messageHandlers['depth.'+input.symbol]?.();if(input.unsubscribe)disposer();
 }catch(e){error={name:e.name,message:e.message};}
 }
 const native={constructorValue,error,disposed,id:instance?.subscriptionId,created,listeners,warnings,callbacks,routes,stats,subscriptions,handlerOps,tickerUpdates,priceUpdates,parentEvents,reads,parseArgs,primary,book:instance?.orderbooks?.[input.symbol],ticker:instance?.store?.tickersBySymbol?.[input.symbol]};
 if(input.method==='constructor'){native.book=undefined;native.ticker=undefined;}
 return{id:c.id,input:c.input,parsed,native:encode(native),events};}
(async()=>{const output=[];for(const c of cases)output.push(await run(c));assert.equal(output.length,71,'public_feed::fixture_floor');process.stdout.write(JSON.stringify(output));})().catch(e=>{console.error(e);process.exitCode=1;});
