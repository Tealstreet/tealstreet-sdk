// Later supplement over the actual shared Store; no copied mutation algorithm.
import assert from 'node:assert/strict';
import {sourceStore} from '../../safe-cex-store/tests/store_source.mjs';
const {Store}=sourceStore();
const market={id:'m',symbol:'BTC',base:'BTC',quote:'USDT',active:true,precision:{amount:1,price:1},limits:{amount:{min:1,max:9},leverage:{min:1,max:1}}};
const position={id:'p',symbol:'BTC',side:'long',entryPrice:1,notional:1,leverage:1,contracts:1,unrealizedPnl:0,liquidationPrice:0};
const tagged=value=>JSON.parse(JSON.stringify(value,(_,v)=>typeof v==='number'?(Object.is(v,-0)?'-0':String(v)):v));
const rows=[];
for(const n of [2.5,-0,NaN,Infinity,-Infinity]){
 const store=new Store();store.update({markets:[{...market}],positions:[{...position}]});
 const terms={underlying:'BTC',expiry:n,strike:n,right:'put',style:'european',settlement:'cash',premiumAsset:'USDT',settleAsset:'USDT'};
 const greeks={delta:n,gamma:3,vega:2,theta:-4,units:'test'};
 const snapshots=[];
 const snapshot=()=>snapshots.push(tagged({market:store.markets[0].option??null,position:store.positions[0].option??null}));
 store.updateMarket({id:'m'},{option:terms});snapshot();
 store.updatePosition({id:'p'},{option:{greeks}});snapshot();
 store.updatePositions([[{id:'p'},{option:{greeks:{...greeks,gamma:8}}}]]);snapshot();
 for(const method of ['addOrUpdatePosition','addOrUpdatePositions','addOrUpdatePositionsFull']){
  store[method](method==='addOrUpdatePosition'?{...position}:[{...position}]);snapshot();
 }
 store.update({markets:[{...market}]},true);snapshot();
 store.updatePosition({id:'p'},{option:{}});snapshot();
 assert.equal(snapshots.length,8,'eight actual shared mutation paths');
 assert.equal(store.positions[0].option!==undefined,true,'present empty option stays present');
 assert.equal(store.markets[0].option,terms,'omitted worker market field retains source terms');
 rows.push({input:Object.is(n,-0)?'-0':String(n),snapshots});
}
assert.equal(rows.length,5);process.stdout.write(JSON.stringify(rows));
