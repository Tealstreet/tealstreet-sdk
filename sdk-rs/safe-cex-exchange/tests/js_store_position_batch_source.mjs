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

const selectors={
 string_id:()=>({id:'123'}),number_id:()=>({id:123}),bare_string:()=> 'abc',
 undefined:()=>undefined,null:()=>null,missing_id:()=>({}),undefined_id:()=>({id:undefined}),
 null_id:()=>({id:null}),object_id:()=>({id:{toString(){return '123'}}}),
 throwing_id:()=>Object.defineProperty({},'id',{get(){throw new RangeError('selector getter')}}),
 noncoercible_id:()=>({id:{toString(){throw new RangeError('selector conversion')}}}),
};

selectors.object_symbol_id=()=>({id:{toString(){return Symbol.iterator}}});
selectors.symbol_id=()=>({id:Symbol.iterator});
selectors.surrogate_id=()=>({id:'\ud800'});
const seed={id:'123',symbol:'BTC',side:'long',entryPrice:10,notional:20,leverage:2,unrealizedPnl:3,contracts:2,liquidationPrice:5};
const states={single:['123'],distinct:['123','abc'],duplicate:['123','123'],empty:[]};
const cases=[];
for(const [mode,ids] of Object.entries(states))for(const count of [0,1,2])for(const [tag,select] of Object.entries(selectors)){
 const store=new DefaultStore();store.update({positions:ids.map(id=>({...seed,id}))});
 const before=store.updateIds.positions;
 const updates=count===0?[]:[[select(),{contracts:7}],...(count===2?[[{id:'123'},{contracts:9}]]:[])];
 let error=null;try{store.updatePositions(updates)}catch(e){error=e.name}
 cases.push({mode,count,tag,error,ids,delta:store.updateIds.positions-before,rows:store.positions.map(row=>({id:row.id,contracts:String(row.contracts)}))});
}
assert.equal(cases.length,168,'four position states,three update counts,fourteen selectors');
process.stdout.write(JSON.stringify({seed,cases}));
