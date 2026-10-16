import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {root,ts,load} from './source.mjs';
process.env.TZ='UTC';
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
const fixture=(kind,id,side,time)=>{
 const base={symbol:'BTC_USDT',createTime:time,probe:{marker:7}};
 if(kind==='order')return {...base,orderId:id,side:1,state:1,vol:2,dealVol:0,price:10};
 if(kind==='position')return {...base,positionType:side,holdVol:2,holdAvgPrice:20};
 if(kind==='plan')return {...base,id,side:1,state:1,vol:2,triggerPrice:21};
 return {...base,stopPlanOrderId:id,positionType:side,state:1,vol:2,stopLossPrice:5};
};
const output=[];
for(const kind of ['order','position','plan','stop'])for(const scenario of ['live-market','info-alias']){
 const store=new DefaultStore();
 const market=size=>utils.normalizeMarket({symbol:'BTC_USDT',contractSize:size,maxLeverage:20});
 store.update({markets:[market(.5)]},false);
 const getMarkets=Object.getOwnPropertyDescriptor(DefaultStore.prototype,'marketsBySymbol').get;
 let reads=0;Object.defineProperty(store,'marketsBySymbol',{get(){reads++;return getMarkets.call(store);}});
 const seen=[],shields=[];
 const holder=new context.Holder();Object.assign(holder,{store,parent:{noteRejectedRegularOrder(){throw Error('unexpected reject hook');},markStopLikeOrderSeen(id){seen.push(id);},isRecentlyCanceledStopLikeOrderId(id){shields.push(id);return false;}}});
 const isLive=scenario==='live-market';
 const rows=isLive?[fixture(kind,'live-a',1,'2026-01-02T03:04:05'),fixture(kind,'live-b',2,'2026-01-02T03:04:06')]:[fixture(kind,'alias',1,1000)];
 const frame={data:rows};
 let clockCalls=0;
 const original=Date.parse;
 if(isLive)Date.parse=function(value){const parsed=original(value);clockCalls++;if(clockCalls===1)store.update({markets:[market(3)]},false);return parsed;};
 try{holder[{order:'handleOrder',position:'handlePosition',plan:'handlePlanOrder',stop:'handleStopOrder'}[kind]](frame);}finally{Date.parse=original;}
 const stored=kind==='position'?store.positions:store.orders;
 assert.equal(reads,isLive?2:1,'SOURCE independent reference market getter calls');
 assert.equal(clockCalls,isLive?2:0,'SOURCE independent real Date.parse calls');
 assert.equal(stored.length,isLive?2:1,'SOURCE independent reference stored count');
 const amounts=stored.map(v=>kind==='position'?v.contracts:v.amount);
 assert.deepEqual(amounts,isLive?[1,6]:[1],'SOURCE each row uses newly returned market');
 if(isLive)assert.equal(store.markets[0].contractSize,3,'SOURCE producer replaced actual market');
 else{
  assert.equal(stored[0].info.probe.marker,7,'SOURCE original nested info marker');
  frame.data[0].probe.marker=42;
  assert.equal(stored[0].info.probe.marker,42,'SOURCE caller mutation reaches stored nested info alias');
 }
 assert.deepEqual(seen,['plan','stop'].includes(kind)?(isLive?['live-a','live-b']:['alias']):[],'SOURCE reference independent seen calls');
 assert.deepEqual(shields,['plan','stop'].includes(kind)?(isLive?['live-a','live-b']:['alias']):[],'SOURCE reference independent shield calls');
 output.push({kind,scenario,reads,clockCalls,amounts,seen,shields,marker:stored[0].info.probe.marker});
}
assert.equal(output.length,8,'SOURCE reference root floor');
assert.equal(output.reduce((n,r)=>n+r.reads,0),12,'SOURCE independent reference getter floor');
assert.equal(output.reduce((n,r)=>n+r.clockCalls,0),8,'SOURCE independent real Date.parse floor');
console.log(JSON.stringify(output));
