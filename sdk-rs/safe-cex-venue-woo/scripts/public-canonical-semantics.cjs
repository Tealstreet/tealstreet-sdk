const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const source=fs.readFileSync(path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/woo/woo.v3.public-parity.ts'),'utf8');
const context=vm.createContext({exports:{},require:key=>{assert.equal(key,'./woo.v3.public');return {mapWooV3Candles:()=>{throw Error('unexpected mapper');}};}});
vm.runInContext(ts.transpileModule(source+'\nexport {canonicalString,canonicalMarket,canonicalLegacyTickerStats,canonicalV3TickerStats,canonicalOrderBookLevel,canonicalCandle};',{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,context);
const api=context.exports;
const market={id:'id',symbol:'OTHER',base:'B',quote:'Q',active:true,precision:{amount:11,price:12},limits:{amount:{min:13,max:14},leverage:{min:15,max:16}}};
const legacy={id:'id',symbol:'OTHER',index:1,mark:2,percentage:3,openInterest:4,fundingRate:5,fundingTime:6,volume:7,quoteVolume:8};
const stats={id:'id',symbol:'OTHER',index:1,mark:2,percentage24h:3,openInterest:4,estimatedFundingRate:5,fundingTime:6,volume24h:7,quoteVolume24h:8};
const level={price:21,amount:22,total:23},candle={timestamp:31,open:32,high:33,low:34,close:35,volume:36};
const defs=[['market','canonicalMarket',market],['legacy','canonicalLegacyTickerStats',legacy],['stats','canonicalV3TickerStats',stats],['level','canonicalOrderBookLevel',level],['candle','canonicalCandle',candle]];
function observed(v,trace,cache=new WeakMap()){if(v===null||typeof v!=='object')return v;if(cache.has(v))return cache.get(v);const p=new Proxy(v,{get(o,k){trace.push(String(k));return observed(Reflect.get(o,k),trace,cache);}});cache.set(v,p);return p;}
const projectionKeys=[['id','symbol','base','quote','active','precision','limits'],['id','symbol','index','mark','percentage','openInterest','fundingRate','fundingTime','volume','quoteVolume'],['id','symbol','index','mark','percentage','openInterest','fundingRate','fundingTime','volume','quoteVolume'],['price','amount','total'],['timestamp','open','high','low','close','volume']];
const projections=defs.map(([id,name,input],i)=>{const trace=[];const output=api[name](observed(input,trace));assert.deepEqual(Object.keys(output),projectionKeys[i]);assert.equal(trace.length,[21,10,10,3,6][i],'source property-call count');if(id==='legacy'||id==='stats'){assert.equal(output.fundingRate,5,'source statistics fundingRate');assert.equal(output.volume,7,'source statistics volume');}return {id,input,trace,keys:Object.keys(output),wire:JSON.stringify(output)};});
assert.equal(projections.reduce((n,p)=>n+p.trace.length,0),50,'source total property calls');
const lists=[['markets','canonicalizeWooV3Markets',market],['legacy','canonicalizeWooLegacyTickerStats',legacy],['stats','canonicalizeWooV3TickerStats',stats]].map(([id,name,base])=>{
 const input=[{...base,id:'first',symbol:'OTHER'},{...base,id:'number',symbol:'42'},{...base,id:'last',symbol:'OTHER'}];const original=input.slice();const wire=api[name](input);assert.equal(input.length,3);for(let i=0;i<3;i++)assert.equal(input[i],original[i]);return {id,input,wire};});
const lone=String.fromCharCode(0xd800),alias={live:1};const projected=api.canonicalLegacyTickerStats({...legacy,id:lone,index:alias});assert.equal(projected.id.charCodeAt(0),0xd800);assert.equal(projected.index,alias);alias.live=9;assert.equal(projected.index.live,9);
let n=0;const dynamic={...market,get precision(){n++;return {amount:n===1?41:99,price:n===2?42:99};}};const dyn=api.canonicalMarket(dynamic);assert.equal(n,2);assert.equal(dyn.precision.amount,41);assert.equal(dyn.precision.price,42);
const failure={original:true},failureTrace=[];const bad=new Proxy(market,{get(o,k){failureTrace.push(k);if(k==='quote')throw failure;return o[k];}});let failureIdentity=false;try{api.canonicalMarket(bad);}catch(e){failureIdentity=e===failure;}assert.equal(failureIdentity,true);
const strings=[api.canonicalString(undefined),api.canonicalString(NaN),api.canonicalString(-0),api.canonicalString(lone)];assert.equal(strings[0],undefined);assert.equal(strings[1],'null');assert.equal(strings[2],'0');assert.equal(strings[3],'"\\ud800"');let bigError=false;try{api.canonicalString(2n);}catch(e){bigError=e.name==='TypeError';}assert.equal(bigError,true);
context.events=[];context.originalError={identity:true};context.lookupResult=undefined;
vm.runInContext(`globalThis.savedSort=Array.prototype.sort;
 const receiver={get localeCompare(){events.push('prepare');return function(rhs){events.push('invoke-old:'+rhs);return 7;};}};
 globalThis.lookupRows=[{get symbol(){events.push('left-symbol');return receiver;}},{get symbol(){events.push('right-symbol');Object.defineProperty(receiver,'localeCompare',{value:()=>99,configurable:true});return 'R';}}];
 Array.prototype.sort=function(compare){const n=compare(this[0],this[1]);lookupResult=n;throw originalError;};`,context);
let lookupIdentity=false;try{api.canonicalizeWooV3Markets(context.lookupRows);}catch(e){lookupIdentity=e===context.originalError;}finally{vm.runInContext('Array.prototype.sort=savedSort;',context);}
assert.equal(lookupIdentity,true);assert.equal(context.lookupResult,7);assert.deepEqual(Array.from(context.events),['left-symbol','prepare','right-symbol','invoke-old:R']);
context.mapEvents=[];context.oneRow=legacy;
vm.runInContext(`Array.prototype.sort=function(){mapEvents.push('sort');return {marker:99,map(project){mapEvents.push(this.marker);return [project(oneRow)];}};};`,context);
let custom;try{custom=api.canonicalizeWooLegacyTickerStats([legacy]);}finally{vm.runInContext('Array.prototype.sort=savedSort;',context);}
assert.deepEqual(Array.from(context.mapEvents),['sort',99]);
context.laterRow=market;
vm.runInContext(`globalThis.savedMap=Array.prototype.map;Array.prototype.sort=function(compare){globalThis.retainedCompare=compare;return this;};Array.prototype.map=function(project){globalThis.retainedProject=project;return [];};`,context);
try{assert.equal(api.canonicalizeWooV3Markets([market]),'[]');}finally{vm.runInContext('Array.prototype.sort=savedSort;Array.prototype.map=savedMap;',context);}
const retainedCompare=context.retainedCompare({symbol:'OTHER'},{symbol:'42'});const retainedProjection=context.retainedProject(market);assert.equal(retainedCompare,1);assert.equal(retainedProjection.precision.amount,11);
process.stdout.write(JSON.stringify({projections,lists,alias:{idUnits:[projected.id.charCodeAt(0)],live:projected.index.live},dynamic:{reads:n,amount:dyn.precision.amount,price:dyn.precision.price},failure:{identity:failureIdentity,trace:failureTrace},strings:{undefined:strings[0]===undefined,nan:strings[1],zero:strings[2],lone:strings[3],bigError},lookup:{trace:Array.from(context.events),value:context.lookupResult,identity:lookupIdentity},custom:{wire:custom,trace:Array.from(context.mapEvents)},retained:{compare:retainedCompare,wire:JSON.stringify(retainedProjection)}}));
