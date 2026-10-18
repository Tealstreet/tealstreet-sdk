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
const identity=[],deletion=[],overlays=[],integrity=[],generations=[],unicode=[],partialWrites=[];
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
process.stdout.write(JSON.stringify({identity,deletion,overlays,integrity,generations,unicode,bits,partialWrites}));
