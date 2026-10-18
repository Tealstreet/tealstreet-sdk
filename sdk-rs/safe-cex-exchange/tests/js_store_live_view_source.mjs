import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import {createRequire} from 'node:module';
import {fileURLToPath} from 'node:url';
import {ts} from '../../safe-cex-store/tests/source_loader.mjs';
const root = fileURLToPath(new URL('../../../', import.meta.url));
const require = createRequire(path.join(root,'package.json'));
const modules = new Map();
// Execute the maintained Store and its actual relative/aliased TS dependencies.
// Only the external clock transport is disabled; no mutation code is replaced.
function load(filename) {
  if (modules.has(filename)) return modules.get(filename).exports;
  const module = {exports:{}};
  modules.set(filename,module);
  const code = ts.transpileModule(fs.readFileSync(filename,'utf8'), {compilerOptions:{
    target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS,esModuleInterop:true,
  }}).outputText;
  const localRequire = specifier => {
    if (specifier === 'axios') return {get:async()=>{throw Error('oracle network disabled');}};
    if (specifier === '../transport/registry') return {withPublicCorsProxy:()=>{throw Error('oracle network disabled');}};
    if (specifier.startsWith('@tealstreet/')) return load(path.join(root,'packages',specifier.slice('@tealstreet/'.length)+'.ts'));
    if (specifier.startsWith('.')) return load(path.resolve(path.dirname(filename),specifier+'.ts'));
    return require(specifier);
  };
  vm.runInThisContext('(function(require,module,exports){'+code+'\n})',{filename})(localRequire,module,module.exports);
  return module.exports;
}
const {DefaultStore} = load(path.join(root,'packages/safe-cex/src/store/store.base.ts'));

const market={id:'m',symbol:'BTCUSD',base:'BTC',quote:'USD',active:true,
 precision:{amount:1,price:0.5},limits:{amount:{min:1,max:1000},leverage:{min:1,max:100}}};
const ticker={id:'t',symbol:'BTCUSD',bid:1,ask:2,last:1,mark:1,index:1,
 percentage:0,openInterest:0,fundingRate:0,fundingTime:0,volume:0,quoteVolume:0};
const names=['marketsById','marketsBySymbol','tickersById','tickersBySymbol'];
function fixture(name){
 const store=new DefaultStore();store.update({markets:[structuredClone(market)],tickers:[structuredClone(ticker)]});
 const markets=name.startsWith('markets'),id=markets?'m':'t',key=name.endsWith('ById')?id:'BTCUSD';
 const field=markets?'active':'bid',changed=markets?false:7;
 const notices={count:0};store.subscribe(()=>{notices.count++});
 return {store,markets,id,key,field,changed,notices,view:store[name],other:store[markets?(name.endsWith('ById')?'marketsBySymbol':'marketsById'):(name.endsWith('ById')?'tickersBySymbol':'tickersById')],
  state:store[markets?'markets':'tickers'],write(changes){store[markets?'updateMarket':'updateTicker']({id},changes??{[field]:changed})}};
}
function attempt(action){try{action();return null}catch(e){return e.name}}
function domain(view,key){const d=Object.getOwnPropertyDescriptor(view,key);return {own:Object.hasOwn(view,key),keys:Object.keys(view),descriptor:d?{enumerable:d.enumerable,writable:d.writable,configurable:d.configurable}:null,json:JSON.stringify(view)}}
const identity=[],deletion=[],overlays=[],integrity=[],generations=[],unicode=[],partialWrites=[],reentry=[];
for(const name of names){
 {
  const f=fixture(name),row=f.view[f.key];
  const initial={getter:f.store[name]===f.view,lookup:f.view[f.key]===row,other:Object.values(f.other)[0]===row,state:f.state[0]===row};
  f.write();identity.push({name,initial,sameRow:f.view[f.key]===row,lateValue:row[f.field]});
  assert.deepEqual(initial,{getter:true,lookup:true,other:true,state:true});assert.equal(row[f.field],f.changed);
 }
 {
  const f=fixture(name),row=f.view[f.key];delete f.view[f.key];
  const before=domain(f.view,f.key);const freshSame=f.store[name]===f.view;
  assert.equal(before.own,false);assert.equal(freshSame,true);assert.equal(Object.values(f.other)[0],row);assert.equal(f.state[0],row);
  f.write();const after=domain(f.view,f.key),resurrected=!(name==='tickersById');
  assert.equal(after.own,resurrected);assert.equal(row[f.field],resurrected?f.changed:(f.markets?true:1));
  deletion.push({name,before,freshSame,resurrected,after,rowValue:row[f.field]});
 }
 for(const how of ['assign','defineHidden']){
  const f=fixture(name),row=f.view[f.key],replacement={...row,[f.field]:f.markets?false:19};
  if(how==='assign')f.view[f.key]=replacement;
  else Object.defineProperty(f.view,f.key,{value:replacement,writable:true,configurable:true,enumerable:false});
  const before=domain(f.view,f.key);assert.equal(f.store[name][f.key],replacement);
  assert.equal(Object.values(f.other)[0],row);assert.equal(f.state[0],row);
  f.write();const tickerId=name==='tickersById';
  assert.equal(f.view[f.key],tickerId?replacement:row);
  assert.equal(f.state[0][f.field],tickerId?1:f.changed);
  assert.equal(Object.values(f.other)[0],tickerId?replacement:row);
  if(how==='defineHidden')assert.equal(Object.getOwnPropertyDescriptor(f.view,f.key).enumerable,false);
  overlays.push({name,how,before,after:domain(f.view,f.key),replacementSurvives:tickerId,stateValue:f.state[0][f.field]});
 }
 {
  const f=fixture(name),row=f.view[f.key],last=f.markets?'quote':'ask',lastValue=row[last];
  Object.defineProperty(row,last,{writable:false});
  const error=attempt(()=>f.write({[f.field]:f.changed,[last]:f.markets?'CHANGED':9}));
  assert.equal(error,'TypeError');assert.equal(row[f.field],f.changed);assert.equal(row[last],lastValue);
  assert.equal(f.notices.count,0);assert.equal(f.state[0],row);assert.equal(Object.values(f.other)[0],row);
  partialWrites.push({name,error,firstValue:row[f.field],lastValue,notices:f.notices.count});
 }
 for(const operation of ['freeze','seal','preventExtensions'])for(const subject of ['map','row']){
  const f=fixture(name),row=f.view[f.key],target=subject==='map'?f.view:row;
  Object[operation](target);
  const mutationError=attempt(()=>f.write());
  const expectedError=operation==='freeze'?'TypeError':null;
  assert.equal(mutationError,expectedError);
  const mutationNotices=f.notices.count;assert.equal(mutationNotices,operation==='freeze'?0:1);
  assert.equal(row[f.field],operation==='freeze'&&subject==='row'?(f.markets?true:1):f.changed);
  const insertionError=attempt(()=>{f.view.extra=row});
  assert.equal(insertionError,subject==='map'?'TypeError':null);
  const deleteError=attempt(()=>{delete f.view[f.key]});
  assert.equal(deleteError,subject==='map'&&operation!=='preventExtensions'?'TypeError':null);
  const beforeFull=row[f.field];
  const fullError=attempt(()=>f.store.update({[f.markets?'markets':'tickers']:[structuredClone(f.markets?market:ticker)]}));
  assert.equal(fullError,null);assert.equal(f.notices.count,mutationNotices+1);assert.notEqual(f.store[name],f.view);assert.notEqual(f.store[name][f.key],row);
  assert.equal(row[f.field],beforeFull);
  integrity.push({name,operation,subject,mutationError,mutationNotices,rowValue:row[f.field],insertionError,deleteError,fullError});
 }
 {
  const f=fixture(name),oldMap=f.view,oldRow=f.view[f.key];
  f.store.update({[f.markets?'markets':'tickers']:[structuredClone(f.markets?market:ticker)]});
  const newMap=f.store[name],newRow=newMap[f.key];
  assert.notEqual(newMap,oldMap);assert.notEqual(newRow,oldRow);
  f.write();assert.equal(newRow[f.field],f.changed);assert.equal(oldRow[f.field],f.markets?true:1);
  f.store.reset();assert.notEqual(f.store[name],newMap);assert.deepEqual(Object.keys(f.store[name]),[]);
  assert.equal(newMap[f.key],newRow);assert.equal(oldMap[f.key],oldRow);
  generations.push({name,newMap:true,newRow:true,oldValue:oldRow[f.field],newValue:newRow[f.field],resetKeys:Object.keys(f.store[name])});
 }
 {
  const f=fixture(name),row=f.view[f.key],key='extra\uD800';f.view[key]=row;
  assert.equal(f.store[name][key],row);assert.equal(Object.keys(f.view).includes(key),true);
  assert.equal(Object.getOwnPropertyDescriptor(f.view,key).value,row);
  delete f.view[key];assert.equal(Object.hasOwn(f.store[name],key),false);
  unicode.push({name,keyUnits:Array.from(key,c=>c.charCodeAt(0)),afterDelete:Object.hasOwn(f.view,key)});
 }
}
const bits=[];
for(const value of [-0,NaN,Infinity,-Infinity]){
 const f=fixture('tickersById'),row=f.view[f.key];f.write({bid:value});
 assert.equal(Object.is(row.bid,value),true);assert.equal(f.view[f.key],row);
 bits.push({value:Object.is(value,-0)?'-0':String(value),same:true});
}
// Evaluate object bases at their source expression sites, independently of
// getters/setters that publish a different Store generation during evaluation.
{
 const f=fixture('tickersById'),oldMap=f.view,oldRow=oldMap.t;
 const selector={get id(){f.store.update({tickers:[{...ticker,bid:99}]});return 't'}};
 f.store.updateTicker(selector,{bid:7});
 assert.equal(oldRow.bid,7);assert.equal(f.store.tickersById.t,oldRow);
 assert.equal(f.store.tickers[0].bid,99);assert.notEqual(f.store.tickersById,oldMap);
 reentry.push({case:'base-before-selector-getter',oldBid:oldRow.bid,stateBid:f.store.tickers[0].bid,mapUsesOldRow:f.store.tickersById.t===oldRow});
}
{
 const f=fixture('tickersById'),oldMap=f.view,oldRow=oldMap.t;let retainedBid=1;
 Object.defineProperty(oldRow,'bid',{enumerable:true,configurable:true,get(){return retainedBid},set(value){retainedBid=value;f.store.update({tickers:[{...ticker,bid:99}]})}});
 f.write();
 assert.equal(oldRow.bid,7);assert.equal(f.store.tickersById.t,oldRow);
 assert.equal(f.store.tickersBySymbol.BTCUSD,oldRow);assert.equal(f.store.tickers[0].bid,99);
 reentry.push({case:'setter-republishes-before-map-statements',oldBid:oldRow.bid,stateBid:f.store.tickers[0].bid,mapUsesOldRow:true});
}
{
 const f=fixture('tickersById'),oldMap=f.view,oldRow=oldMap.t;
 Object.defineProperty(oldRow,'id',{enumerable:true,configurable:true,get(){f.store.update({tickers:[{...ticker,bid:99}]});return 't'}});
 f.write();
 assert.equal(oldRow.bid,7);assert.notEqual(f.store.tickersById.t,oldRow);
 assert.equal(f.store.tickersById.t.bid,99);assert.equal(f.store.tickersBySymbol.BTCUSD,oldRow);
 assert.equal(oldMap.t,oldRow);assert.equal(f.store.tickers[0].bid,99);
 reentry.push({case:'assignment-base-before-id-getter',oldBid:oldRow.bid,stateBid:f.store.tickers[0].bid,idMapUsesNewRow:true,symbolMapUsesOldRow:true});
}
const balances=[];
{
 const store=new DefaultStore(),captured=store.balance;
 // A captured native referent is read lazily, without a Store-wide borrow.
 captured.total=7;
 assert.equal(store.balance,captured);assert.equal(captured.total,7);
 store.update({balance:{used:1,free:2,total:3,upnl:4}});
 assert.notEqual(store.balance,captured);assert.equal(captured.total,7);
 const replaced=store.balance;
 store.reset();assert.notEqual(store.balance,replaced);assert.equal(replaced.total,3);
 balances.push({case:'captured-generation',granular:7,replaced:3,reset:store.balance.total});
}
{
 let store=new DefaultStore();const captured=store.balance,cause={code:'opaque\uD800'};
 Object.defineProperty(captured,'total',{enumerable:true,configurable:true,get(){throw cause}});
 // Retaining the reference does not read its fields. Each later projection
 // reaches the same getter, even after the Store has replaced its balance.
 assert.equal(store.balance,captured);
 for(let pass=0;pass<2;pass++){
  let thrown;try{({...captured})}catch(error){thrown=error}
  assert.equal(thrown,cause);
  if(pass===0)store.reset();
 }
 store=null;
 let thrown;try{captured.total}catch(error){thrown=error}
 assert.equal(thrown,cause);
 balances.push({case:'getter-cause-after-owner-drop',sameCause:true});
}
const ordering=[];
for(const throws of [false,true]){
 const store=new DefaultStore(),events=[],cause={opaque:'\uD800'};
 const row={id:'t',symbol:'BTCUSD',get info(){events.push('row.info');return undefined}};
 const changes={get tickers(){events.push('changes.tickers');return [row]},get latency(){events.push('changes.latency');if(throws)throw cause;return 7}};
 let error;try{store.update(changes)}catch(e){error=e}
 const expected=throws?['changes.tickers','changes.tickers','changes.latency']:['changes.tickers','changes.tickers','changes.latency','changes.tickers','row.info'];
 assert.deepEqual(events,expected);assert.equal(error,throws?cause:undefined);
 ordering.push({case:'spread-before-strip',throws,events,sameCause:error===cause});
}
{
 const store=new DefaultStore();store.update({orders:[{id:'a'},{id:'b'}]});
 let expanded=false;const selectors=[{get id(){if(!expanded){expanded=true;selectors.push({id:'b'})}return 'a'}}];
 store.removeOrders(selectors);assert.deepEqual(store.orders,[]);assert.equal(selectors.length,2);
 ordering.push({case:'dynamic-id-lookup-length',remaining:store.orders,selectorCount:selectors.length});
}
for(const packed of [false,true]){
 const f=fixture('tickersById'),deltas=[];
 const append=()=>{if(deltas.length===1)deltas.push(packed?[0,1,9]:[0,{bid:9}]);return packed?7:0};
 deltas.push(packed?[0,1]:[]);
 Object.defineProperty(deltas[0],packed?'2':'0',{enumerable:true,configurable:true,get:append});
 if(!packed)deltas[0][1]={bid:7};
 f.store[packed?'applyPackedTickerDeltas':'applyTickerDeltas'](deltas);
 assert.equal(f.view.t.bid,9);ordering.push({case:packed?'live-packed-length':'live-legacy-length',bid:f.view.t.bid});
}
{
 const store=new DefaultStore(),a={id:'a',symbol:'A',active:true},b={id:'b',symbol:'B',active:true};store.update({markets:[a]});
 const incoming=[{id:'a',symbol:'A',get active(){if(incoming.length===1){store.markets.push(b);incoming.push({...b,active:false})}return false}}];
 store.update({markets:incoming},true);assert.equal(b.active,false);
 ordering.push({case:'live-worker-patch-length',active:b.active});
}
{
 const store=new DefaultStore(),positions=[{id:'a',contracts:1},{id:'b',contracts:1}];let appended=false;store.update({positions});
 Object.defineProperty(positions[0],'id',{enumerable:true,configurable:true,get(){if(!appended){appended=true;positions.push({id:'c',contracts:1})}return 'a'}});
 store.updatePositions([[{id:'c'},{contracts:7}],[{id:'x'},{contracts:8}]]);
 assert.equal(positions[2].contracts,7);ordering.push({case:'live-id-index-length',contracts:positions[2].contracts});
}
{
 const store=new DefaultStore(),cause={opaque:'\uD800'},replacement={id:'c',contracts:1};
 const first={contracts:1},poison={id:'b',get contracts(){throw cause}};
 const positions=[first,poison];Object.defineProperty(first,'id',{enumerable:true,configurable:true,get(){positions[1]=replacement;return 'a'}});
 store.update({positions});assert.equal(store.positions[1],replacement);
 ordering.push({case:'audit-reached-row-order',replacement:true});
}
{
 const store=new DefaultStore(),cause={opaque:'\uD800'};let idReads=0,contractsReads=0,notices=0;
 const row={get id(){if(++idReads===2)throw cause;return 'negative'},get contracts(){contractsReads++;return -1}};
 store.subscribe(()=>{notices++});let error;try{store.update({positions:[row]})}catch(e){error=e}
 assert.equal(error,cause);assert.equal(idReads,2);assert.equal(contractsReads,1);assert.equal(notices,0);assert.equal(store.updateIds.positions,0);
 ordering.push({case:'audit-log-expression-order',idReads,contractsReads,notices,revision:store.updateIds.positions,sameCause:error===cause});
}
const defaultSnapshot=new DefaultStore().serialize();
const defaultProjection={starkConfigOwn:Object.hasOwn(defaultSnapshot,'starkConfig'),starkConfigUndefined:defaultSnapshot.starkConfig===undefined,wire:JSON.parse(JSON.stringify(defaultSnapshot))};
assert.equal(defaultProjection.starkConfigOwn,true);assert.equal(defaultProjection.starkConfigUndefined,true);assert.equal(Object.hasOwn(defaultProjection.wire,'starkConfig'),false);
process.stdout.write(JSON.stringify({identity,deletion,overlays,integrity,generations,unicode,bits,partialWrites,reentry,balances,ordering,defaultProjection}));
