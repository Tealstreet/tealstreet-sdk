const assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const {root, ts, load, capture} = require('./types-oracle.cjs');
const enums = load(path.join(root, 'packages/safe-cex/src/types.ts'));
const venue = load(path.join(root, 'packages/safe-cex/src/exchanges/weex/weex.types.ts'));
const names = ['cancelOrders', 'cancelSymbolOrders', 'cancelAllOrders', 'setLeverage', 'changeMarginType', 'changePositionMode', 'nukePositions', 'nuke', 'updateSymbolMetadata', 'isAlgoOrder'];
const file = path.join(root, 'packages/safe-cex/src/exchanges/weex/weex.exchange.ts');
const tree = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
const cls = tree.statements.find(n => ts.isClassDeclaration(n) && n.name?.text === 'WeexExchange');
const fields = cls.members.filter(n => ts.isPropertyDeclaration(n) && names.includes(n.name.getText(tree)));
assert.equal(fields.length, 10, 'account_actions::source_fields');
const compiled = ts.transpileModule('export class Slice {' + fields.map(n => n.getText(tree)).join('\n') + '}',
  {compilerOptions: {module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022}, reportDiagnostics: true});
assert.equal(compiled.diagnostics.filter(d => d.category === ts.DiagnosticCategory.Error).length, 0);
const scope = {exports: {}, ...enums, ...venue, chunk: require('lodash/chunk'), uniq: require('lodash/uniq')};
vm.runInNewContext(compiled.outputText, scope);
const units = s => Array.from({length: s.length}, (_, i) => s.charCodeAt(i));
const text = u => String.fromCharCode(...u);
const number = n => capture(n).bits;
const nativeNumber = bits => Buffer.from(bits, 'hex').readDoubleBE();
const order = (id, type = 'limit', symbol = 'BTC') => ({id: units(id), type, symbol: units(symbol)});
const config = (marginType = 'cross', positionMode = 'oneway') => ({marginType, positionMode, crossLeverage: number(7), isolatedLongLeverage: number(8), isolatedShortLeverage: number(9), extra: units('EXTRA')});
const cases = [];
function add(id, method, patch = {}) {
  cases.push({id, input: {method, symbol: units('BTC'), flag: true, side: null, leverage: number(15), min: number(1), max: number(100), market: true,
    orders: [order('N1'),order('A1__virtual')], selected: [0,1], stored: [0,1], positionSymbols: [units('BTC'),units('ETH'),units('BTC')], marketSymbols: [units('SOL'),units('SOL')],
    configs: [config()], positions: [units('REFRESHED')], delays: {}, fail: '', fails: [], reverseDue: false, mutateIds: {}, afterOrders: null, afterLimits: null, ...patch}});
}
add('cancel_empty','cancelOrders',{selected: []});
add('cancel_mixed','cancelOrders');
add('cancel_batches','cancelOrders',{orders: [...Array.from({length:21},(_,i)=>order('N'+i)),order('TP',enums.OrderType.TakeProfit),order('__A','conditional'),order('X__Y__Z')],selected: Array.from({length:24},(_,i)=>i), stored: []});
add('cancel_normal_failure','cancelOrders',{fail:'request:1:end'});
add('cancel_algo_failure','cancelOrders',{fail:'request:2:end'});
add('cancel_start_failure','cancelOrders',{fail:'request:1:start'});
add('cancel_mutated_id','cancelOrders',{mutateIds:{'1':{index:1,id:units('CHANGED__protect')}}});
add('cancel_status_failure','cancelOrders',{fail:'status'});
add('symbol_cancel','cancelSymbolOrders',{orders:[order('BTC1'),order('ETH1','limit','ETH')]});
add('symbol_parallel','cancelSymbolOrders',{delays:{'request:1':3,'request:2':1}});
add('symbol_failure_peer_survives','cancelSymbolOrders',{fail:'request:2:end',delays:{'request:1':3,'request:2':1}});
add('symbol_latest_store','cancelSymbolOrders',{orders:[order('OLD'),order('NEW')],stored:[0],afterOrders:[1]});
add('symbol_factory_failure','cancelSymbolOrders',{fail:'request:1:start'});
add('symbol_surrogate','cancelSymbolOrders',{symbol:[55296],orders:[{id:units('MATCH'),type:'limit',symbol:[55296]},order('OTHER')]});
add('all_cancel','cancelAllOrders');
add('all_parallel','cancelAllOrders',{delays:{'request:1':1,'request:2':3}});
add('all_failure','cancelAllOrders',{fail:'request:1:end',delays:{'request:1':1,'request:2':3}});
add('all_latest_store','cancelAllOrders',{stored:[0],afterOrders:[1]});
add('all_empty_store','cancelAllOrders',{stored:[]});
add('leverage_cross','setLeverage');
add('leverage_lower','setLeverage',{leverage:number(-10)});
add('leverage_upper','setLeverage',{leverage:number(1000)});
add('leverage_isolated_both','setLeverage',{configs:[config('isolated')]});
add('leverage_isolated_long','setLeverage',{configs:[config('isolated')],side:'long'});
add('leverage_isolated_short','setLeverage',{configs:[config('isolated')],side:'short'});
add('leverage_nan','setLeverage',{leverage:number(NaN)});
add('leverage_zero','setLeverage',{leverage:number(-0),min:number(-0),max:number(0)});
add('leverage_missing_market','setLeverage',{market:false,symbol:[55296]});
add('leverage_after_fetch_limits','setLeverage',{afterLimits:[number(20),number(30)]});
add('leverage_config_failure','setLeverage',{fail:'config:1:end'});
add('leverage_post_failure','setLeverage',{fail:'request:1:end'});
add('leverage_metadata_failure','setLeverage',{fail:'config:2:end'});
add('margin_isolated','changeMarginType',{configs:[config('cross','hedged')]});
add('margin_cross','changeMarginType',{flag:false});
add('margin_missing','changeMarginType',{symbol:null});
add('margin_empty','changeMarginType',{symbol:[]});
add('margin_post_failure','changeMarginType',{fail:'request:1:end'});
add('margin_metadata_failure','changeMarginType',{fail:'config:2:end'});
add('mode_explicit','changePositionMode');
add('mode_position_unique','changePositionMode',{symbol:null,configs:[config(),config('isolated','hedged')]});
add('mode_market_duplicates','changePositionMode',{symbol:[],positionSymbols:[]});
add('mode_no_targets','changePositionMode',{symbol:null,positionSymbols:[],marketSymbols:[],flag:false});
add('mode_second_post_failure','changePositionMode',{symbol:null,fail:'request:2:end'});
add('mode_cache_failure','changePositionMode',{fail:'cache'});
add('mode_setting_failure','changePositionMode',{fail:'setting'});
add('mode_oneway','changePositionMode',{flag:false});
add('close_positions','nukePositions');
add('close_post_failure','nukePositions',{fail:'request:1:end'});
add('close_fetch_failure','nukePositions',{fail:'positions:1:end'});
add('close_update_failure','nukePositions',{fail:'updatePositions'});
add('nuke_sequence','nuke');
add('nuke_close_failure','nuke',{fail:'request:1:end'});
add('nuke_cancel_failure','nuke',{fail:'request:2:end'});
add('metadata_refetch','updateSymbolMetadata');
add('metadata_failure','updateSymbolMetadata',{fail:'config:1:end'});
add('symbol_dual_rejection','cancelSymbolOrders',{fails:['request:1:end','request:2:end'],reverseDue:true});
add('all_dual_rejection','cancelAllOrders',{fails:['request:1:end','request:2:end'],reverseDue:true});
add('all_second_factory_failure','cancelAllOrders',{fail:'request:2:start'});
function encodeValue(v) {
  if (typeof v === 'string') return {kind:'string',units:units(v)};
  if (Array.isArray(v)) return {kind:'strings',items:v.map(units)};
  throw Error('unexpected request value');
}
const entries = v => v === undefined ? null : Object.entries(v).map(([k,val])=>[k,encodeValue(val)]);
async function run(c) {
  const input = c.input, events = [], requests = [], tasks = [], statuses = [], cache = [], configCalls = [], fetchedPositions = [];
  const ids = input.orders.map(o=>text(o.id)), orders = input.orders.map((o,i) => {
    const result = {type:o.type};
    Object.defineProperty(result,'id',{get(){events.push('id:'+i);check('id');return ids[i];}});
    Object.defineProperty(result,'symbol',{get(){events.push('symbol:'+i);return text(o.symbol);}});return result;
  });
  let stored = input.stored.slice(), limits = [input.min,input.max], accountMode = 'oneway', hedged = null, updatedPositions = null;
  let requestIndex = 0, configIndex = 0;
  const instance = new scope.exports.Slice();
  function check(stage) {if (input.fail === stage || input.fails.includes(stage)) throw new RangeError('fail:'+stage);}
  function schedule(key, value, finish = () => {}) {
    let resolve,reject;const promise = new Promise((a,b)=>{resolve=a;reject=b;});
    promise.catch(()=>{});
    tasks.push({key,left:input.delays[key] ?? 1,value,finish,resolve,reject});return promise;
  }
  function configObject(index) {
    const raw = input.configs[Math.min(index,input.configs.length-1)], result = {};
    for (const key of ['marginType','positionMode','crossLeverage','isolatedLongLeverage','isolatedShortLeverage','extra']) {
      Object.defineProperty(result,key,{enumerable:true,get(){events.push('configField:'+key);return key==='extra'?text(raw[key]):key.endsWith('Leverage')?nativeNumber(raw[key]):raw[key];}});
    }return result;
  }
  function request(method,url,opts) {
    const index = ++requestIndex, key = 'request:'+index;events.push(key+':start');
    requests.push({method,endpoint:url,body:entries(method==='post'?opts:opts?.data),params:entries(method==='delete'?opts?.params:undefined)});
    check(key+':start');
    return schedule(key,{},()=>{
      if (input.mutateIds[index]) ids[input.mutateIds[index].index]=text(input.mutateIds[index].id);
      if (input.afterOrders) stored=input.afterOrders.slice();
    });
  }
  instance.xhr = {delete:(url,opts)=>request('delete',url,opts),post:(url,body)=>request('post',url,body)};
  const algo = instance.isAlgoOrder;
  instance.isAlgoOrder = o => {events.push('algo:'+orders.indexOf(o));return algo(o);};
  instance.getOrFetchSymbolConfig = (symbol,refetch=false) => {
    const index = ++configIndex, key='config:'+index;events.push(key+':start'); configCalls.push({symbol:units(symbol),refetch});check(key+':start');
    return schedule(key,configObject(index-1),()=>{if(input.afterLimits)limits=input.afterLimits;});
  };
  instance.fetchPositions = () => {events.push('positions:1:start');check('positions:1:start');return schedule('positions:1',input.positions.map(text));};
  instance.symbolSettings = {set(symbol,next){events.push('cache');check('cache');cache.push({symbol:units(symbol),config:{marginType:next.marginType,positionMode:next.positionMode,crossLeverage:number(next.crossLeverage),isolatedLongLeverage:number(next.isolatedLongLeverage),isolatedShortLeverage:number(next.isolatedShortLeverage),extra:units(next.extra)}});}};
  Object.defineProperty(instance,'accountPositionMode',{set(value){events.push('accountMode');check('accountMode');accountMode=value;}});
  const market = {limits:{leverage:{}}};
  for(const [key,i] of [['min',0],['max',1]]) Object.defineProperty(market.limits.leverage,key,{get(){events.push('limit:'+key);return nativeNumber(limits[i]);}});
  const marketsBySymbol = new Proxy({}, {get(_,symbol){events.push('market');return input.market?market:undefined;}});
  const store = {marketsBySymbol,
    updateOrdersStatus(ids,status){events.push('status');check('status');statuses.push({ids:ids.map(o=>units(o.id)),status});},
    setSetting(key,value){assert.equal(key,'isHedged');events.push('setting');check('setting');hedged=value;},
    update({positions}){events.push('updatePositions');check('updatePositions');updatedPositions=positions.map(units);},
  };
  Object.defineProperty(store,'orders',{get(){events.push('orders');check('orders');return stored.map(i=>orders[i]);}});
  Object.defineProperty(store,'positions',{get(){events.push('positionSymbols');return input.positionSymbols.map(symbol=>({symbol:text(symbol)}));}});
  Object.defineProperty(store,'markets',{get(){events.push('marketSymbols');return input.marketSymbols.map(symbol=>({symbol:text(symbol)}));}});
  instance.store=store;
  scope.Promise={all(promises){events.push('all');check('all');return Promise.all(promises);}};
  const symbol = input.symbol===null?undefined:text(input.symbol);
  let promise;
  if(input.method==='cancelOrders')promise=instance.cancelOrders(input.selected.map(i=>orders[i]));
  else if(input.method==='setLeverage')promise=instance.setLeverage(symbol,nativeNumber(input.leverage),input.side??undefined);
  else if(input.method==='changeMarginType'||input.method==='changePositionMode')promise=instance[input.method](input.flag,symbol);
  else if(input.method==='cancelSymbolOrders'||input.method==='updateSymbolMetadata')promise=instance[input.method](symbol);
  else promise=instance[input.method]();
  const prefix=events.slice();let done=false,error=null;
  promise.then(()=>{done=true;},err=>{done=true;error={name:err.name,units:units(err.message)};});
  await new Promise(setImmediate);
  for(let tick=0;!done||tasks.length;tick++) {
    assert.ok(tick<100,'account_actions::scheduler_floor');
    const due=[];for(let i=tasks.length-1;i>=0;i--)if(--tasks[i].left<=0)due.unshift(tasks.splice(i,1)[0]);
    if(input.reverseDue)due.reverse();
    for(const t of due){events.push(t.key+':end');try{check(t.key+':end');t.finish();t.resolve(t.value);}catch(err){t.reject(err);}}
    await new Promise(setImmediate);
  }
  return {...c,prefix,error,requests,statuses,cache,configCalls,accountMode,hedged,updatedPositions,events};
}
(async()=>{const results=[];for(const c of cases)results.push(await run(c));assert.equal(results.length,58);process.stdout.write(JSON.stringify(results));})()
.catch(err=>{console.error(err);process.exitCode=1;});
