// Actual source Store operations, extending bitget's maintained six-price witness.
import {sourceStore} from './store_source.mjs';
import assert from 'node:assert/strict';
const control=process.argv.includes('--negative-source-control');
const {Store}=sourceStore(source=>{
 if(!control) return source;
 const mutated=source.replace("this.updateInArray('positions', idx, changes);", '');
 assert.notEqual(mutated, source, 'source mutation must reach actual updatePosition');
 return mutated;
});
const tag=n=>Object.is(n,-0)?'-0':String(n);
const aliases=new Store();
aliases.update({tickers:[{id:'duplicate',symbol:'A',last:1},{id:'duplicate',symbol:'B',last:2}]});
for(const index of [0,0.5,NaN,Infinity,-Infinity]) {
 for(const patch of [{},{info:{discard:true}}]) {
  aliases.applyTickerDeltas([[index,patch]],true);
  assert.equal(aliases.tickersById.duplicate.symbol,'B','empty delta preserves duplicate-ID alias');
 }
}
const rows=[];
for(const price of [0,-0,1,NaN,Infinity,-Infinity]) {
 const store=new Store();
 store.addOrUpdatePosition({id:'p',symbol:'BTC',side:'long',contracts:1,entryPrice:price,notional:1,leverage:1,unrealizedPnl:0,liquidationPrice:price,extra:{contractsNative:price}});
 store.updatePosition({id:'p'},{contracts:2});
 assert.equal(store.positions[0].contracts,2,'source numeric patch must execute');
 store.update({tickers:[{id:'t',symbol:'BTC',last:100,bid:90,ask:110,mark:99}]});
 store.updateTicker({id:'t'},{last:price,bid:price,ask:price,midPrice:price});
 const before=store.tickers[0].last;
 store.updateTicker({id:'t'},{bid:7});
 rows.push({input:tag(price),entry:tag(store.positions[0].entryPrice),nested:tag(store.positions[0].extra.contractsNative),contracts:store.positions[0].contracts,last:tag(before),retained:tag(store.tickers[0].last),bidAlias:tag(store.tickersBySymbol.BTC.bid),lastAlias:tag(store.tickersById.t.last),mark:store.tickers[0].mark});
}
process.stdout.write(JSON.stringify(rows));
