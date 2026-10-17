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

const seeds={
 orders:{id:'123',symbol:'BTC',amount:2,filled:0,remaining:2,reduceOnly:false,lastUpdate:1,status:'open'},
 positions:{id:'123',symbol:'BTC',side:'long',entryPrice:10,notional:20,leverage:2,unrealizedPnl:3,contracts:2,liquidationPrice:5},
 tickers:{id:'123',symbol:'BTC',bid:1,ask:2,last:1,mark:1,index:1,percentage:0,openInterest:0,fundingRate:0,fundingTime:0,volume:0,quoteVolume:0},
 markets:{id:'123',symbol:'BTC',base:'BTC',quote:'USD',active:true,precision:{amount:1,price:1},limits:{amount:{min:0,max:100},leverage:{min:1,max:100}}},
};
const selectors={
 string_id:()=>({id:'123'}),number_id:()=>({id:123}),bare_string:()=> 'abc',
 undefined:()=>undefined,null:()=>null,missing_id:()=>({}),undefined_id:()=>({id:undefined}),
 null_id:()=>({id:null}),object_id:()=>({id:{toString(){return '123'}}}),
 throwing_id:()=>Object.defineProperty({},'id',{get(){throw new RangeError('selector getter')}}),
 noncoercible_id:()=>({id:{toString(){throw new RangeError('selector conversion')}}}),
};
const methods=[['removeOrder','orders','amount',null],['updateOrder','orders','amount',{amount:7}],['updateOrderStatus','orders','status','canceled'],['updatePosition','positions','contracts',{contracts:7}],['updateTicker','tickers','bid',{bid:7}],['updateMarket','markets','active',{active:false}]];
const cases=[];
for(const [method,state,field,changes] of methods)for(const [tag,select] of Object.entries(selectors))for(const empty of [false,true]){
 const store=new DefaultStore();
 const rows=empty?[]:['123','abc','undefined'].map(id=>({...seeds[state],id}));
 store.update({[state]:rows});
 let error=null;try{store[method](select(),changes)}catch(e){error=e.name}
 cases.push({method,state,field,changes,tag,empty,error,rows:store[state].map(row=>({id:row.id,value:String(row[field])}))});
}
assert.equal(cases.length,132,'six singular mutators,eleven selectors,empty/nonempty states');
process.stdout.write(JSON.stringify({seeds,cases}));
