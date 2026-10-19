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


// Source evidence for the raw Store boundary; no Store method is replaced.
const settings = new DefaultStore();
let settingCalls = 0;
settings.subscribe(() => settingCalls++);
settings.setSetting('other', true);
settings.setSetting('other', true);
assert.equal(settings.options.other, true);
assert.equal(settingCalls, 1);

const position = {id:'p',symbol:'BTCUSD',side:'long',entryPrice:20000,notional:1000,
 leverage:10,unrealizedPnl:0,contracts:0.05,liquidationPrice:10000};
const patches = new DefaultStore();
patches.update({positions:[position]});
const entry = () => null;
patches.updatePosition({id:'p'}, {leverage:undefined,entryPrice:entry,marginType:'isolated',
 info:null,unknown:NaN});
const raw = patches.positions[0];
const patch = {ownLeverage:Object.hasOwn(raw,'leverage'),leverageUndefined:raw.leverage===undefined,
 entryIdentity:raw.entryPrice===entry,marginType:raw.marginType,infoNull:raw.info===null,
 unknownNaN:Number.isNaN(raw.unknown)};
assert.deepEqual(patch,{ownLeverage:true,leverageUndefined:true,entryIdentity:true,
 marginType:'isolated',infoNull:true,unknownNaN:true});
patches.updatePosition({id:'p'}, {leverage:null});
assert.equal(raw.leverage, null);

const seed = id => ({id,symbol:'BTCUSD',status:'open',type:'limit',side:'buy',price:20000,
 amount:1,filled:0,remaining:1,reduceOnly:false,lastUpdate:1});
const terminals = new DefaultStore();
terminals.markOrdersTerminal([{id:'\ud800'}], 'closed');
terminals.update({orders:[seed('\ufffd'),seed('\ud800')]});
const ids = terminals.orders.map(order => Array.from({length:order.id.length},(_,i)=>order.id.charCodeAt(i)));
assert.deepEqual(ids,[[0xfffd]],'surrogate terminal memory must not alias replacement character');
// Symbols must retain their own slot, including object/array ToPrimitive.
const symbols=[];
for (const arrayKey of [false,true]) {
 const symbolic = new DefaultStore();
 symbolic.update({orders:[seed(Symbol.iterator),seed('Symbol(Symbol.iterator)')],
 positions:[{...position,id:Symbol.iterator},{...position,id:'Symbol(Symbol.iterator)'}]});
 let conversions = 0;
 const key = arrayKey ? [] : {};
 key.toString=()=>{conversions++;return Symbol.iterator};
 const selector = {id:key};
 symbolic.removeOrders([selector]);
 symbolic.updatePositions([[selector,{contracts:7}],[{id:'Symbol(Symbol.iterator)'},{contracts:9}]]);
 const result={ids:symbolic.orders.map(row=>row.id),contracts:symbolic.positions.map(row=>row.contracts),conversions};
 assert.deepEqual(result,{ids:['Symbol(Symbol.iterator)'],contracts:[7,9],conversions:2});
 symbols.push(result);
}
// Assignment observes the length RHS before the final key coercion/PutValue.
const indexed = new DefaultStore();
indexed.update({orders:[seed('a'),seed('b')]});
let upsertConversions=0;
const upsertKey={toString(){if(++upsertConversions===2)indexed.orders.push(seed('filler'));return 'new'}};
indexed.addOrUpdateOrders([{...seed(upsertKey),amount:7},{...seed('new'),amount:9}]);
const upsert={rows:indexed.orders.map(row=>({id:typeof row.id==='string'?row.id:'object',amount:row.amount})),conversions:upsertConversions};
assert.deepEqual(upsert,{rows:[{id:'a',amount:1},{id:'b',amount:1},{id:'new',amount:9},{id:'object',amount:7}],conversions:2});
process.stdout.write(JSON.stringify({setting:{other:settings.options.other,calls:settingCalls},patch,
 nullLeverage:raw.leverage===null,terminalIds:ids,symbols,upsert}));
