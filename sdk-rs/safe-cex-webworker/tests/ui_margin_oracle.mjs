// Live source capture: validated Node 26.7.0 on darwin/arm64; CI executes on its own host.
import fs from 'node:fs';
import vm from 'node:vm';
import crypto from 'node:crypto';
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const entry=require.resolve(process.env.SAFE_CEX_WORKER_LODASH_PATH?process.env.SAFE_CEX_WORKER_LODASH_PATH+'/lodash.js':'lodash');
assert.equal(crypto.createHash('sha256').update(fs.readFileSync(entry)).digest('hex'),'f5465f55566bf544aad0a31c6135889ca1ed81eea8f53ec61c6cbe86926f07cf');
const lodash=require(entry);assert.equal(lodash.VERSION,'4.18.1');
const root=new URL('../../../packages/safe-cex/src/',import.meta.url);
function ast(path){return ts.createSourceFile(path,fs.readFileSync(new URL(path,root),'utf8'),ts.ScriptTarget.Latest,true);}
function body(path){const a=ast(path);return a.statements.filter(n=>!ts.isImportDeclaration(n)).map(n=>n.getText(a)).join('\n').replace(/\bexport /g,'');}
const types=ast('types.ts'),names=['OrderSide','OrderType','OrderStatus','PositionSide','PositionMode'];
const enums=types.statements.filter(n=>ts.isEnumDeclaration(n)&&names.includes(n.name.text));
assert.equal(enums.length,names.length);
let events=[],caught=[];
const context=vm.createContext({Error,structuredClone,uniqBy:lodash.uniqBy,isEqual:lodash.isEqual,Date:{now:()=>10000},console:{error(...args){caught.push(args.find(v=>v instanceof Error));},warn(){},log(){}}});
function run(source){vm.runInContext(ts.transpileModule(source,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);}
const clockAst=ast('utils/virtualClock.ts');
const clock=clockAst.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text==='VirtualClock');assert.ok(clock);
run(clock.getText(clockAst)+'\nconst virtualClock=new VirtualClock();\n'+enums.map(n=>n.getText(types).replace('export ','')).join('\n')+'\n'+['utils/clone.ts','store/store.base.ts'].map(body).join('\n')+'\nglobalThis.Store=DefaultStore;');
for(const name of ['BaseHandler','UiStateHandler']){
 run(`(()=>{${body(`webworker/handlers/${name}.ts`)}\nglobalThis.${name}=${name};})();`);
}
const scenarios = [
 {name:'disabled',enabled:false}, {name:'missing',symbol:null}, {name:'empty',symbol:''},
 {name:'existing',existing:true}, {name:'fetching',existing:true,fetching:true},
 {name:'success'}, {name:'success-no-positions',positions:false},
 {name:'none',outcome:'none'}, {name:'reject',outcome:'reject'},
 {name:'initial-throw',fail:[1]}, {name:'position-throw',fail:[2]},
 {name:'none-clear-throw',outcome:'none',fail:[2]},
 {name:'none-both-clear-throw',outcome:'none',fail:[2,3]},
 {name:'reject-clear-throw',outcome:'reject',fail:[2]},
 {name:'position-and-clear-throw',fail:[2,3]},
 {name:'concurrent-none',outcome:'none',concurrent:true},
 {name:'scalar',symbol:'é'}, {name:'astral',symbol:'🪙'}, {name:'whitespace',symbol:'\ufeff'},
 ...Object.getOwnPropertyNames(Object.prototype).map(symbol=>({name:'inherited:'+symbol,symbol})),
 {name:'numeric-key',symbol:'42',initialOther:true},
];
assert.equal(scenarios.length,32,'MARGIN source scenario total');
const rows=[];
async function scenario(spec){
 const enabled=spec.enabled!==false, symbol=Object.hasOwn(spec,'symbol')?spec.symbol:'BTC';
 const store=new context.Store();
 const position={id:'p',symbol:'BTC',side:'long',entryPrice:100,contracts:2,notional:200,leverage:5,unrealizedPnl:10,liquidationPrice:0};
 const previous={buyLeverage:3,sellLeverage:4,isFetching:!!spec.fetching};
 store.update({positions:spec.positions===false?[]:[position],fetchedLeverage:spec.existingNull?{BTC:null}:spec.existing?{BTC:previous}:spec.initialOther?{OTHER:{buyLeverage:7,sellLeverage:8}}:{}},true);
 if(spec.customPrototype)Object.setPrototypeOf(store.fetchedLeverage,{BTC:previous});
 const events=[];let calls=0,notifications=0,pending,thrown=null;
 function view(){return {fetched:store.fetchedLeverage,leverage:store.positions.map(p=>p.leverage)};}
 store.subscribe(()=>{const index=++notifications;events.push({event:'notify',index,state:structuredClone(view())});if(spec.fail?.includes(index)){const error=new TypeError('notify '+index);error.code=index;throw error;}});
 const exchange={store,fetchLeverageForSymbol(value){calls++;events.push({event:'fetch',symbol:value});
  if(spec.concurrent)store.update({fetchedLeverage:{...store.fetchedLeverage,OTHER:{buyLeverage:7,sellLeverage:8}}},true);
  const fetch=spec.outcome==='reject'?Promise.reject(new Error('fetch failed')):Promise.resolve(spec.outcome==='none'?null:{buyLeverage:11,sellLeverage:13});
  return {then(callback){return {catch(onError){pending=fetch.then(callback).catch(onError);return pending;}};}};
 }};
 const handler=new context.UiStateHandler('a','bybit',exchange,{}, {},{fetchLeverageOnMarginSymbolChange:enabled});
 try{handler.handleActiveMarginSymbolChange(symbol===null?{}:{activeMarginSymbol:symbol});if(pending)await pending;}catch(error){thrown={name:error.name,message:error.message,code:error.code};}
 return {spec,symbol,enabled,calls,notifications,events,state:view(),thrown,codeJson:thrown ? JSON.stringify(thrown.code) : null};
}
for(const spec of scenarios)rows.push(await scenario(spec));
const numeric=await scenario({symbol:42,positions:false});
const surrogate=await scenario({symbol:'\ud800',positions:false});
const falsy=await scenario({existingNull:true});
const custom=await scenario({customPrototype:true});
const boundaries={falsyCalls:falsy.calls,customPrototypeCalls:custom.calls,numericCalls:numeric.calls,numericType:typeof numeric.events.find(e=>e.event==='fetch').symbol,surrogateCalls:surrogate.calls,surrogateJson:JSON.stringify(surrogate.state.fetched)};
process.stdout.write(JSON.stringify({boundaries,architecture:process.arch,nodeMajor:process.versions.node.split('.')[0],rows}));
