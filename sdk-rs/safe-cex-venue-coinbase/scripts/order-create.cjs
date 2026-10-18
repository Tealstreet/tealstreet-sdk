const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm'), assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH ?? 'typescript');
const root = path.resolve(__dirname, '../../..');
function source(rel) {const file=path.join(root,rel);return ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);}
const exchange = source('packages/safe-cex/src/exchanges/coinbase/coinbase.exchange.ts');
const owner = exchange.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text==='CoinbaseExchange');
assert.ok(owner,'live CoinbaseExchange declaration');
const names=['shouldPollOrders','mergeRestOrdersWithFreshWsState','resolveMarketByCoinbaseProductId','formatCreateOrder','buildOptimisticOrderFromCreate','emitOrdersFetchError','placeOrder','cancelOrders'];
const fields=names.map(name=>{const n=owner.members.find(n=>ts.isPropertyDeclaration(n)&&n.name.getText(exchange)===name);assert.ok(n?.initializer,'live field '+name);return name+' = '+n.initializer.getText(exchange)+';';});
const constants=exchange.statements.filter(ts.isVariableStatement).filter(n=>n.declarationList.declarations.some(d=>['COINBASE_ORDER_POLL_INTERVAL_MS','COINBASE_ORDER_RATE_LIMIT_BACKOFF_MS','COINBASE_ORDER_REST_LAG_TOLERANCE_MS'].includes(d.name.getText(exchange))));
assert.equal(constants.length,3,'live order timing constants');
const types=source('packages/safe-cex/src/types.ts');
const enums=types.statements.filter(n=>ts.isEnumDeclaration(n)&&['OrderStatus','OrderType','OrderSide','OrderTimeInForce','LogSeverity'].includes(n.name.text));
assert.equal(enums.length,4,'live order enum declarations');
const logging=source('packages/logging/src/LogSeverity.ts');
const severity=logging.statements.find(n=>ts.isEnumDeclaration(n)&&n.name.text==='LogSeverity');
assert.ok(severity,'live logging severity enum');
enums.push(severity);
const math=source('packages/safe-cex/src/utils/safe-math.ts');
const mathText=math.statements.filter(n=>!ts.isImportDeclaration(n)).map(n=>n.getText(math)).join('\n');
const notNumber=source('packages/math/src/isNotNumber.ts');
const predicate=notNumber.statements.filter(ts.isVariableStatement).flatMap(n=>[...n.declarationList.declarations]).find(n=>n.name.getText(notNumber)==='isNotNumber');
assert.ok(predicate?.initializer);
const utils=source('packages/safe-cex/src/exchanges/coinbase/coinbase.utils.ts');
const helperNames=['COINBASE_USDC_ALIAS_EXCEPTIONS','normalizeCoinbaseUsdEquivalentProductId','getCoinbaseUsdEquivalentAliasProductId','resolveCoinbaseUsdEquivalentMarket'];
const helpers=utils.statements.filter(ts.isVariableStatement).filter(n=>n.declarationList.declarations.some(d=>helperNames.includes(d.name.getText(utils))));
assert.equal(helpers.length,4,'live USD equivalence declarations');
const venueTypes=source('packages/safe-cex/src/exchanges/coinbase/coinbase.types.ts');
const endpoints=venueTypes.statements.filter(ts.isVariableStatement).find(n=>n.declarationList.declarations.some(d=>d.name.getText(venueTypes)==='ENDPOINTS'));
assert.ok(endpoints,'live REST endpoint table');
let clock=0,trace=[];
const exported={};
const text=[endpoints.getText(venueTypes),...constants.map(n=>n.getText(exchange)),...enums.map(n=>n.getText(n===severity?logging:types)),`const isNotNumber = ${predicate.initializer.getText(notNumber)};`,mathText,...helpers.map(n=>n.getText(utils)),`class Receiver {${fields.join('\n')}}`,`export {Receiver};`].join('\n');
vm.runInNewContext(ts.transpileModule(text,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,{exports:exported,Error,Date:{now(){trace.push({kind:'clock'});return clock;}},uuid(){trace.push({kind:'uuid'});return 'uuid-17';},chunk:require('lodash/chunk'),forEachSeries:require('p-iteration').forEachSeries});
function revive(value) {if(value&&typeof value==='object'){if('$n' in value)return value.$n==='-0'?-0:Number(value.$n);if(Array.isArray(value))return value.map(revive);return Object.fromEntries(Object.entries(value).map(([k,v])=>[k,revive(v)]));}return value;}
function capture(v) {if(v===undefined)return {$undefined:true};if(typeof v==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(v);return {$bits:b.toString('hex')};}if(require('node:util').types.isNativeError(v))return {name:v.name,message:v.message};if(v&&typeof v==='object'){if(Array.isArray(v))return v.map(capture);return Object.fromEntries(Object.entries(v).map(([k,v])=>[k,capture(v)]));}return v;}
if(process.argv.includes('--retained-merge')) {
 const r=new exported.Receiver(),info={missing:undefined,array:[NaN,Infinity,undefined]};
 const order={id:'fresh',status:'open',lastUpdate:0,info};r.store={orders:[order]};
 const merged=r.mergeRestOrdersWithFreshWsState([]);assert.equal(merged.length,1);
 assert.equal(merged[0].info,info);process.stdout.write(JSON.stringify(capture(merged[0].info)));return;
}
function market(id) {return {id,symbol:id,base:'BTC',quote:id.endsWith('USDC')?'USDC':'USD',active:true,precision:{amount:0.001,price:0.05},limits:{amount:{min:0.001,max:100000},leverage:{min:1,max:10}},isSpot:true,isExpiring:false,contractSize:1};}
function receiver(input) {
 const r=new exported.Receiver(), markets=Object.fromEntries((input.marketKeys??['BTC-USD','BTC-USDC']).map(id=>[id,market(id)]));
 for(const m of Object.values(markets)){Object.assign(m,input.marketPatch??{});if(input.marketRemove)delete m[input.marketRemove];}
 let resolveCount=0,postCancelCount=0,updateCount=0;
 function fail(at){if(input.throwAt===at)throw ('hostError' in input?input.hostError:input.error);}
 const stored=(input.stored??[]).map(order=>({...order}));
 r.store={get loaded(){trace.push({kind:'loaded'});return {orders:input.loaded??false};},get orders(){trace.push({kind:'orders'});return stored;},marketsBySymbol:new Proxy(markets,{get(target,key){trace.push({kind:'market',symbol:key});return target[key];}}),get marketsById(){trace.push({kind:'markets'});return markets;},addOrUpdateOrder(order){trace.push({kind:'add',order});fail('add');},updateOrdersStatus(ids,status){trace.push({kind:'update',ids:ids.map(o=>o.id),status});fail('update'+(++updateCount));}};
 r.resolveCallerClientOrderId=opts=>{trace.push({kind:'resolve',ordinal:++resolveCount});fail('resolve'+resolveCount);return opts.clientOrderId;};
 r.xhr={async post(endpoint,payload){if(endpoint.endsWith('batch_cancel')){trace.push({kind:'cancel',endpoint,ids:payload.order_ids});fail('postCancel'+(++postCancelCount));return {data:{}};}trace.push({kind:'create',endpoint,payload});fail('postCreate');return {data:input.response??{success:true,order_id:'created-23'}};}};
 r.scheduleOpenOrdersRestReconciliation=reason=>{trace.push({kind:'reconcile',reason});fail('reconcile');};
 r.schedulePrivateUserStreamWriteCheck=()=>{trace.push({kind:'write'});fail('write');};
 r.log=(message,severity)=>{trace.push({kind:'warning',message,severity});fail('warning');};
 r.emitAndLogError=(...args)=>{trace.push({kind:'emit',args});fail('emit');};
 r.nextOrdersPollAt=input.next??0;
 return r;
}
(async()=>{
 const fixture=JSON.parse(fs.readFileSync(path.resolve(__dirname,'../tests/fixtures/order-create.json'),'utf8'));
 const counts={format:42,optimistic:31,place:18,cancel:8,poll:8,merge:7,fetchError:8,resolve:7};
 const out={};
 for(const [family,cases] of Object.entries(fixture)){
  assert.equal(cases.length,counts[family],'independent '+family+' floor');out[family]=[];
  for(const original of cases){const input=revive(original);trace=[];clock=input.now??10000;const r=receiver(input);let result,error,identity,initialTrace;
   try{switch(family){
    case 'format':result=r.formatCreateOrder(input.opts);break;
    case 'optimistic':result=r.buildOptimisticOrderFromCreate(input.payload,'optimistic-19',input.response);break;
    case 'place':{const pending=r.placeOrder(input.opts);initialTrace=capture(trace);result=await pending;break;}
    case 'cancel':{const pending=r.cancelOrders(Array.from({length:input.count},(_,i)=>({id:'order-'+i})));initialTrace=capture(trace);result=await pending;break;}
    case 'poll':result=r.shouldPollOrders();break;
    case 'merge':result=r.mergeRestOrdersWithFreshWsState(input.rest);identity=result===input.rest;break;
    case 'fetchError':result=r.emitOrdersFetchError(input.error);break;
    case 'resolve':result=r.resolveMarketByCoinbaseProductId(input.product);break;
   }}catch(e){error=e===undefined?{$undefined:true}:require('node:util').types.isNativeError(e)?{name:e.name,message:e.message}:e;}
   out[family].push({label:input.label,result:capture(result),error:error===undefined?null:capture(error),trace:capture(trace),next:capture(r.nextOrdersPollAt),identity,initialTrace});
  }
 }
 out.hot=[];
 for(const family of ['place','cancel']) {
  const input=revive(fixture[family][family==='place'?0:3]);trace=[];clock=10000;const r=receiver(input);
  if(family==='place') r.placeOrder(input.opts); else r.cancelOrders(Array.from({length:input.count},(_,i)=>({id:'order-'+i})));
  const initialTrace=capture(trace);await new Promise(resolve=>setImmediate(resolve));out.hot.push({family,initialTrace,trace:capture(trace)});
 }
 trace=[];clock=10000;const r=receiver({});const payload={client_order_id:'caller',product_id:'BTC-USDC',side:'BUY',order_configuration:{limit_limit_gtc:{base_size:'3',limit_price:'17'}}};
 const response={object:{x:1},array:[2],primitive:3,optimisticFromCreateOrder:false};const optimistic=r.buildOptimisticOrderFromCreate(payload,'id',response);
 const identities={object:optimistic.info.object===response.object,array:optimistic.info.array===response.array,root:optimistic.info===response};response.object.x=19;response.array.push(23);response.object={x:29};response.primitive=31;
 out.alias={identities,info:capture(optimistic.info),response:capture(response)};
 out.timings=[];
 for(const reject of [false,true]) {
  const input={count:1,...(reject?{throwAt:'postCancel1',error:{message:'failed'}}:{})};trace=[];clock=10000;const r=receiver(input);
  const pending=r.cancelOrders([{id:'order-0'}]);trace.push({kind:'marker',name:'after-call'});Promise.resolve().then(()=>trace.push({kind:'marker',name:'queued'}));await pending;
  out.timings.push({reject,trace:capture(trace)});
 }
 process.stdout.write(JSON.stringify(out));
})().catch(e=>{console.error(e);process.exit(1);});
