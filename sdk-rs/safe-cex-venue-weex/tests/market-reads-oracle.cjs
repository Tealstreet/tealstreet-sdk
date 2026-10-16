const assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const {root, ts, load, capture} = require('./types-oracle.cjs');
const types = load(path.join(root, 'packages/safe-cex/src/types.ts'));
const venue = load(path.join(root, 'packages/safe-cex/src/exchanges/weex/weex.types.ts'));
const {LogSeverity} = load(path.join(root, 'packages/logging/src/LogSeverity.ts'));
const names = ['fetchMarkets','fetchTickers','fetchOHLCV','listenOHLCV','listenOrderBook','fetchOrderBook','fetchPublicTrades','fetchHistoricalFills','fetchUserTradeRows'];
const file = path.join(root, 'packages/safe-cex/src/exchanges/weex/weex.exchange.ts');
const tree = ts.createSourceFile(file, fs.readFileSync(file,'utf8'), ts.ScriptTarget.Latest, true);
const cls = tree.statements.find(n => ts.isClassDeclaration(n) && n.name?.text === 'WeexExchange');
const fields = cls.members.filter(n => ts.isPropertyDeclaration(n) && names.includes(n.name.getText(tree)));
assert.equal(fields.length, 9, 'market_reads::source_fields');
function declaration(tree, name) {
  const statement = tree.statements.find(n => ts.isVariableStatement(n) && n.declarationList.declarations.some(d => d.name.getText(tree) === name));
  assert.ok(statement, 'market_reads::source_'+name);return statement.getText(tree);
}
const utilsFile = path.join(root,'packages/safe-cex/src/exchanges/weex/weex.utils.ts');
const utils = ts.createSourceFile(utilsFile,fs.readFileSync(utilsFile,'utf8'),ts.ScriptTarget.Latest,true);
const omitFile=path.join(root,'packages/safe-cex/src/utils/omitUndefined.ts');
const omitTree=ts.createSourceFile(omitFile,fs.readFileSync(omitFile,'utf8'),ts.ScriptTarget.Latest,true);
const compiled = ts.transpileModule(declaration(omitTree,'omitUndefined')+'\n'+declaration(tree,'extractArray')+'\n'+declaration(utils,'toIntegerString')+'\n'+declaration(utils,'normalizeTimestampToMs')+'\nexport class Slice {'+fields.map(n=>n.getText(tree)).join('\n')+'}',
 {compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022},reportDiagnostics:true});
assert.equal(compiled.diagnostics.filter(d=>d.category===ts.DiagnosticCategory.Error).length,0);
const scope = {exports:{}, ...types, ...venue, LogSeverity, uniqBy:require('lodash/uniqBy'),
 omitBy:require('lodash/omitBy')};
const context=vm.createContext(scope);
vm.runInContext(compiled.outputText,context);
const units = s=>Array.from({length:s.length},(_,i)=>s.charCodeAt(i));
const text = u=>String.fromCharCode(...u);
function encode(v) {
 if(v===undefined)return {kind:'undefined'};if(v===null)return {kind:'null'};
 if(typeof v==='number')return {kind:'number',bits:capture(v).bits};
 if(typeof v==='string')return {kind:'string',units:units(v)};
 if(typeof v==='boolean')return {kind:'bool',value:v};
 if(typeof v==='symbol')return {kind:'symbol',units:units(v.description??'')};
 if(v instanceof Map)return {kind:'array',items:Array.from(v,([key,value])=>encode([key,value]))};
 if(Array.isArray(v))return {kind:'array',items:Array.from({length:v.length},(_,i)=>i in v?encode(v[i]):{kind:'hole'})};
 return {kind:'object',fields:Object.entries(v).map(([key,value])=>[key,encode(value)])};
}
function decode(v) {
 switch(v.kind){case 'undefined':return undefined;case 'null':return null;case 'bool':return v.value;case 'number':return Buffer.from(v.bits,'hex').readDoubleBE();case 'string':return text(v.units);
 case 'array':{const a=new Array(v.items.length);v.items.forEach((x,i)=>{if(x.kind!=='hole')a[i]=decode(x);});return a;}
 case 'symbol':return Symbol(text(v.units));case 'object':return Object.fromEntries(v.fields.map(([k,x])=>[k,decode(x)]));default:throw Error(v.kind);}
}
const market=(symbol='BTC',quoteAsset='USDT',marginAsset='USDT')=>({symbol,quoteAsset,marginAsset,native:{symbol,marker:'market:'+symbol}});
const tick=(symbol='BTC',marker='ticker')=>({symbol,native:marker});
const candle=(timestamp=2,id='C')=>({native:{id,timestamp}});
const fill=(orderId='1',time=100,symbol='BTC',id='F')=>({orderId,time,symbol,native:{id,timestamp:time}});
const opts=patch=>({symbol:'BTC',interval:'1m',from:undefined,to:undefined,limit:undefined,orderId:undefined,...patch});
const cases=[];
function add(id,method,patch={}){
 const base={opts:opts({}),symbol:'BTC',limit:undefined,data:[],replies:[],markets:[market('BTC'),market('ETH')],tickers:[{id:'OLD'}],
  provider:false,providerSecond:undefined,providerFailure:'',fail:'',fails:[],delays:{},reverseDue:false,publicOnly:false,failGet:'',afterMarkets:undefined,
  optsSequence:{},speciesFail:false,mutateDepth:false,marketSymbolSequence:false,venueMessage:undefined,...patch};
 const input={};for(const[k,v]of Object.entries(base))input[k]=['provider','providerSecond','providerFailure','fail','fails','delays','reverseDue','publicOnly','failGet','optsSequence','speciesFail','mutateDepth','marketSymbolSequence'].includes(k)?v:encode(v);
 cases.push({id,method,input});
}
add('markets_filter','fetchMarkets',{data:{symbols:[market(),market('BAD','USDC'),market('BADM','USDT','USDC')]}});
add('markets_missing','fetchMarkets',{data:{}});
add('markets_sparse','fetchMarkets',{data:{symbols:[,market('ETH')]}});
add('markets_failure','fetchMarkets',{fail:'request:1:end'});
add('markets_factory_failure','fetchMarkets',{fail:'request:1:start'});
add('markets_normalizer_failure','fetchMarkets',{data:{symbols:[market()]},fail:'normalizeMarket'});
add('markets_report_failure','fetchMarkets',{fail:'request:1:end',fails:['report']});
add('markets_latest_fallback','fetchMarkets',{fail:'request:1:end',afterMarkets:[market('NEW')]});
add('markets_species_failure','fetchMarkets',{data:{symbols:[market()]},speciesFail:true});
add('tickers_join','fetchTickers',{replies:[[tick('BTC','OLD'),tick('BTC','LAST')],[tick('ETH','BOOK')]],markets:[market('BTC'),market('ETH'),market('SKIP')]});
add('tickers_numeric_keys','fetchTickers',{replies:[[tick(42,'NUMBER'),tick(Symbol('k'),'SYMBOL')],[]],markets:[market('42'),market('Symbol(k)')]});
add('tickers_empty','fetchTickers',{replies:[[],[]]});
add('tickers_parallel','fetchTickers',{replies:[[tick()],[tick('BTC','BOOK')]],delays:{'request:1':3,'request:2':1}});
add('tickers_failure','fetchTickers',{fail:'request:2:end',delays:{'request:1':3,'request:2':1}});
add('tickers_dual_failure','fetchTickers',{fails:['request:1:end','request:2:end'],reverseDue:true});
add('tickers_second_factory_failure','fetchTickers',{fail:'request:2:start'});
add('tickers_data_get_failure','fetchTickers',{failGet:'response1:data',replies:[[tick()],[tick()]]});
add('tickers_rejection_before_data_get','fetchTickers',{failGet:'response1:data',fail:'request:2:end',delays:{'request:1':1,'request:2':3}});
add('tickers_sparse_map_failure','fetchTickers',{replies:[[,tick()],[]]});
add('candles_recent','fetchOHLCV',{data:[candle(3,'LATE'),{native:null},candle(1,'EARLY'),candle(1,'TIE')]});
add('candles_historical_zero','fetchOHLCV',{opts:opts({from:0,limit:500}),data:[candle()]});
add('candles_to_only','fetchOHLCV',{opts:opts({to:null}),data:[]});
add('candles_recent_upper','fetchOHLCV',{opts:opts({limit:2000}),data:[]});
add('candles_recent_zero','fetchOHLCV',{opts:opts({limit:-0}),data:[]});
add('candles_negative_limit','fetchOHLCV',{opts:opts({limit:-3}),data:[]});
add('candles_unsupported','fetchOHLCV',{opts:opts({interval:'3m'})});
add('candles_unsupported_surrogate','fetchOHLCV',{opts:opts({interval:'\ud800'})});
add('candles_symbol_interval','fetchOHLCV',{opts:opts({interval:Symbol('x')})});
add('candles_opts_repeated','fetchOHLCV',{opts:opts({from:1}),optsSequence:{from:[1,9]},data:[]});
add('candles_non_array','fetchOHLCV',{data:{items:[candle()]}});
add('candles_failure','fetchOHLCV',{fail:'request:1:end'});
add('listen_candle_provider','listenOHLCV',{provider:true});
add('listen_candle_websocket','listenOHLCV');
add('listen_book_provider','listenOrderBook',{provider:true,symbol:'\ud800'});
add('listen_book_websocket','listenOrderBook');
add('listen_provider_changes','listenOHLCV',{provider:true,providerSecond:false});
add('listen_method_failure','listenOrderBook',{provider:true,fail:'method:subscribeOrderbook'});
add('depth_default','fetchOrderBook',{data:{lastUpdateId:9007199254740992,native:{bids:[[1,2]],asks:[[3,4]],lastUpdateId:'OLD'}}});
add('depth_null_limit','fetchOrderBook',{limit:null,data:{lastUpdateId:'\ud800',native:{bids:[],asks:[]}}});
add('depth_missing_id','fetchOrderBook',{data:{native:{bids:[],asks:[]}}});
add('depth_mutated_id','fetchOrderBook',{data:{lastUpdateId:'OLD',native:{bids:[],asks:[]}},mutateDepth:true});
add('depth_failure','fetchOrderBook',{fail:'request:1:end'});
add('public_trades','fetchPublicTrades',{data:[{native:{id:'T1'}},{native:null},{native:{id:'T2'}}]});
add('public_limit_lower','fetchPublicTrades',{limit:-3});
add('public_limit_upper','fetchPublicTrades',{limit:10000});
add('public_limit_nan','fetchPublicTrades',{limit:NaN});
add('public_sparse','fetchPublicTrades',{data:[,{native:{id:'T'}}]});
add('public_non_array','fetchPublicTrades',{data:{data:[{native:{id:'DROP'}}]}});
add('query_default','fetchUserTradeRows',{data:[fill()]});
add('query_filters','fetchUserTradeRows',{opts:opts({orderId:7,from:100000,to:200000,limit:1000}),data:{data:[fill('8',150),fill('7',99),fill(7,100),fill('7',200),fill('7',201)]}});
add('query_zero_bounds','fetchUserTradeRows',{opts:opts({from:0,to:0,limit:0}),data:[fill('1',500)]});
add('query_created_time','fetchUserTradeRows',{opts:opts({from:1000,to:2000}),data:[{orderId:'1',time:null,createdTime:1,symbol:'BTC'}]});
add('query_failure','fetchUserTradeRows',{fail:'request:1:end'});
add('history_order','fetchHistoricalFills',{data:[fill('1',100,'BTC','EARLY'),fill('2',300,'ETH','LATE'),fill('3',300,'BTC','TIE')]});
add('history_public','fetchHistoricalFills',{publicOnly:true});
add('history_falsey_filter','fetchHistoricalFills',{data:[fill(),{...fill('2',200),native:null}]});
add('history_missing_symbol','fetchHistoricalFills',{data:[fill('1',100,'','EMPTY')]});
add('history_live_symbol','fetchHistoricalFills',{data:[fill()],marketSymbolSequence:true});
add('markets_venue_message','fetchMarkets',{fail:'request:1:end',venueMessage:'VENUE'});
add('markets_empty_venue_message','fetchMarkets',{fail:'request:1:end',venueMessage:''});
assert.equal(cases.length,60,'market_reads::fixture_floor');
async function run(c){
 const input={};for(const[k,v]of Object.entries(c.input))input[k]=v?.kind?decode(v):v;
 const events=[],requests=[],reports=[],calls=[],tasks=[];let requestIndex=0,providerIndex=0,markets=input.markets,marketVersion='OLD';
 const callback=()=>{},instance=new scope.exports.Slice();
 function check(stage){if(input.fail===stage||input.fails.includes(stage)){const error=new RangeError('fail:'+stage);if(input.venueMessage!==undefined)error.response={data:{msg:input.venueMessage}};throw error;}}
 function record(stage){events.push(stage);check(stage);}
 function watch(value,label){
  const counts={};return Object.defineProperties({},Object.fromEntries(Object.keys(value).map(key=>[key,{enumerable:true,get(){
   const stage='get:'+label+':'+key;record(stage);if(input.failGet===label+':'+key)throw new RangeError('fail:'+stage);
   const seq=label==='opts'?input.optsSequence[key]:undefined;const index=counts[key]??0;counts[key]=index+1;
   return seq?seq[Math.min(index,seq.length-1)]:value[key];
  }}])));
 }
 const options=watch(input.opts,'opts');
 function schedule(key,value){let resolve,reject;const promise=new Promise((a,b)=>{resolve=a;reject=b;});promise.catch(()=>{});tasks.push({key,left:input.delays[key]??1,value,resolve,reject});return promise;}
 instance.xhr={get(endpoint,options){const index=++requestIndex,key='request:'+index;record(key+':start');requests.push({endpoint,params:options?.params===undefined?null:encode(options.params),timeout:options?.timeout===undefined?null:encode(options.timeout)});
  const data=input.replies.length?input.replies[Math.min(index-1,input.replies.length-1)]:input.data;
  if(input.speciesFail&&data.symbols)Object.defineProperty(data.symbols,'constructor',{get(){record('species');throw new RangeError('fail:species');}});
  if(input.marketSymbolSequence&&Array.isArray(data))for(const row of data){let n=0;Object.defineProperty(row,'symbol',{enumerable:true,get(){record('get:row:symbol');if(++n===1)return 'BTC';marketVersion='NEW';return 'ETH';}});}
  return schedule(key,watch({data},'response'+index));
 }};
 instance.store={get markets(){record('markets');return markets;},get tickers(){record('tickers');return input.tickers;},get marketsBySymbol(){record('marketsBySymbol');const version=marketVersion;return new Proxy({},{get(_,symbol){record('market:'+JSON.stringify(units(symbol)));return {tag:input.marketSymbolSequence?version+':'+symbol:symbol};}});}};
 instance.emitAndLogError=message=>{record('report');reports.push(encode(message));};instance.isPublicDataOnly=input.publicOnly;
 function receiver(name){const obj={};for(const method of ['subscribeCandle','subscribeOrderbook','listenOHLCV','listenOrderBook'])Object.defineProperty(obj,method,{get(){record('method:'+method);return function(argument,passed){record('call:'+method);calls.push({receiver:name,method,argument:encode(argument),callbackSame:passed===callback,argumentSame:argument===options||argument===input.symbol});return {unsubscribe:name+':'+method};};}});return obj;}
 const external=receiver('provider'),socket=receiver('websocket');
 Object.defineProperty(instance,'externalPublicDataProvider',{get(){record('provider');const enabled=++providerIndex===1?input.provider:input.providerSecond??input.provider;return enabled?external:null;}});
 Object.defineProperty(instance,'publicWebsocket',{get(){record('publicWebsocket');return socket;}});
 scope.normalizeMarket=row=>{record('normalizeMarket');return row.native;};
 scope.normalizeTicker=(ticker,market,book)=>{record('normalizeTicker');return {symbol:market.symbol,ticker:ticker?.native,book:book?.native};};
 scope.normalizeCandle=row=>{record('normalizeCandle');return row.native;};
 scope.normalizeOrderBook=data=>{record('normalizeOrderBook');if(input.mutateDepth)data.lastUpdateId='CHANGED';return data.native;};
 scope.normalizePublicTrade=(row,symbol)=>{record('normalizePublicTrade');return row.native&&{...row.native,symbol};};
 scope.normalizeExecution=(row,market)=>{record('normalizeExecution');return row.native&&{...row.native,market:market?.tag};};
 scope.Promise={all(promises){record('all');return Promise.all(promises);}};
 let result,error,done=false,promise;
 try {
  if(c.method==='fetchOHLCV'||c.method==='fetchUserTradeRows'||c.method==='fetchHistoricalFills')promise=instance[c.method](options);
  else if(c.method==='listenOHLCV')promise=instance.listenOHLCV(options,callback);
  else if(c.method==='listenOrderBook')promise=instance.listenOrderBook(input.symbol,callback);
  else if(c.method==='fetchOrderBook'||c.method==='fetchPublicTrades')promise=instance[c.method](input.symbol,input.limit);
  else promise=instance[c.method]();
 }catch(e){error={name:e.name,units:units(e.message)};done=true;}
 const prefix=events.slice();
 if(!done)Promise.resolve(promise).then(v=>{result=v;done=true;},e=>{error={name:e.name,units:units(e.message)};done=true;});
 await new Promise(setImmediate);
 for(let tick=0;!done||tasks.length;tick++){
  assert.ok(tick<100,'market_reads::scheduler_floor');const due=[];for(let i=tasks.length-1;i>=0;i--)if(--tasks[i].left<=0)due.unshift(tasks.splice(i,1)[0]);if(input.reverseDue)due.reverse();
  for(const t of due){events.push(t.key+':end');if(input.afterMarkets)markets=input.afterMarkets;try{check(t.key+':end');t.resolve(t.value);}catch(e){t.reject(e);}}
  await new Promise(setImmediate);
 }
 return {...c,prefix,result:encode(result),error:error??null,requests,reports,calls,events};
}
(async()=>{const result=[];for(const c of cases)result.push(await run(c));process.stdout.write(JSON.stringify(result));})().catch(e=>{console.error(e);process.exitCode=1;});
