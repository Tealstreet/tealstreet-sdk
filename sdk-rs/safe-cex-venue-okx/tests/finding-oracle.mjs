import assert from 'node:assert/strict';
import fs from 'node:fs';
import {loadSource} from './source-loader.mjs';
import {ts} from '../../safe-cex-store/tests/source_loader.mjs';
const root = new URL('../../../packages/safe-cex/src/exchanges/okx/', import.meta.url);
const keys = ['constructor','__defineGetter__','__defineSetter__','hasOwnProperty','__lookupGetter__','__lookupSetter__','isPrototypeOf','propertyIsEnumerable','toString','valueOf','__proto__','toLocaleString'];
assert.equal(keys.length, 12, 'independent stock member floor');
const income = loadSource(new URL('okx.income.ts', root));
const source = new URL('okx.exchange.ts', root);
const ast = ts.createSourceFile('okx.exchange.ts', fs.readFileSync(source, 'utf8'), ts.ScriptTarget.Latest, true);
const klass = ast.statements.find(n => ts.isClassDeclaration(n) && n.name.text === 'OKXExchange');
const member = klass.members.find(n => n.name?.getText(ast) === 'findOrderByClientOrderId');
const tableNames = ['OKX_LOOKUP_LIVE_STATE','OKX_LOOKUP_STATUS','OKX_ORDER_DOES_NOT_EXIST'];
const tables = ast.statements.filter(n => ts.isVariableStatement(n) && n.declarationList.declarations.some(d => tableNames.includes(d.name.getText(ast))));
assert.equal(tables.length, 3, 'actual lookup table inventory');
const domain = loadSource(new URL('../../types.ts', root));
const {orderCarriesClientOrderId}=loadSource(new URL('../clientOrderId.ts',root), ['orderCarriesClientOrderId'], ['CLIENT_ID_INFO_FIELDS','sameClientOrderId','orderCarriesClientOrderId']);
const exported = {};
new Function('exports', 'OrderStatus', 'ENDPOINTS', 'orderCarriesClientOrderId', ts.transpileModule(tables.map(n => n.getText(ast)).join('\n') + '\nexport class Lookup {' + member.getText(ast) + '}', {compilerOptions: {target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS}}).outputText)(exported, domain.OrderStatus, {ORDER_DETAIL:'/api/v5/trade/order'}, orderCarriesClientOrderId);
const nativeTag = (value, key) => {
  assert.equal(value, key === '__proto__' ? Object.prototype : Object.prototype[key], 'source inherited referent BEFORE serialization');
  const kind = typeof value;
  assert.equal(kind, key === '__proto__' ? 'object' : 'function', 'source native category/status kind');
  return {kind, member:key, truthy:Boolean(value)};
};
const lookup = [];
for (const key of keys) for (const outcome of ['mapped','empty','throw']) {
  const calls = [], receiver = new exported.Lookup();
  receiver.store = {orders:[], marketsBySymbol:{BTCUSDT:{id:'BTC-USDT-SWAP'}}};
  receiver.xhr = {get:async (url, options) => {calls.push('request');assert.equal(url,'/api/v5/trade/order');assert.deepEqual(options.params,{instId:'BTC-USDT-SWAP',clOrdId:'client'});return {data:{data:[{state:key,ordId:'fixture'}]}};}};
  const original = new Error('mapper sentinel');
  receiver.mapOrders = rows => {
    calls.push('mapper'); nativeTag(rows[0].state, key);
    if (outcome === 'throw') throw original;
    return outcome === 'empty' ? [] : [{id:'mapped',status:'open'}];
  };
  let result, error;
  try {result = await receiver.findOrderByClientOrderId('BTCUSDT','client');} catch (e) {error=e;}
  assert.deepEqual(calls,['request','mapper'],'source callback effect order');
  if(outcome==='mapped') nativeTag(result.order.status,key);
  else if(outcome==='empty') assert.deepEqual(result,{found:false,definitive:false});
  else assert.equal(error,original,'source original mapper error identity');
  lookup.push({key,outcome,kind:key==='__proto__'?'object':'function',calls});
}
assert.equal(lookup.length,36,'12 members x 3 mapper continuations');
const bills = [];
for (const key of keys) for (const shape of ['category','fee-only','empty','later-coercion']) {
  const tag=nativeTag(income.okxBillCategory(key),key);let resolverCalls=0;
  const row={ts:'1',instId:'BTC',billId:'bill',ccy:'USDT',type:key,pnl:shape==='category'?'2':'0',fee:shape==='fee-only'?'-0.5':'0',balChg:shape==='category'?'3':shape==='fee-only'?'-0.5':'0'};
  if(shape==='later-coercion')row.ccy={toString:null};
  let result,error;
  try{result=income.mapOkxBillToIncomeRecords(row,()=>{resolverCalls++;return 'BTCUSDT';});}catch(e){error=e;}
  assert.equal(resolverCalls,1,'source resolver effect before projection');
  if(shape==='category') {assert.equal(result.length,2,'PnL and residual remain distinct');for(const record of result){nativeTag(record.category,key);assert.equal(record.incomeType,undefined,'source income type lookup remains undefined');}}
  if(shape==='fee-only'){assert.equal(result.length,1);assert.equal(result[0].category,'commission');assert.equal(result[0].income,-0.5);}
  if(shape==='empty')assert.deepEqual(result,[]);
  if(shape==='later-coercion'){assert.equal(error.name,'TypeError');assert.equal(error.message,'Cannot convert object to primitive value');}
  bills.push({key,shape,tag,resolverCalls});
}
assert.equal(bills.length,48,'12 members x 4 income continuations');
const spread=[];
for (const raw of [true,1,'ab',['x',2],{},'😀']) {
 const receiver=new exported.Lookup();receiver.store={orders:[],marketsBySymbol:{BTCUSDT:{id:'BTC-USDT-SWAP'}}};
 receiver.xhr={get:async()=>({data:{data:[raw]}})};
 let mapped;
 receiver.mapOrders=rows=>{mapped=rows[0];return [];};
 const result=await receiver.findOrderByClientOrderId('BTCUSDT','client');assert.deepEqual(result,{found:false,definitive:false});
 assert.equal(typeof mapped,'object');assert.equal(mapped.state,'live');
 if(raw!=='😀') {
  const expected=typeof raw==='string'?{'0':'a','1':'b',state:'live'}:Array.isArray(raw)?{'0':'x','1':2,state:'live'}:{state:'live'};
  assert.deepEqual(mapped,expected,'source own spread keys and values before serialization');
 }
 if(raw==='😀'){assert.equal(mapped[0].length,1);assert.equal(mapped[0].charCodeAt(0),0xd83d);assert.equal(mapped[1].charCodeAt(0),0xde00);}
 spread.push({raw,keys:Object.keys(mapped),mapped:raw==='😀'?null:mapped,units:raw==='😀'?[mapped[0].charCodeAt(0),mapped[1].charCodeAt(0)]:null});
}
assert.equal(spread.length,6,'JSON row spread floor; stop paired input materializing D83D/DE00');
const ordinary=[];
for(const state of ['live','partially_filled','new-state','filled','canceled','mmp_canceled']) {
 const receiver=new exported.Lookup();receiver.store={orders:[],marketsBySymbol:{BTCUSDT:{id:'BTC-USDT-SWAP'}}};receiver.xhr={get:async()=>({data:{data:[{state}]}})};
 receiver.mapOrders=()=>[{id:'mapped',status:'canceled'}];
 const result=await receiver.findOrderByClientOrderId('BTCUSDT','client');
 assert.equal(result.order.status,state==='filled'?'closed':'canceled','ordinary mapped-status fallback');ordinary.push({state,status:result.order.status});
}
assert.equal(ordinary.length,6,'independent ordinary status floor');
const envelopes=[];
for (const body of [{data:{0:{state:'filled'}}},{data:'ab'},{data:''},null,{data:'😀'}]) {
 const receiver=new exported.Lookup();receiver.store={orders:[],marketsBySymbol:{BTCUSDT:{id:'BTC-USDT-SWAP'}}};receiver.xhr={get:async()=>({data:body})};
 let mapped,error;receiver.mapOrders=rows=>{mapped=rows[0];return [];};
 try{await receiver.findOrderByClientOrderId('BTCUSDT','client');}catch(e){error=e;}
 if(body!==null&&body.data!=='😀') {
  const expected=typeof body.data==='object'?{state:'live'}:body.data==='ab'?{'0':'a',state:'live'}:undefined;
  assert.deepEqual(mapped,expected,'source data-index own values before serialization');
  assert.equal(error,undefined);
 }
 if(body===null){assert.equal(mapped,undefined);assert.equal(error.name,'TypeError');assert.equal(error.message,"Cannot read properties of null (reading 'data')");}
 if(body?.data==='😀'){assert.equal(mapped[0].charCodeAt(0),0xd83d);assert.equal(mapped[0].length,1);}
 envelopes.push({mapped:Boolean(mapped),row:body?.data==='😀'?null:mapped??null,error:error?.name??null});
}
assert.equal(envelopes.length,5,'data indexing floor; stop paired data materializing D83D');
const stored={id:'stored',symbol:'BTCUSDT',status:'open',amount:1,info:{clOrdId:'client'}};
const aliasReceiver=new exported.Lookup();aliasReceiver.store={orders:[stored],marketsBySymbol:{}};
aliasReceiver.xhr={get:async()=>{throw Error('store hit must not dispatch');}};
aliasReceiver.mapOrders=()=>{throw Error('store hit must not map');};
const found=await aliasReceiver.findOrderByClientOrderId('BTCUSDT','client');
assert.equal(found.order,stored,'store hit returns original native referent BEFORE serialization');
stored.status='closed';assert.equal(found.order.status,'closed','caller mutation is visible through source returned alias');
found.order.amount=17;assert.equal(stored.amount,17,'source returned alias mutation is visible through original');
const alias={sameReference:true,afterInput:found.order.status,afterOutput:stored.amount};
process.stdout.write(JSON.stringify({lookup,bills,spread,ordinary,envelopes,alias}));
