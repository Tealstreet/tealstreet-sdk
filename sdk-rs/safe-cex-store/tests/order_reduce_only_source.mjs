import assert from 'node:assert/strict';
import {sourceStore} from './store_source.mjs';
const {Store} = sourceStore();
const base = amount => ({id:'o', symbol:'BTC', amount, filled:0, remaining:amount, lastUpdate:1});
const put = (row, state) => { if(state) row.reduceOnly = state === 1 ? undefined : state === 3; };
const cases = [];
for (const method of ['addOrUpdateOrder','addOrUpdateOrders','updateOrder']) {
  for (let oldState=0; oldState<4; oldState++) for(let newState=0; newState<4; newState++) {
    const old=base(2), incoming=base(3); put(old,oldState); put(incoming,newState);
    const store=new Store(); store.addOrUpdateOrder(old);
    if(method==='updateOrder') store.updateOrder({id:'o'}, incoming);
    else store[method](method==='addOrUpdateOrders'?[incoming]:incoming);
    const row=store.orders[0];
    const state=!Object.hasOwn(row,'reduceOnly')?0:row.reduceOnly===undefined?1:row.reduceOnly?3:2;
    assert.equal(state,newState||oldState);
    cases.push({method,oldState,newState,state,wire:JSON.stringify(row)});
  }
}
assert.equal(cases.length,48);
process.stdout.write(JSON.stringify(cases));
