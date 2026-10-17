import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import {createRequire} from 'node:module';
import {fileURLToPath} from 'node:url';
import {ts} from './source_loader.mjs';
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
const seed = {
  id:'p', symbol:'BTCUSDT', side:'short', entryPrice:10, notional:20, leverage:2,
  unrealizedPnl:3, contracts:2, liquidationPrice:5, marginType:'cross', positionMode:'hedged',
  realizedPnl:11, marginSize:120, info:{old:true}, extra:{contractsNative:12}, lastUpdate:1,
  option:{greeks:{delta:0.25,gamma:0.01,vega:2,theta:-1,units:'underlying'}},
};
const replacements = {
  marginSize:0, side:'long', marginType:'isolated', positionMode:'oneway', realizedPnl:0,
  info:{new:true}, extra:{}, lastUpdate:2, option:{},
};
const cases = [];
for (const method of ['addOrUpdatePosition','addOrUpdatePositions','addOrUpdatePositionsFull']) {
  for (const [field,value] of Object.entries(replacements)) {
    for (const mode of ['undefined','absent','value']) {
      const store = new DefaultStore();
      const sibling = {...seed,id:'sibling'};
      store.update({positions:[structuredClone(seed),structuredClone(sibling)]});
      const incoming = {...seed, contracts:0, entryPrice:0};
      delete incoming[field];
      if (mode === 'undefined') incoming[field] = undefined;
      if (mode === 'value') incoming[field] = value;
      const before = store.updateIds.positions;
      if (method === 'addOrUpdatePosition') store[method](incoming);
      else store[method]([incoming,sibling]);
      const row = store.positions.find(row=>row.id==='p');
      assert.equal(store.updateIds.positions,before+1);
      assert.equal(row.contracts,0);
      assert.equal(Object.hasOwn(row,field),true);
      assert.deepEqual(row[field],mode==='absent'?seed[field]:mode==='undefined'?undefined:value);
      cases.push({method,field,mode,undefined:row[field]===undefined,value:row[field]});
    }
  }
}
assert.equal(cases.length,81);
console.log(JSON.stringify({seed,replacements,cases}));
