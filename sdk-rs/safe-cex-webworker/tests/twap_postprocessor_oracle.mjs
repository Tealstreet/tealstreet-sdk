// Live bit capture: validated Node 26.7.0 on darwin/arm64; CI captures on its own host.
import fs from 'node:fs';
import {records as identities} from './task_identity_oracle.mjs';
import vm from 'node:vm';
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=new URL('../../../packages/safe-cex/src/',import.meta.url);
function source(path){let text=fs.readFileSync(new URL(path,root),'utf8');
if(process.argv.includes('--mutate-market-copy') && path==='webworker/postProcessors/TwapPostprocessor.ts') {
 const marker='activeTWAPs.push({';assert.ok(text.includes(marker));
 text=text.replace(marker,'[twapInstance.state.activeMarket.id,twapInstance.state.activeMarket.base]=[twapInstance.state.activeMarket.base,twapInstance.state.activeMarket.id]; '+marker);
}
const ast=ts.createSourceFile(path,text,ts.ScriptTarget.Latest,true);return ast.statements.filter(s=>!ts.isImportDeclaration(s)).map(s=>s.getText(ast)).join('\n').replace(/\bexport /g,'');}
const context=vm.createContext({Date:{now:()=>123},nanoid:()=> 'generated'});
vm.runInContext(ts.transpileModule(['webworker/twap/TwapStatus.ts','webworker/twap/TwapManager.ts','webworker/twap/TwapInstance.ts','webworker/postProcessors/ExchangeDependentPostprocessor.ts','webworker/postProcessors/TwapPostprocessor.ts'].map(source).join('\n')+'\nglobalThis.Instance=TwapInstance;globalThis.Manager=twapManager;globalThis.Processor=TwapPostprocessor;', {compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
const bytes=new DataView(new ArrayBuffer(8));
function bits(n){if(Number.isNaN(n))return 'NaN';bytes.setFloat64(0,n);return bytes.getBigUint64(0).toString(16).padStart(16,'0');}
function encode(v){return typeof v==='number'?{$num:bits(v)}:Array.isArray(v)?v.map(encode):v&&typeof v==='object'?Object.fromEntries(Object.entries(v).map(([k,x])=>[k,encode(x)])):v;}
const market={id:'BTC',symbol:'BTC',base:'BTC',quote:'USD',active:true,precision:{amount:0.01,price:0.01},limits:{amount:{min:0,max:100},leverage:{min:1,max:100}}};
function options(side,size,symbol,origin){return {accountName:'target',activeMarket:{...structuredClone(market),symbol},side,size,ordersCount:7,duration:size,chaserDelay:100,reduceOnly:side==='buy',randomnessPercentage:12.5,useLimitOrders:side==='sell',useChaserOrders:origin==='webhook',timeoutPct:25,timeoutAction:'next',failAction:'market',...(origin?{origin}:{})};}
function state(status,side,origin){return {accountName:'state-account',activeMarket:{...structuredClone(market),symbol:'STATE'},size:99,ordersSizes:[-0,NaN,Infinity],side:side==='buy'?'sell':'buy',status,ordersCount:3,ordersExecuted:1,nextOrderAt:123,markForCleanup:false,...(origin?{origin}:{}),createdAt:42,startedAt:43};}
function add(id,opts,s){const i=new context.Instance({}, {postMessage(){}},opts,()=>{},s,id);context.Manager.add(i);return i;}
const rows=[];
for(const side of ['buy','sell'])for(const status of ['idle','running','paused','completed'])for(const size of [0,-0,3,NaN,Infinity,-Infinity])for(const symbol of ['',' BTC🚀\t','constructor'])for(const originMode of [0,1,2]){
 context.Manager.runningTwaps.clear();
 const first=add('__proto__',options(side,size,symbol,originMode===0?undefined:'cli'),state(status,side,originMode===2?'automation':undefined));
 add('wrong-account',{...options(side,size,symbol,'web'),accountName:'different'},state('running',side,'web'));
 add('completed',options(side,size,symbol,'web'),state('completed',side,'web'));
 add('constructor',options(side,size,symbol,'webhook'),state('running',side,undefined));
 const input=[...context.Manager.runningTwaps.values()].map(i=>encode({id:i.id,options:i.options,state:i.state}));
 const store={activeTWAPs:[{id:'stale'}]},processor=new context.Processor('target','bybit',{},{});
 processor.process(store);const expected=encode(store.activeTWAPs);
 const selected=context.Manager.getById('constructor'),held=store.activeTWAPs.find(t=>t.id==='constructor');
 selected.state.ordersSizes.push(77);selected.state.activeMarket.base='from-state';
 const alias={orders:encode(held.ordersSizes),base:held.activeMarket.base};
 held.activeMarket.quote='from-output';alias.quote=selected.state.activeMarket.quote;
 first.options.accountName='different';selected.state.status='completed';processor.process(store);const cleared=encode(store.activeTWAPs);
 rows.push({input,expected,alias,cleared});
}
context.Manager.runningTwaps.clear();
const emptyOpts={...options('buy',0,'BTC',undefined),ordersCount:0};
add('empty-array',emptyOpts,undefined);
const processor=new context.Processor('target','bybit',{},{}),store={activeTWAPs:[]};processor.process(store);
const emptyProperty={own:Object.hasOwn(store.activeTWAPs[0].ordersSizes,'-1'),value:bits(store.activeTWAPs[0].ordersSizes[-1])};
context.Manager.runningTwaps.clear();
const lone=JSON.parse('"\\ud800"');add(lone,options('buy',1,'BTC',undefined),state('running','buy',undefined));processor.process(store);
const loneSurrogate={codeUnit:store.activeTWAPs[0].id.charCodeAt(0),json:JSON.stringify(store.activeTWAPs)};
context.Manager.runningTwaps.clear();
add('explicit-undefined',options('buy',1,'BTC','cli'),{...state('running','buy',undefined),origin:undefined});processor.process(store);
const explicitUndefined={own:Object.hasOwn(store.activeTWAPs[0],'origin'),type:typeof store.activeTWAPs[0].origin,serializedOwn:Object.hasOwn(JSON.parse(JSON.stringify(store.activeTWAPs[0])),'origin')};
const marketIdentity=identities.TwapOptions.cases.filter(c=>c.path.startsWith('/activeMarket/')).map(c=>{
 context.Manager.runningTwaps.clear();
 const opts=options('buy',2,'OPTIONS-MARKET','cli');
 const original={...state('running','buy','automation'),activeMarket:structuredClone(c.input.activeMarket),ordersSizes:[1,2]};
 const input=structuredClone({options:opts,state:original});
 add('identity-market',opts,original);processor.process(store);
 const expected=structuredClone(store.activeTWAPs[0].activeMarket);
 assert.deepEqual(expected,input.state.activeMarket,'SOURCE-TWAP-PROJECTION market copy identity');
 return {path:c.path,...input,expected};
});
process.stdout.write(JSON.stringify({rows,emptyProperty,loneSurrogate,explicitUndefined,marketIdentity}));
