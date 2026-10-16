import assert from 'node:assert/strict';
import fs from 'node:fs';
import {sourceStore} from './store_source.mjs';
const context=sourceStore(),store=new context.Store();let notifications=0;store.subscribe(()=>notifications++);
const order=(id,status='open',symbol='BTC')=>({id,status,symbol,type:'limit',side:'buy',price:10,amount:1,filled:0,remaining:1,reduceOnly:false,lastUpdate:0});
const rows=[];
function step(op,args,now=context.now) {
 const input=structuredClone(args);context.now=now;store[op](...args);
 const snapshot=store.serialize();
 rows.push({op,args:input,now,expected:{orders:snapshot.orders,updateIds:snapshot.updateIds,requestStats:snapshot.requestStats,socketHealth:snapshot.socketHealth},notifications});
 rows[rows.length-1]=structuredClone(rows[rows.length-1]);
}
step('addOrder',[{...order('a'),info:{old:true}}]);step('addOrUpdateOrder',[order('a')]);
step('addOrder',[order('b','open','ETH')]);
step('markOrdersTerminal',[[{id:'a'}],'closed'],100);
step('addOrder',[order('a')],200);step('addOrUpdateOrder',[order('a')]);
step('addOrUpdateOrders',[[order('a'),order('c')]]);
step('addOrUpdateOrdersFull',[[order('a'),order('b','open','ETH')]]);
step('setOrdersForSymbol',['BTC',[order('a'),order('d')]]);
step('update',[{orders:[order('a')]},false]);
step('update',[{orders:[order('a')],updateIds:{orders:50}},true]);
step('update',[{orders:[order('new')],updateIds:{orders:50}},true]);
step('update',[{orders:[order('a')],updateIds:{orders:2}},true]);
step('update',[{orders:[order('a')]},true]);
step('markOrdersTerminal',[[{id:'a'}],'canceled'],300);
step('addOrUpdateOrdersFull',[[order('a')]],60299);
step('addOrUpdateOrdersFull',[[order('a')]],60300);
step('markOrdersTerminal',[[{id:'absent'}],'closed'],70000);
step('addOrUpdateOrders',[[order('absent')]]);step('addOrder',[order('absent')]);
step('removeOrders',[[{id:'a'}]]);step('removeOrder',[{id:'absent'}]);
step('reset',[]);
step('addOrUpdateOrdersFull',[[order('dup'),{...order('dup'),price:20}]]);
step('addOrUpdateOrders',[[{...order('dup'),price:30},order('other')]]);
step('addOrUpdateOrders',[[{...order('dup'),price:40}]]);
step('updateOrdersStatus',[[{id:'dup'}],'closed']);
step('addOrUpdateOrders',[[order('dup')]]); // ordinary status is not remembered
step('markOrdersTerminal',[[{id:'dup'}],'closed'],80000);
step('addOrUpdateOrdersFull',[[order('dup')]],79999); // backwards clock retained
step('reset',[]);step('addOrder',[order('dup')]);
// Corpus pins prevent empty or truncated snapshots from turning this gate green.
assert.equal(rows.length, 32);
const path=new URL('./terminal_oracle.json',import.meta.url),output=JSON.stringify(rows,null,2)+'\n';
if(process.argv.includes('--check')) {if(fs.readFileSync(path,'utf8')!==output)throw Error('terminal oracle drift');}else fs.writeFileSync(path,output);
console.log(`${rows.length} terminal source-oracle frames verified`);
