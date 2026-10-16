import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {root,ts,load} from './source.mjs';
const file=root+'packages/safe-cex/src/exchanges/mexc/mexc.ws-private.ts';
const src=fs.readFileSync(file,'utf8');
const ast=ts.createSourceFile(file,src,ts.ScriptTarget.Latest,true);
const owner=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text==='MexcPrivateWebsocket');
const names=['handleOrder','handlePosition','handlePlanOrder','handleStopOrder','applyStopLikeFrame'];
const methods=names.map(name=>{const n=owner.members.find(n=>n.name?.getText(ast)===name);assert(n,`SOURCE real ${name}`);return n.getText(ast);});
const utils=load('packages/safe-cex/src/exchanges/mexc/mexc.utils.ts');
const {DefaultStore}=load('packages/safe-cex/src/store/store.base.ts');
const {OrderStatus}=load('packages/safe-cex/src/types.ts');
const context={...utils,OrderStatus};
vm.runInNewContext(ts.transpileModule('class Holder{'+methods.join('\n')+'}\nglobalThis.Holder=Holder;',{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
const fixtures={
 order:{orderId:'row',symbol:'BTC_USDT',side:1,state:1,vol:2,dealVol:0,price:10,createTime:1000},
 position:{symbol:'BTC_USDT',positionType:1,holdVol:6,holdAvgPrice:20,createTime:1000},
 plan:{id:'row',symbol:'BTC_USDT',side:1,state:2,vol:2,triggerPrice:21,createTime:1000},
 stop:{stopPlanOrderId:'row',symbol:'BTC_USDT',positionType:1,state:2,vol:2,stopLossPrice:5,createTime:1000},
};
const output=[];
for(const kind of ['order','position','plan','stop']){
 for(const scenario of ['null-frame','null-row','later-null','contract-only','bad-symbol','empty-array','primitive-row','primitive-frame']){
  const store=new DefaultStore();
  store.update({markets:[utils.normalizeMarket({symbol:'BTC_USDT',contractSize:.5,maxLeverage:20})]},false);
  const getMarkets=Object.getOwnPropertyDescriptor(DefaultStore.prototype,'marketsBySymbol').get;
  let reads=0,first=0,later=0;const effects=[];
  Object.defineProperty(store,'marketsBySymbol',{get(){reads++;return getMarkets.call(store);}});
  store.subscribe(()=>{first++;});store.subscribe(()=>{later++;});
  const holder=new context.Holder();
  Object.assign(holder,{store,parent:{noteRejectedRegularOrder(id){effects.push('rejected:'+id);},markStopLikeOrderSeen(id){effects.push('seen:'+id);},isRecentlyCanceledStopLikeOrderId(id){effects.push('shield:'+id);return false;}}});
  const row=structuredClone(fixtures[kind]);let frame={data:[row]};
  if(scenario==='null-frame')frame=null;
  if(scenario==='null-row')frame={data:[null]};
  if(scenario==='later-null')frame={data:[row,null]};
  if(scenario==='contract-only'){delete row.symbol;row.contract='BTC_USDT';}
  if(scenario==='bad-symbol')row.symbol={toString:0,valueOf:0};
  if(scenario==='empty-array')frame={data:[]};
  if(scenario==='primitive-row')frame={data:[false]};
  if(scenario==='primitive-frame')frame=0;
  let error;try{holder[{order:'handleOrder',position:'handlePosition',plan:'handlePlanOrder',stop:'handleStopOrder'}[kind]](frame);}catch(e){error=e;}
  const wantReads={'null-frame':0,'null-row':1,'later-null':2,'contract-only':1,'bad-symbol':1,'empty-array':0,'primitive-row':1,'primitive-frame':1}[scenario];
  assert.equal(reads,wantReads,'SOURCE independent private row market getter calls '+kind+'/'+scenario);
  const writes=scenario==='contract-only'?1:0;
  assert.equal(first,writes,'SOURCE independent private row first subscriber calls');
  assert.equal(later,writes,'SOURCE independent private row later subscriber calls');
  assert.deepEqual(effects,scenario==='contract-only'&&['plan','stop'].includes(kind)?['seen:row']:[],'SOURCE no partial stop hooks before later null');
  if(['null-frame','null-row','later-null','bad-symbol'].includes(scenario)){
   assert.equal(error?.name,'TypeError','SOURCE private row exact error name');
   const message=scenario==='bad-symbol'?'Cannot convert object to primitive value':`Cannot read properties of null (reading '${scenario==='null-frame'?'data':'symbol'}')`;
   assert.equal(error.message,message,'SOURCE private row exact error message');
   assert.equal(store.orders.length,0,'SOURCE null/coercion refusal before order write');assert.equal(store.positions.length,0,'SOURCE null/coercion refusal before position write');
  }else{
   assert.equal(error,undefined,'SOURCE accepted private row');
   if(scenario==='contract-only'){
    if(kind==='position'){
     assert.equal(store.positions.length,1,'SOURCE contract-only position count');assert.equal(store.positions[0].contracts,6,'SOURCE outer position lookup uses only symbol');
    }else{
     assert.equal(store.orders.length,1,'SOURCE contract-only order count');assert.equal(store.orders[0].amount,2,'SOURCE outer order lookup uses only symbol');
    }
   }else{assert.equal(store.orders.length,0);assert.equal(store.positions.length,0);}
  }
  output.push({kind,scenario,reads,first,later,effects,error:error?{name:error.name,message:error.message}:null,orders:store.orders.map(o=>({id:o.id,amount:o.amount,status:o.status})),positions:store.positions.map(p=>({id:p.id,contracts:p.contracts}))});
 }
}
assert.equal(output.length,32,'SOURCE private row root floor');
assert.equal(output.reduce((n,r)=>n+r.reads,0),28,'SOURCE independent private row getter floor');
assert.equal(output.reduce((n,r)=>n+r.first,0),4,'SOURCE independent private row first subscriber floor');
assert.equal(output.reduce((n,r)=>n+r.later,0),4,'SOURCE independent private row later subscriber floor');
assert.equal(output.filter(r=>r.error!==null).length,16,'SOURCE independent private row refusal floor');
console.log(JSON.stringify(output));
