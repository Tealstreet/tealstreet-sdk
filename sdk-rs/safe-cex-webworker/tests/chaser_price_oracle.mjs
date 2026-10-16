// Live bit capture: validated Node 26.7.0 on darwin/arm64; CI captures on its own host.
import fs from 'node:fs';
import vm from 'node:vm';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=new URL('../../../packages/safe-cex/src/',import.meta.url);
function source(path){const ast=ts.createSourceFile(path,fs.readFileSync(new URL(path,root),'utf8'),ts.ScriptTarget.Latest,true);return ast.statements.filter(s=>!ts.isImportDeclaration(s)).map(s=>s.getText(ast)).join('\n').replace(/\bexport /g,'');}
const ast=ts.createSourceFile('types',fs.readFileSync(new URL('types.ts',root),'utf8'),ts.ScriptTarget.Latest,true);
const enums=ast.statements.filter(s=>ts.isEnumDeclaration(s)&&['OrderSide','OrderStatus','PositionSide'].includes(s.name.text)).map(s=>s.getText(ast).replace('export ','')).join('\n');
const context=vm.createContext({structuredClone,nanoid:()=> 'id',Date:{now:()=>0}});
vm.runInContext(ts.transpileModule(enums+'\n'+['utils/clone.ts','utils/safe-math.ts','utils/getTickerPredictedBestBid.ts','utils/getTickerPredictedBestAsk.ts','store/store.base.ts','webworker/chaserOrder/ChaserOrderInstance.ts','webworker/chaserOrder/ChaserOrderManager.ts'].map(source).join('\n')+'\nglobalThis.Store=DefaultStore;globalThis.Instance=ChaserOrderInstance;globalThis.Manager=ChaserOrderManager;', {compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
const bytes=new DataView(new ArrayBuffer(8));
function bits(n){if(Number.isNaN(n))return 'NaN';bytes.setFloat64(0,n);return bytes.getBigUint64(0).toString(16).padStart(16,'0');}
function market(symbol,precision){return {id:'market',symbol,base:'BTC',quote:'USD',active:true,precision:{amount:1,price:precision},limits:{amount:{min:0,max:100},leverage:{min:1,max:100}}};}
function ticker(symbol,bid,ask,last){return {id:'ticker',symbol,bid,ask,last,mark:last,index:last,percentage:0,openInterest:0,fundingRate:0,fundingTime:0,volume:0,quoteVolume:0};}
function run(symbol,side,prices,precision,distance,availability='both',poison=false,duplicate=false,mutate=false,listenerThrow=null){
 const store=new context.Store(),events=[];
 store.update({markets:availability==='ticker'||availability==='none'?[]:[market(poison?'__proto__':symbol,precision)],tickers:availability==='market'||availability==='none'?[]:[ticker(symbol,...prices)]},true);
 if(duplicate)store.update({markets:[market(symbol,precision),market(symbol,precision*2)],tickers:[ticker(symbol,...prices),ticker(symbol,80,85,82)]},true);
 const exchange={name:'bybit',store,emitter:{emit(...args){events.push(args);if(args[0]===listenerThrow)throw new TypeError('listener failed');if(mutate&&args[0]==='log')instance.options.symbol='from-log';if(mutate&&args[0]==='error')instance.options.symbol='from-error';}}};
 const instance=new context.Instance(exchange,{symbol,side,size:1,maxAttempts:1,delay:1,reduceOnly:true});instance.minTickDistance=distance;
 let result;
 try{result={price:bits(new context.Manager().findBestPrice(instance))};}catch(e){result={error:{name:e.name,message:e.message}};}
 return {symbol,side,prices:prices.map(bits),precision:bits(precision),distance:bits(distance),availability,poison,duplicate,mutate,listenerThrow,lookupKinds:[typeof store.marketsBySymbol[symbol],typeof store.tickersBySymbol[symbol]],result,events,stopped:instance.isStopped,reason:instance.state.stopReason};
}
const rows=[];
for(const side of ['buy','sell'])for(const prices of [[100,102,101],[0,0,100],[100,99,101],[NaN,Infinity,-Infinity],[-0,0,-0]])for(const precision of [0,-0,0.01,1,-1,NaN,Infinity])for(const distance of [-1,0,1,2.5,Infinity,NaN])rows.push(run('BTC',side,prices,precision,distance));
const inherited=Object.getOwnPropertyNames(Object.prototype);
for(const symbol of ['missing',...inherited])for(const availability of ['none','market','ticker','both'])rows.push(run(symbol,'buy',[100,102,101],1,1,availability));
for(const side of ['buy','sell'])rows.push(run('BTC',side,[100,102,101],1,1,'both',false,true));
rows.push(run('missing','buy',[100,102,101],1,1,'none',false,false,true));
for(const symbol of ['', ' BTC🚀\t'])for(const availability of ['none','both'])rows.push(run(symbol,'sell',[100,102,101],1,1,availability));
const loneSurrogate=run(JSON.parse('\"\\ud800\"'),'buy',[100,102,101],1,1,'none');
const poisoned=run('precision','buy',[100,102,101],1,1,'both',true);
const listenerThrows=['log','error'].map(event=>run('missing','buy',[100,102,101],1,1,'none',false,false,false,event));
process.stdout.write(JSON.stringify({rows,poisoned,inherited,listenerThrows,loneSurrogate:JSON.stringify(loneSurrogate)}));
