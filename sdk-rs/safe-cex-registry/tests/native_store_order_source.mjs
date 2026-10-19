import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {createRequire} from 'node:module';
import {ts} from '../../safe-cex-store/tests/source_loader.mjs';
import fs from 'node:fs';
import {execFileSync} from 'node:child_process';
import vm from 'node:vm';
const root = new URL('../../../packages/safe-cex/src/', import.meta.url);
const cache=new Map();
const dependencies=createRequire(process.env.SAFE_CEX_TYPESCRIPT_PATH
  ? path.join(path.dirname(process.env.SAFE_CEX_TYPESCRIPT_PATH),'oracle.cjs') : import.meta.url);
function loadSource(url) {
  const filename=fileURLToPath(url);
  if(cache.has(filename))return cache.get(filename).exports;
  const module={exports:{}};cache.set(filename,module);
  const localRequire=name=>{
    if(name==='axios')return {get:async()=>{throw Error('network disabled')}};
    if(name==='../transport/registry')return {withPublicCorsProxy:()=>{throw Error('network disabled')}};
    if(name.startsWith('@tealstreet/'))return loadSource(new URL('../../../packages/'+name.slice('@tealstreet/'.length)+'.ts',import.meta.url));
    if(name.startsWith('.'))return loadSource(new URL(name+'.ts',url));
    return dependencies(name);
  };
  const code=ts.transpileModule(fs.readFileSync(filename,'utf8'),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS,esModuleInterop:true}}).outputText;
  vm.runInThisContext('(function(require,module,exports){'+code+'\n})',{filename})(localRequire,module,module.exports);
  return module.exports;
}
const {DefaultStore,PACKED_TICKER_FIELDS} = loadSource(new URL('store/store.base.ts',root));
const file = ts.createSourceFile('ExchangeWorker.ts',fs.readFileSync(new URL('webworker/ExchangeWorker.ts',root),'utf8'),ts.ScriptTarget.Latest,true);
const cls = file.statements.find(s=>ts.isClassDeclaration(s)&&s.name.text==='ExchangeWorker');
const names=['_sanitizeForPostMessage','_shouldSendUpdateIdBackedSnapshot','_recordSentUpdateIdBackedSnapshot','_copyTickerBaseline','_packTickerDeltasSinceBaseline','_prepareStorePayload'];
const members=cls.members.filter(m=>names.includes(m.name?.getText(file)) || m.name?.getText(file)?.startsWith('_last'));
const context=vm.createContext({PACKED_TICKER_FIELDS});
vm.runInContext(ts.transpileModule('class Worker {'+members.map(m=>m.getText(file)).join('\n')+'} globalThis.Worker=Worker;', {compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
const store=new DefaultStore(),worker=new context.Worker();
worker._store=store;worker._options={hostedMode:true};
const snapshot=worker._prepareStorePayload(store.serialize(),{forceFullStore:true});
const corpus=JSON.parse(execFileSync(process.execPath,[fileURLToPath(new URL('../../safe-cex-webworker/tests/store_preparation_oracle.mjs',import.meta.url))],{encoding:'utf8',maxBuffer:16*1024*1024}));
const base=corpus.fieldIdentity[0].input;
base.activeChasers=[{id:'c',symbol:'BTC',size:2,side:'buy',maxAttempts:7,attempts:3,isStopped:false,stopReason:'',currentOrderIds:['o']}];
base.activeTWAPs=[{id:'tw',symbol:'BTC',accountName:'a',activeMarket:base.markets[0],size:2,duration:10,ordersCount:2,side:'buy',chaserDelay:1,reduceOnly:false,randomnessPercentage:1,useLimitOrders:false,useChaserOrders:false,ordersSizes:[1,1],status:'running',ordersExecuted:0,nextOrderAt:1}];
base.ordersRisk={BTC:[{orderPnl:1,totalRealizedPnl:2,rollingUnrealizedPnl:3,orderLinePosSide:'long',fee:4,positionSize:5}]};
function reorder(value,rotation) {
 if(Array.isArray(value))return value.map(v=>reorder(v,rotation));
 if(value&&typeof value==='object'){
  const fields=Object.entries(value),n=fields.length?rotation%fields.length:0;
  return Object.fromEntries([...fields.slice(n),...fields.slice(0,n)].map(([k,v])=>[k,reorder(v,rotation)]));
 }
 return value;
}
const cases=[];
for(let rotation=0;rotation<6;rotation++){
 const store=new DefaultStore(),worker=new context.Worker();
 const input=reorder(base,rotation);store.update(input);
 worker._store=store;worker._options={hostedMode:true};
 cases.push({input,expected:worker._prepareStorePayload(store.state,{forceFullStore:true})});
}
console.log(JSON.stringify({snapshot:Object.keys(snapshot),live:Object.keys(store.state),cases}));
