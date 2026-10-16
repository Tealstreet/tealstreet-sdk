const assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const {root, ts, load, capture} = require('./types-oracle.cjs');
const types = load(path.join(root, 'packages/safe-cex/src/types.ts'));
const venue = load(path.join(root, 'packages/safe-cex/src/exchanges/weex/weex.types.ts'));
const {LogSeverity} = load(path.join(root, 'packages/logging/src/LogSeverity.ts'));
const names = ['fetchSymbolMaxLeverage','fetchLeverageForSymbol','fetchBalance','fetchBalanceAndPositions','fetchOrders','fetchNormalOrders','fetchAlgoOrders','fetchSymbolConfigs','fetchOpenOrderRows'];
const file = path.join(root, 'packages/safe-cex/src/exchanges/weex/weex.exchange.ts');
const tree = ts.createSourceFile(file, fs.readFileSync(file,'utf8'), ts.ScriptTarget.Latest, true);
const cls = tree.statements.find(n => ts.isClassDeclaration(n) && n.name?.text === 'WeexExchange');
const fields = cls.members.filter(n => ts.isPropertyDeclaration(n) && names.includes(n.name.getText(tree)));
assert.equal(fields.length, 9, 'account_reads::source_fields');
function declaration(tree, name) {
  const statement = tree.statements.find(n => ts.isVariableStatement(n) && n.declarationList.declarations.some(d => d.name.getText(tree) === name));
  assert.ok(statement, 'account_reads::source_'+name);return statement.getText(tree);
}
const utilsFile = path.join(root,'packages/safe-cex/src/exchanges/weex/weex.utils.ts');
const utils = ts.createSourceFile(utilsFile,fs.readFileSync(utilsFile,'utf8'),ts.ScriptTarget.Latest,true);
const omitFile=path.join(root,'packages/safe-cex/src/utils/omitUndefined.ts');
const omitTree=ts.createSourceFile(omitFile,fs.readFileSync(omitFile,'utf8'),ts.ScriptTarget.Latest,true);
const compiled = ts.transpileModule(declaration(omitTree,'omitUndefined')+'\n'+declaration(tree,'extractArray')+'\n'+declaration(utils,'toNumber')+'\nexport class Slice {'+fields.map(n=>n.getText(tree)).join('\n')+'}',
 {compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022},reportDiagnostics:true});
assert.equal(compiled.diagnostics.filter(d=>d.category===ts.DiagnosticCategory.Error).length,0);
const scope = {exports:{}, ...types, ...venue, LogSeverity, uniqBy:require('lodash/uniqBy'),
 omitBy:require('lodash/omitBy')};
const context=vm.createContext(scope);
vm.runInContext(compiled.outputText,context);
const arrayPrototype=vm.runInContext("Array.prototype",context);
const originalPush=arrayPrototype.push;
const units = s=>Array.from({length:s.length},(_,i)=>s.charCodeAt(i));
const text = u=>String.fromCharCode(...u);
function encode(v) {
 if(v===undefined)return {kind:'undefined'};if(v===null)return {kind:'null'};
 if(typeof v==='number')return {kind:'number',bits:capture(v).bits};
 if(typeof v==='string')return {kind:'string',units:units(v)};
 if(typeof v==='boolean')return {kind:'bool',value:v};
 if(v instanceof Map)return {kind:'array',items:Array.from(v,([key,value])=>encode([key,value]))};
 if(Array.isArray(v))return {kind:'array',items:Array.from({length:v.length},(_,i)=>i in v?encode(v[i]):{kind:'hole'})};
 return {kind:'object',fields:Object.entries(v).map(([key,value])=>[key,encode(value)])};
}
function decode(v) {
 switch(v.kind){case 'undefined':return undefined;case 'null':return null;case 'bool':return v.value;case 'number':return Buffer.from(v.bits,'hex').readDoubleBE();case 'string':return text(v.units);
 case 'array':{const a=new Array(v.items.length);v.items.forEach((x,i)=>{if(x.kind!=='hole')a[i]=decode(x);});return a;}
 case 'object':return Object.fromEntries(v.fields.map(([k,x])=>[k,decode(x)]));default:throw Error(v.kind);}
}
const order=(id,symbol='BTC',normal={id,symbol},algo=[{id:id+'__tp',symbol},{id:id+'__sl',symbol}])=>({symbol,normal,algo});
const cfg=(symbol='BTC',marginType='ISOLATED',separatedType='SEPARATED',crossLeverage='7',isolatedLongLeverage='8',isolatedShortLeverage='9')=>({symbol,marginType,separatedType,crossLeverage,isolatedLongLeverage,isolatedShortLeverage});
const cases=[];
function add(id,method,patch={}) {
 const base={symbol:'BTC',symbols:undefined,publicOnly:false,market:false,max:-0,config:{marginType:'cross',crossLeverage:7,isolatedLongLeverage:8,isolatedShortLeverage:9},
  data:[],replies:[],cached:[['OLD',cfg('OLD')]],fetchedLeverage:{OLD:{buyLeverage:2,sellLeverage:3}},balance:{total:17},positions:[{id:'P'}],endpoint:venue.ENDPOINTS.OPEN_ORDERS,
  delays:{},fail:'',fails:[],reverseDue:false,appendSymbols:undefined,failGet:'',includeMarket:false,replaceCache:false,pushLimit:null, ...patch};
 const input={};for(const[k,v]of Object.entries(base))input[k]=['delays','fail','fails','reverseDue','publicOnly','market','failGet','includeMarket','replaceCache','pushLimit'].includes(k)?v:encode(v);
 cases.push({id,method,input});
}
add('max_cached','fetchSymbolMaxLeverage',{market:true});
add('max_remote','fetchSymbolMaxLeverage',{data:{symbols:[{maxLeverage:'125x'}]}});
add('max_default','fetchSymbolMaxLeverage',{data:{}});
add('max_invalid','fetchSymbolMaxLeverage',{data:{symbols:[{maxLeverage:'Infinity'}]}});
add('max_failure','fetchSymbolMaxLeverage',{fail:'request:1:end'});
add('leverage_cross','fetchLeverageForSymbol');
add('leverage_isolated','fetchLeverageForSymbol',{config:{marginType:'isolated',crossLeverage:7,isolatedLongLeverage:-0,isolatedShortLeverage:19}});
add('leverage_failure','fetchLeverageForSymbol',{fail:'config:1:end',symbol:'\ud800'});
add('leverage_update_failure','fetchLeverageForSymbol',{fail:'updateLeverage'});
add('leverage_get_failure','fetchLeverageForSymbol',{failGet:'config:isolatedShortLeverage',config:{marginType:'isolated',crossLeverage:7,isolatedLongLeverage:8,isolatedShortLeverage:9}});
add('balance_array','fetchBalance',{data:[{coin:'USDT',total:12}]});
add('balance_nested','fetchBalance',{data:{data:[{coin:'USDC',total:23}]}});
add('balance_empty','fetchBalance',{data:{}});
add('balance_failure','fetchBalance',{fail:'request:1:end'});
add('balance_normalizer_failure','fetchBalance',{fail:'balanceNormalize'});
add('balance_positions','fetchBalanceAndPositions');
add('balance_positions_parallel','fetchBalanceAndPositions',{delays:{'balance:1':3,'positions:1':1}});
add('balance_positions_failure','fetchBalanceAndPositions',{fail:'positions:1:end',delays:{'balance:1':3,'positions:1':1}});
add('balance_positions_dual_failure','fetchBalanceAndPositions',{fails:['balance:1:end','positions:1:end'],reverseDue:true});
add('configs_public','fetchSymbolConfigs',{publicOnly:true});
add('configs_rows','fetchSymbolConfigs',{data:[cfg(),{...cfg('ETH','cross','COMBINED','0','bad'),isolatedShortLeverage:undefined}]});
add('configs_duplicate','fetchSymbolConfigs',{data:{data:[cfg(),cfg('BTC','CROSS','OTHER','33','44','55')]},symbol:''});
add('configs_failure','fetchSymbolConfigs',{fail:'request:1:end'});
add('configs_partial','fetchSymbolConfigs',{data:[cfg(),cfg('ETH')],fail:'cacheSet:2'});
add('configs_null_row','fetchSymbolConfigs',{data:[cfg(),null]});
add('rows_all','fetchOpenOrderRows',{data:{orders:[order('N1'),order('N2')]}});
add('rows_empty_symbols','fetchOpenOrderRows',{symbols:[],data:{items:[order('N1')]}});
add('rows_symbols','fetchOpenOrderRows',{symbols:['BTC','ETH'],replies:[{orders:[order('N1')]},{items:[order('N2','ETH')]}]});
add('rows_algo_page','fetchOpenOrderRows',{endpoint:venue.ENDPOINTS.OPEN_ALGO_ORDERS,symbols:['\ud800',null,'__undefined__'],data:[]});
add('rows_live_symbols','fetchOpenOrderRows',{symbols:['BTC'],appendSymbols:['ETH'],replies:[{orders:[order('N1')]},{items:[order('N2','ETH')]}]});
add('rows_failure','fetchOpenOrderRows',{symbols:['BTC','ETH'],fail:'request:2:end'});
add('normal_orders','fetchNormalOrders',{data:[order('N1'),order('DROP','BTC',null),order('N2','ETH')]});
add('normal_normalizer_failure','fetchNormalOrders',{data:[order('N1')],fail:'normalNormalize'});
add('algo_orders','fetchAlgoOrders',{data:{orders:[order('A1'),order('EMPTY','BTC',{},[])]}});
add('algo_sparse_flatten','fetchAlgoOrders',{data:[order('A1','BTC',{},[{id:'FIRST'},,...[{id:'LAST'}]])]});
add('algo_normalizer_failure','fetchAlgoOrders',{data:[order('A1')],fail:'algoNormalize'});
add('orders_public','fetchOrders',{publicOnly:true});
add('orders_unique','fetchOrders',{replies:[[order('SAME')],[order('A','BTC',{},[{id:'SAME',symbol:'OTHER'},{id:'A'}])]]});
add('orders_normal_failure','fetchOrders',{replies:[[],[order('A')]],fail:'request:1:end'});
add('orders_algo_failure','fetchOrders',{replies:[[order('N')],[]],fail:'request:2:end'});
add('orders_both_failure','fetchOrders',{fails:['request:1:end','request:2:end'],reverseDue:true});
add('max_factory_failure','fetchSymbolMaxLeverage',{fail:'request:1:start'});
add('leverage_factory_failure','fetchLeverageForSymbol',{fail:'config:1:start'});
add('balance_factory_failure','fetchBalance',{fail:'request:1:start'});
add('configs_factory_failure','fetchSymbolConfigs',{fail:'request:1:start'});
add('rows_factory_failure','fetchOpenOrderRows',{fail:'request:1:start'});
add('balance_positions_first_factory_failure','fetchBalanceAndPositions',{fail:'balance:1:start'});
add('balance_positions_second_factory_failure','fetchBalanceAndPositions',{fail:'positions:1:start'});
add('balance_positions_aggregate_failure','fetchBalanceAndPositions',{fail:'all'});
add('normal_capture_market','fetchNormalOrders',{data:[order('N')],includeMarket:true});
add('algo_capture_market','fetchAlgoOrders',{data:[order('A')],includeMarket:true});
add('orders_reverse_success','fetchOrders',{replies:[[order('N')],[order('A')]],reverseDue:true});
add('configs_capture_cache','fetchSymbolConfigs',{data:[cfg()],replaceCache:true});
add('rows_push_failure','fetchOpenOrderRows',{data:[order('N1'),order('N2')],pushLimit:1});
assert.equal(cases.length,54,'account_reads::fixture_floor');
async function run(c) {
 const raw=c.input,input={};for(const[k,v]of Object.entries(raw))input[k]=v?.kind?decode(v):v;
 const events=[],requests=[],warnings=[],configCalls=[],tasks=[],cache=new Map(input.cached),leverage={...input.fetchedLeverage};
 let requestIndex=0,cacheIndex=0;
 function check(stage){if(input.fail===stage||input.fails.includes(stage))throw new RangeError('fail:'+stage);}
 function record(stage){events.push(stage);check(stage);}
 function watch(value,label) {
  if(!value||typeof value!=='object')return value;
  return Object.defineProperties({},Object.fromEntries(Object.keys(value).map(key=>[key,{enumerable:true,get(){const stage='get:'+label+':'+key;events.push(stage);if(input.failGet===label+':'+key)throw new RangeError('fail:'+stage);return value[key];}}])));

 }
 function schedule(key,value,finish=()=>{}) {
  let resolve,reject;const promise=new Promise((a,b)=>{resolve=a;reject=b;});promise.catch(()=>{});
  tasks.push({key,left:input.delays[key]??1,value,finish,resolve,reject});return promise;
 }
 arrayPrototype.push=function(...values){if(input.pushLimit!==null&&values.length>input.pushLimit)throw new RangeError('fail:pushArguments');return originalPush.apply(this,values);};
 const instance=new scope.exports.Slice();
 instance.xhr={get(endpoint,options){const key='request:'+(++requestIndex);record(key+':start');requests.push({endpoint:units(endpoint),params:options?.params===undefined?null:Object.entries(options.params).map(([k,v])=>[k,encode(v)])});
  const data=input.replies.length?input.replies[Math.min(requestIndex-1,input.replies.length-1)]:input.data;
  if((input.includeMarket||input.replaceCache)&&Array.isArray(data))for(const row of data){const symbol=row.symbol;Object.defineProperty(row,'symbol',{enumerable:true,get(){events.push('get:row:symbol');instance.store.marketsBySymbol=marketMap('NEW');if(input.replaceCache)instance.symbolSettings=new Map();return symbol;}});}
  return schedule(key,{data},()=>{if(requestIndex===1&&input.appendSymbols)input.symbols.push(...input.appendSymbols);});}};
 instance.isPublicDataOnly=input.publicOnly;
 instance.getOrFetchSymbolConfig=(symbol,refetch)=>{record('config:1:start');configCalls.push({symbol:encode(symbol),refetch});return schedule('config:1',watch(input.config,'config'));};
 function marketMap(tag){return new Proxy({},{get(_,symbol){record('market:'+JSON.stringify(units(symbol)));if(input.includeMarket)return {tag};return input.market?{limits:{leverage:{max:input.max}}}:undefined;}});}
 instance.store={marketsBySymbol:marketMap('OLD'),
  fetchedLeverage:leverage,update({fetchedLeverage}){record('updateLeverage');Object.assign(leverage,fetchedLeverage);}};
 instance.symbolSettings=cache;
 const originalSet=cache.set.bind(cache);cache.set=(symbol,config)=>{record('cacheSet:'+(++cacheIndex));return originalSet(symbol,config);};
 instance.log=(message,severity)=>{assert.equal(severity,LogSeverity.Warning);record('warning');warnings.push(units(message));};
 if(c.method==='fetchBalanceAndPositions') {
  instance.fetchBalance=()=>{record('balance:1:start');return schedule('balance:1',input.balance);};
  instance.fetchPositions=()=>{record('positions:1:start');return schedule('positions:1',input.positions);};
 }
 scope.normalizeBalance=rows=>{record('balanceNormalize');return {rows};};
 scope.normalizeOrder=(row,market)=>{record('normalNormalize');return input.includeMarket?{order:row.normal,market:market.tag}:row.normal;};
 scope.normalizeAlgoOrder=(row,market)=>{record('algoNormalize');return input.includeMarket?row.algo.map(order=>({order,market:market.tag})):row.algo;};
 scope.Promise={all(promises){record('all');return Promise.all(promises);},allSettled(promises){record('settled');return Promise.allSettled(promises);}};
 const args=c.method==='fetchOpenOrderRows'?[input.endpoint,input.symbols]:['fetchSymbolMaxLeverage','fetchLeverageForSymbol','fetchSymbolConfigs'].includes(c.method)?[input.symbol]:[input.symbols];
 const promise=instance[c.method](...args),prefix=events.slice();let done=false,result,error;
 promise.then(value=>{done=true;result=value;},err=>{done=true;error={name:err.name,units:units(err.message)};});
 await new Promise(setImmediate);
 for(let tick=0;!done||tasks.length;tick++) {
  assert.ok(tick<100,'account_reads::scheduler_floor');const due=[];
  for(let i=tasks.length-1;i>=0;i--)if(--tasks[i].left<=0)due.unshift(tasks.splice(i,1)[0]);if(input.reverseDue)due.reverse();
  for(const t of due){events.push(t.key+':end');try{check(t.key+':end');t.finish();t.resolve(t.value);}catch(e){t.reject(e);}}
  await new Promise(setImmediate);
 }
 return {...c,prefix,result:encode(result),error:error??null,requests,warnings,configCalls,cache:encode(instance.symbolSettings),originalCache:encode(cache),leverage:encode(leverage),events};
}
(async()=>{const result=[];for(const c of cases)result.push(await run(c));process.stdout.write(JSON.stringify(result));})()
.catch(e=>{console.error(e);process.exitCode=1;});
