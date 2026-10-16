import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {root,ts,load} from './source.mjs';
const file=root+'packages/safe-cex/src/exchanges/mexc/mexc.ws-private.ts';
const src=fs.readFileSync(file,'utf8');const ast=ts.createSourceFile(file,src,ts.ScriptTarget.Latest,true);
const owner=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text==='MexcPrivateWebsocket');
const names=['handleOrder','handlePosition','handlePlanOrder','handleStopOrder','applyStopLikeFrame'];
const methods=names.map(name=>{const n=owner.members.find(n=>n.name?.getText(ast)===name);assert(n,`SOURCE real ${name}`);return n.getText(ast);});
const utils=load('packages/safe-cex/src/exchanges/mexc/mexc.utils.ts');
const {DefaultStore}=load('packages/safe-cex/src/store/store.base.ts');const {OrderStatus}=load('packages/safe-cex/src/types.ts');
const context={...utils,OrderStatus};vm.runInNewContext(ts.transpileModule('class Holder{'+methods.join('\n')+'}\nglobalThis.Holder=Holder;',{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
const symbols=[
 ['lone',String.raw`BTC_\ud800USDT`,[66,84,67,55296,85,83,68,84]],
 ['bmp',String.raw`BTC_\u00e9USDT`,[66,84,67,233,85,83,68,84]],
 ['astral',String.raw`BTC_\ud83d\ude03USDT`,[66,84,67,55357,56835,85,83,68,84]],
 ['space',String.raw`\ufeff BTC_USDT \u3000`,[65279,32,66,84,67,85,83,68,84,32,12288]],
];
const fixture=kind=>kind==='order'?{orderId:'row',symbol:'PRIVATE_SYMBOL',side:1,state:1,vol:2,price:10,createTime:1000}:kind==='position'?{symbol:'PRIVATE_SYMBOL',positionType:1,holdVol:6,holdAvgPrice:20,createTime:1000}:kind==='plan'?{id:'row',symbol:'PRIVATE_SYMBOL',side:1,state:1,vol:2,triggerPrice:21,createTime:1000}:{stopPlanOrderId:'row',symbol:'PRIVATE_SYMBOL',positionType:1,state:1,vol:2,stopLossPrice:5,createTime:1000};
const output=[];
for(const method of ['order','position','plan','stop','direct-plan','direct-stop'])for(const [label,escape,expectedUnits]of symbols){
 const kind=method.replace('direct-','');const store=new DefaultStore();
 let first=0,later=0,reads=0;const seen=[],shields=[];
 const getter=Object.getOwnPropertyDescriptor(DefaultStore.prototype,'marketsBySymbol').get;
 Object.defineProperty(store,'marketsBySymbol',{get(){reads++;return getter.call(store);}});
 store.subscribe(()=>{first++;});store.subscribe(()=>{later++;});
 const holder=new context.Holder();Object.assign(holder,{store,parent:{noteRejectedRegularOrder(){throw Error('unexpected reject');},markStopLikeOrderSeen(id){seen.push(id);},isRecentlyCanceledStopLikeOrderId(id){shields.push(id);return false;}}});
 const wire=JSON.stringify({data:[fixture(kind)]}).replace('PRIVATE_SYMBOL',escape);
 // Materialize the JSON escape before the actual method; the witness is a
 // program-visible stored symbol, not a JSON spelling or diagnostic string.
 const frame=JSON.parse(wire);
 if(method.startsWith('direct-'))holder.applyStopLikeFrame(frame,kind);
 else holder[{order:'handleOrder',position:'handlePosition',plan:'handlePlanOrder',stop:'handleStopOrder'}[kind]](frame);
 assert.equal(reads,1,'SOURCE independent Unicode getter count');assert.equal(first,1,'SOURCE independent Unicode first subscriber count');assert.equal(later,1,'SOURCE independent Unicode later subscriber count');
 const stored=kind==='position'?store.positions:store.orders;assert.equal(stored.length,1,'SOURCE independent accepted Unicode record count');
 const symbol=stored[0].symbol;assert.equal(typeof symbol,'string','SOURCE native stored symbol type before unit observation');const units=Array.from({length:symbol.length},(_,i)=>symbol.charCodeAt(i));
 assert.deepEqual(units,expectedUnits,'SOURCE program reads exact stored symbol UTF16 units');
 assert.equal(kind==='position'?stored[0].contracts:stored[0].amount,kind==='position'?6:2,'SOURCE independent Unicode normalized numeric value');
 assert.deepEqual(seen,['plan','stop'].includes(kind)?['row']:[],'SOURCE independent Unicode seen hook count');
 assert.deepEqual(shields,['plan','stop'].includes(kind)?['row']:[],'SOURCE independent Unicode shield hook count');
 output.push({method,label,wire,units,first,later,reads,seen,shields});
}
assert.equal(output.length,24,'SOURCE independent Unicode root floor');
assert.equal(output.reduce((n,r)=>n+r.first,0),24,'SOURCE independent Unicode first subscriber floor');
assert.equal(output.reduce((n,r)=>n+r.later,0),24,'SOURCE independent Unicode later subscriber floor');
assert.equal(output.reduce((n,r)=>n+r.reads,0),24,'SOURCE independent Unicode getter floor');
assert.equal(output.reduce((n,r)=>n+r.seen.length,0),16,'SOURCE independent Unicode seen call floor');
assert.equal(output.reduce((n,r)=>n+r.shields.length,0),16,'SOURCE independent Unicode shield call floor');
console.log(JSON.stringify(output));
