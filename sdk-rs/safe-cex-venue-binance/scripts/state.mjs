import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url);
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const root=new URL('../../../',import.meta.url);
const ast=variant=>ts.createSourceFile(variant,fs.readFileSync(new URL(`packages/safe-cex/src/exchanges/binance/${variant}.exchange.ts`,root),'utf8'),ts.ScriptTarget.Latest,true);
function method(variant,name,context={}) {const source=ast(variant);const klass=source.statements.find(ts.isClassDeclaration);const member=klass.members.find(m=>m.name?.getText(source)===name);assert.ok(member,name);const exports={};vm.runInNewContext(ts.transpileModule(`exports.fn = function(...args) { return (${member.initializer.getText(source)})(...args); };`,{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports,...context});return exports.fn;}
const encode=n=>Object.is(n,-0)?'-0':String(n);
const values=[-Infinity,-1,-0,0,1,1.5,100,9007199254740991,9007199254740992,Infinity,NaN];
const cases=[];
let now=0;
const pause=method('binance','pauseBackgroundReconciliation',{Date:{now:()=>now}});
const paused=method('binance','isBackgroundReconciliationPaused',{Date:{now:()=>now}});
const mark=method('binance','markPositionStateUpdatedByWebsocket');
const get=method('binance','getPositionStateVersion');
for(const before of values) for(const clock of values) for(const duration of values) {
 now=clock; const owner={backgroundReconciliationPausedUntil:before};
 pause.call(owner,duration);
 cases.push({kind:'pause',before:encode(before),now:encode(now),duration:encode(duration),expected:encode(owner.backgroundReconciliationPausedUntil),paused:paused.call(owner)});
}
for(const before of values) {const owner={positionStateVersion:before};const outputs=[encode(get.call(owner))];for(let i=0;i<3;i++){mark.call(owner);outputs.push(encode(get.call(owner)));}cases.push({kind:'version',before:encode(before),expected:outputs});}
const price=method('binancespot','getAssetUsdPrice');
const quotes=[undefined,NaN,-Infinity,Infinity,-0,0,-1,1,123.4];
for(const asset of ['USD','USDT','USDC','BTC','FDUSD','','usdt','é']) for(const a of quotes) for(const b of quotes) for(const c of quotes) {
 const data=Object.fromEntries([a,b,c].map((n,i)=>[`${asset}${['USDT','USDC','USD'][i]}`,n===undefined?undefined:{last:n}]));
 const calls=[];const proxy=new Proxy(data,{get:(o,k)=>{calls.push(k);return o[k];}});
 const expected=price.call({store:{tickersBySymbol:proxy}},asset);
 cases.push({kind:'price',asset,quotes:[a,b,c].map(n=>n===undefined?'undefined':encode(n)),expected:encode(expected),calls});
}
process.stdout.write(JSON.stringify(cases));
