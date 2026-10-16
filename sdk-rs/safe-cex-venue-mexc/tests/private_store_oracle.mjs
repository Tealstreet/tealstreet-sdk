import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {root,ts,load} from './source.mjs';
const file=root+'packages/safe-cex/src/exchanges/mexc/mexc.ws-private.ts';
const src=fs.readFileSync(file,'utf8');
const ast=ts.createSourceFile(file,src,ts.ScriptTarget.Latest,true);
const owner=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text==='MexcPrivateWebsocket');
assert(owner,'SOURCE real private class');
const names=['handleOrder','handlePosition','handlePlanOrder','handleStopOrder','applyStopLikeFrame'];
const methods=names.map(name=>{const method=owner.members.find(n=>n.name?.getText(ast)===name);assert(method,`SOURCE real method ${name}`);return method.getText(ast);});
const utils=load('packages/safe-cex/src/exchanges/mexc/mexc.utils.ts');
const {DefaultStore}=load('packages/safe-cex/src/store/store.base.ts');
const {OrderStatus}=load('packages/safe-cex/src/types.ts');
const ctx={...utils,OrderStatus};
vm.runInNewContext(ts.transpileModule('class Holder{'+methods.join('\n')+'}\nglobalThis.Holder=Holder;',{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,ctx);
const fixtures={
 order:[{orderId:'order-closed',symbol:'BTC_USDT',side:1,state:3,vol:2,dealVol:2,price:10,createTime:1000},{orderId:'order-canceled',symbol:'BTC_USDT',side:3,state:4,vol:4,dealVol:0,errorCode:510,price:20,createTime:1000}],
 position:[{positionId:'position-7',symbol:'BTC_USDT',positionType:1,holdVol:6,holdAvgPrice:20,leverage:4,createTime:1000}],
 plan:[{id:'plan-open',symbol:'BTC_USDT',side:1,state:1,vol:4,triggerPrice:21,createTime:1000},{id:'plan-terminal',symbol:'BTC_USDT',side:1,state:2,vol:2,triggerPrice:22,createTime:1000}],
 stop:[{stopPlanOrderId:'stop-open',symbol:'BTC_USDT',positionType:1,state:1,vol:4,stopLossPrice:5,createTime:1000},{stopPlanOrderId:'stop-terminal',symbol:'BTC_USDT',positionType:1,state:2,vol:2,stopLossPrice:6,createTime:1000}]
};
const definitions=[['order',1],['order',2],['order',3],['order',0],['position',1],['position',0],['plan',1],['plan',0],['stop',1],['stop',0],['direct-plan',1],['direct-stop',1]];
const output=[];
for(const [method,failAt] of definitions){
 const kind=method.replace('direct-','');
 const store=new DefaultStore();
 store.update({markets:[utils.normalizeMarket({symbol:'BTC_USDT',contractSize:.5,maxLeverage:20})]},false);
 const writes=[];
 const addOrders=store.addOrUpdateOrders;
 store.addOrUpdateOrders=orders=>{writes.push(['orders',...orders.map(o=>o.id)]);return addOrders(orders);};
 const updateStatus=store.updateOrderStatus;
 store.updateOrderStatus=(order,status)=>{writes.push(['status',order.id,status]);return updateStatus(order,status);};
 const addPositions=store.addOrUpdatePositions;
 store.addOrUpdatePositions=positions=>{writes.push(['positions',...positions.map(p=>p.id)]);return addPositions(positions);};
 const getMarkets=Object.getOwnPropertyDescriptor(DefaultStore.prototype,'marketsBySymbol').get;
 let marketReads=0;
 Object.defineProperty(store,'marketsBySymbol',{get(){marketReads++;return getMarkets.call(store);}});
 const origin=Object.assign(new Error('private subscriber origin'),{code:313});
 let first=0,later=0;
 const trace=[],seen=[],shields=[],rejected=[];
 store.subscribe(()=>{first++;trace.push('store');if(first===failAt)throw origin;});
 store.subscribe(()=>{later++;});
 const holder=new ctx.Holder();
 Object.assign(holder,{store,parent:{
  noteRejectedRegularOrder(id,code){rejected.push([id,code]);trace.push('reject:'+id);},
  markStopLikeOrderSeen(id){seen.push(id);trace.push('seen:'+id);},
  isRecentlyCanceledStopLikeOrderId(id){shields.push(id);trace.push('shield:'+id);return id===kind+'-open';}
 }});
 const frame={data:structuredClone(fixtures[kind])};
 let error;
 try{
  if(method.startsWith('direct-'))holder.applyStopLikeFrame(frame,kind);
  else holder[{order:'handleOrder',position:'handlePosition',plan:'handlePlanOrder',stop:'handleStopOrder'}[kind]](frame);
 }catch(e){error=e;}
 const expectedCalls=kind==='order'?(failAt||3):1;
 assert.equal(marketReads,kind==='position'?1:2,'SOURCE independent per-row market getter calls');
 assert.equal(first,expectedCalls,'SOURCE independent private first-subscriber calls '+method+failAt);
 assert.equal(later,failAt?failAt-1:expectedCalls,'SOURCE independent private later-subscriber calls '+method+failAt);
 assert.equal(error,failAt?origin:undefined,'SOURCE exact private subscriber error identity '+method+failAt);
 if(failAt)assert.equal(error.code,313,'SOURCE opaque subscriber code remains a direct Error property before JSON');
 if(kind==='order'){
  assert.deepEqual(store.orders.map(o=>o.id),['order-closed','order-canceled'],'SOURCE committed order IDs');
  assert.deepEqual(store.orders.map(o=>o.status),['closed','canceled'],'SOURCE committed order statuses');
  assert.deepEqual(store.orders.map(o=>o.amount),[1,2],'SOURCE committed native order amounts');
  assert.deepEqual(writes,[['orders','order-closed','order-canceled'],['status','order-closed','closed'],['status','order-canceled','canceled']].slice(0,failAt||3),'SOURCE exact terminal write order and cutoff');
  assert.deepEqual(rejected,failAt?[]:[['order-canceled',510]],'SOURCE rejection hook cutoff');
 }else if(kind==='position'){
  assert.equal(store.positions.length,1,'SOURCE committed position count');
  assert.equal(store.positions[0].id,'BTCUSDT:long','SOURCE independent position ID');
  assert.equal(store.positions[0].side,'long','SOURCE independent position side');
  assert.deepEqual(writes,[['positions','BTCUSDT:long']],'SOURCE exact position write');
  assert.equal(store.positions[0].positionMode,'hedged','SOURCE handler uses default position mode');
  assert.equal(store.positions[0].contracts,3,'SOURCE committed native contracts');
  assert.equal(store.positions[0].notional,60,'SOURCE committed native notional');
 }else{
  assert.deepEqual(seen,[kind+'-open',kind+'-terminal'],'SOURCE stop seen-before-filter IDs');
  assert.deepEqual(shields,[kind+'-open'],'SOURCE shield only open row');
  assert.deepEqual(trace,['seen:'+kind+'-open','seen:'+kind+'-terminal','shield:'+kind+'-open','store'],'SOURCE stop full effect order');
  assert.deepEqual(store.orders.map(o=>o.id),[kind+'-terminal'],'SOURCE honors terminal filtered order');
  assert.equal(store.orders[0].status,'canceled','SOURCE committed stop terminal status');
  assert.equal(store.orders[0].amount,1,'SOURCE committed stop terminal amount');
  assert.equal(store.orders[0].info[kind==='plan'?'__mexcPlanOrder':'__mexcStopOrder'],true,'SOURCE exact plan/stop info discriminator');
  assert.equal(Object.hasOwn(store.orders[0].info,kind==='plan'?'__mexcStopOrder':'__mexcPlanOrder'),false,'SOURCE opposite plan/stop discriminator absent');
  assert.deepEqual(writes,[['orders',kind+'-terminal']],'SOURCE exact stop terminal write');
 }
 output.push({method,failAt,first,later,marketReads,writes,trace,seen,shields,rejected,error:error?{message:error.message,code:error.code}:null,orders:store.orders.map(o=>({id:o.id,status:o.status,amount:o.amount})),positions:store.positions.map(p=>({id:p.id,contracts:p.contracts,positionMode:p.positionMode,notional:p.notional}))});
}
assert.equal(output.length,12,'SOURCE private Store notification root floor');
assert.equal(output.reduce((n,r)=>n+r.first,0),17,'SOURCE private independent first subscriber floor');
assert.equal(output.reduce((n,r)=>n+r.later,0),9,'SOURCE private independent later subscriber floor');
console.log(JSON.stringify(output));
