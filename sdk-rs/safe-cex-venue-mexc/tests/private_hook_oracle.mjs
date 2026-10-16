import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {root,ts,load} from './source.mjs';
const file=root+'packages/safe-cex/src/exchanges/mexc/mexc.ws-private.ts';
const src=fs.readFileSync(file,'utf8');
const ast=ts.createSourceFile(file,src,ts.ScriptTarget.Latest,true);
const owner=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text==='MexcPrivateWebsocket');
const names=['handleOrder','handlePlanOrder','handleStopOrder','applyStopLikeFrame'];
const methods=names.map(name=>{const n=owner.members.find(n=>n.name?.getText(ast)===name);assert(n,`SOURCE real ${name}`);return n.getText(ast);});
const utils=load('packages/safe-cex/src/exchanges/mexc/mexc.utils.ts');
const {DefaultStore}=load('packages/safe-cex/src/store/store.base.ts');
const {OrderStatus}=load('packages/safe-cex/src/types.ts');
const context={...utils,OrderStatus};
vm.runInNewContext(ts.transpileModule('class Holder{'+methods.join('\n')+'}\nglobalThis.Holder=Holder;',{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
const output=[];
for(const kind of ['order','plan','stop'])for(const hook of (kind==='order'?['reject']:['seen','shield']))for(const failAt of [1,2]){
 const store=new DefaultStore();
 store.update({markets:[utils.normalizeMarket({symbol:'BTC_USDT',contractSize:.5,maxLeverage:20})]},false);
 const getter=Object.getOwnPropertyDescriptor(DefaultStore.prototype,'marketsBySymbol').get;
 let reads=0,first=0,later=0;Object.defineProperty(store,'marketsBySymbol',{get(){reads++;return getter.call(store);}});
 const trace=[];
 store.subscribe(()=>{first++;trace.push('store');});store.subscribe(()=>{later++;});
 const origin=Object.assign(new Error('private parent hook failure'),{code:919});
 const seen=[],shields=[],rejected=[];
 const holder=new context.Holder();Object.assign(holder,{store,parent:{
  noteRejectedRegularOrder(id,code){rejected.push([id,code]);trace.push('reject:'+id);if(hook==='reject'&&rejected.length===failAt)throw origin;},
  markStopLikeOrderSeen(id){seen.push(id);trace.push('seen:'+id);if(hook==='seen'&&seen.length===failAt)throw origin;},
  isRecentlyCanceledStopLikeOrderId(id){shields.push(id);trace.push('shield:'+id);if(hook==='shield'&&shields.length===failAt)throw origin;return false;},
 }});
 const rows=['a','b'].map((id,i)=>{
  const row={symbol:'BTC_USDT',createTime:1000,vol:2*(i+1)};
  if(kind==='order')return {...row,orderId:id,side:1,state:4,errorCode:510+i,dealVol:0,price:10};
  if(kind==='plan')return {...row,id,side:1,state:1,triggerPrice:21};
  return {...row,stopPlanOrderId:id,positionType:1,state:1,stopLossPrice:5};
 });
 let error;try{holder[{order:'handleOrder',plan:'handlePlanOrder',stop:'handleStopOrder'}[kind]]({data:rows});}catch(e){error=e;}
 assert.equal(error,origin,'SOURCE parent hook exact thrown identity');assert.equal(error.code,919,'SOURCE parent hook original code');
 assert.equal(reads,2,'SOURCE independent parent-hook per-row getter floor');
 assert.equal(first,kind==='order'?3:0,'SOURCE parent-hook first subscriber count');
 assert.equal(later,kind==='order'?3:0,'SOURCE parent-hook later subscriber count');
 const wantIds=['a','b'].slice(0,failAt);
 assert.deepEqual(rejected,kind==='order'?wantIds.map((id,i)=>[id,510+i]):[],'SOURCE rejected hook cutoff');
 assert.deepEqual(seen,hook==='seen'?wantIds:(hook==='shield'?['a','b']:[]),'SOURCE mark hook cutoff');
 assert.deepEqual(shields,hook==='shield'?wantIds:[],'SOURCE shield hook cutoff');
 const expectedTrace=kind==='order'?['store','store','store',...wantIds.map(id=>'reject:'+id)]:hook==='seen'?wantIds.map(id=>'seen:'+id):['seen:a','seen:b',...wantIds.map(id=>'shield:'+id)];
 assert.deepEqual(trace,expectedTrace,'SOURCE independent full hook/Store call order');
 assert.equal(store.orders.length,kind==='order'?2:0,'SOURCE parent hook throws before stop Store write');
 if(kind==='order'){
  assert.deepEqual(store.orders.map(o=>o.id),['a','b'],'SOURCE already committed regular IDs');
  assert.deepEqual(store.orders.map(o=>o.status),['canceled','canceled'],'SOURCE already committed regular statuses');
  assert.deepEqual(store.orders.map(o=>o.amount),[1,2],'SOURCE already committed regular amounts');
 }
 output.push({kind,hook,failAt,reads,first,later,seen,shields,rejected,trace,error:{message:error.message,code:error.code},orders:store.orders.map(o=>({id:o.id,amount:o.amount,status:o.status}))});
}
assert.equal(output.length,10,'SOURCE independent parent-hook root floor');
assert.equal(output.reduce((n,r)=>n+r.reads,0),20,'SOURCE independent parent-hook getter floor');
assert.equal(output.reduce((n,r)=>n+r.first,0),6,'SOURCE independent parent-hook first subscriber floor');
assert.equal(output.reduce((n,r)=>n+r.later,0),6,'SOURCE independent parent-hook later subscriber floor');
assert.equal(output.reduce((n,r)=>n+r.rejected.length,0),3,'SOURCE independent reject callback floor');
assert.equal(output.reduce((n,r)=>n+r.seen.length,0),14,'SOURCE independent seen callback floor');
assert.equal(output.reduce((n,r)=>n+r.shields.length,0),6,'SOURCE independent shield callback floor');
console.log(JSON.stringify(output));
