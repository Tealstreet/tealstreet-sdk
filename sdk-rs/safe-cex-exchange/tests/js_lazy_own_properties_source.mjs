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

// Execute actual maintained Store mutations. Table values remain live; individual
// references observe granular mutation; full replacement uses a new generation.
const market={id:'m',symbol:'BTCUSD',base:'BTC',quote:'USD',active:true,
 precision:{amount:1,price:0.5},limits:{amount:{min:1,max:1000},leverage:{min:1,max:100}}};
const ticker={id:'t',symbol:'BTCUSD',bid:1,ask:2,last:1,mark:1,index:1,
 percentage:0,openInterest:0,fundingRate:0,fundingTime:0,volume:0,quoteVolume:0};
const store=new DefaultStore();store.update({markets:[structuredClone(market)],tickers:[structuredClone(ticker)]});
const views=['marketsById','marketsBySymbol','tickersById','tickersBySymbol'];
const tables=views.map(name=>store[name]);
store.updateMarket({id:'m'},{active:false});store.updateTicker({id:'t'},{bid:7});
const storeCases=views.map((name,i)=>({name,key:name.endsWith('ById')?(name.startsWith('markets')?'m':'t'):'BTCUSD',
 field:name.startsWith('markets')?'active':'bid',value:name.startsWith('markets')?false:7,
 observed:tables[i][name.endsWith('ById')?(name.startsWith('markets')?'m':'t'):'BTCUSD'][name.startsWith('markets')?'active':'bid']}));
for(const c of storeCases)assert.equal(c.observed,c.value,c.name+' retained table reads current entry');

// Independent Node Proxy observations specify the bounded native view operations.
// This is a language witness, not an implementation of the Store or Rust hooks.
const token={native:'alias'};
const state=new Map([['10',10],['2',2],['empty',undefined],['word',17],['unit\uD800',token],['negZero',-0],['nan',NaN]]);
const parent={inherited:23,get receiver(){return this}};const target=Object.create(parent);
Object.defineProperty(target,'shadow',{value:31,enumerable:false,configurable:true});
let receiverSeen=false,reads=0,keysCalls=0,failLookup=false,failKeys=false;
const lookupError={failure:'lookup'},keysError={failure:'keys'};
const proxy=new Proxy(target,{
 get(target,key,receiver){if(Reflect.getOwnPropertyDescriptor(target,key))return Reflect.get(target,key,receiver);
  if(failLookup)throw lookupError;reads++;receiverSeen=receiver===proxy;
  return state.has(key)?state.get(key):Reflect.get(target,key,receiver);},
 ownKeys(target){if(failKeys)throw keysError;keysCalls++;
  return [...Reflect.ownKeys(Object.fromEntries(state)),...Reflect.ownKeys(target).filter(k=>!state.has(k))];},
 getOwnPropertyDescriptor(target,key){const own=Reflect.getOwnPropertyDescriptor(target,key);if(own)return own;
  if(state.has(key))return {value:state.get(key),writable:true,enumerable:true,configurable:true};},
 has(target,key){if(failKeys)throw keysError;return Reflect.has(target,key)||state.has(key);}
});
const presentUndefined=Object.hasOwn(proxy,'empty')&&proxy.empty===undefined;
const absent=!Object.hasOwn(proxy,'absent')&&proxy.absent===undefined;
const inherited=proxy.inherited;const keysBefore=Object.keys(proxy);
const ownNames=Object.getOwnPropertyNames(proxy);const beforeReads=reads;
Object.keys(proxy);assert.equal(reads,beforeReads,'key enumeration does not materialize values');
state.set('late',41);const spread={...proxy};const json=JSON.stringify(proxy);
assert.equal(receiverSeen,true);assert.equal(presentUndefined,true);assert.equal(absent,true);
assert.deepEqual(keysBefore,['2','10','empty','word','unit\uD800','negZero','nan']);assert.equal(spread.late,41);
assert.equal(Object.hasOwn(spread,'empty'),true);assert.equal(inherited,23);
assert.equal(proxy.receiver,proxy);const child=Object.create(proxy);assert.equal(child.receiver,child);
assert.equal(proxy['unit\uD800'],token);assert.equal(spread['unit\uD800'],token);
assert.equal(Object.is(proxy.negZero,-0),true);assert.equal(Number.isNaN(proxy.nan),true);
const failures=[];
failLookup=true;try{proxy.missing}catch(e){failures.push(e===lookupError)}failLookup=false;
failKeys=true;
for(const action of [()=>Object.keys(proxy),()=>({...proxy}),()=>JSON.stringify(proxy)]){
 try{action()}catch(e){failures.push(e===keysError)}
}
failKeys=false;
const descriptorError={failure:'descriptor'};
const failingDescriptor=new Proxy({}, {getOwnPropertyDescriptor(){throw descriptorError}});
try{Object.hasOwn(failingDescriptor,'x')}catch(e){failures.push(e===descriptorError)}
assert.deepEqual(failures,[true,true,true,true,true]);
process.stdout.write(JSON.stringify({market,ticker,storeCases,proxy:{presentUndefined,absent,inherited,keysBefore:keysBefore.map(k=>Array.from(k,c=>c.charCodeAt(0))),ownNames:ownNames.map(k=>Array.from(k,c=>c.charCodeAt(0))),
 spreadKeys:Object.keys(spread).map(k=>Array.from(k,c=>c.charCodeAt(0))),json,receiverSeen,failures,keysCalls}}));
