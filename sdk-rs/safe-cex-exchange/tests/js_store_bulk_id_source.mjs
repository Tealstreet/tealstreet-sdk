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
 symbol_id:()=>({id:Symbol.iterator}),
 object_symbol_id:()=>({id:{toString(){return Symbol.iterator}}}),
 surrogate_id:()=>({id:'\ud800'}),
 throwing_id:()=>Object.defineProperty({},'id',{get(){throw new RangeError('selector getter')}}),
 noncoercible_id:()=>({id:{toString(){throw new RangeError('selector conversion')}}}),
};

const seed={id:'123',symbol:'BTC',amount:2,filled:0,remaining:2,reduceOnly:false,lastUpdate:1,status:'open'};
const ids=['123','abc','undefined','null','[object Object]'];
const cases=[];
for(const [tag,select] of [...Object.entries(selectors),['empty_input',()=>null]])for(const empty of [false,true]){
 const store=new DefaultStore();
 store.update({orders:empty?[]:ids.map(id=>({...seed,id}))});
 let error=null;try{store.removeOrders(tag==='empty_input'?[]:[select()])}catch(e){error=e.name}
 cases.push({tag,empty,error,ids:store.orders.map(order=>order.id)});
}
assert.equal(cases.length,30,'fifteen array-selector cases,empty/nonempty stores');
process.stdout.write(JSON.stringify({seed,ids,cases}));
