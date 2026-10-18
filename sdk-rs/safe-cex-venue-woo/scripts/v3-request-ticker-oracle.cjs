'use strict';
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/woo');
function load(file,names,context={}){
 const ast=ts.createSourceFile(file,fs.readFileSync(path.join(root,file),'utf8'),ts.ScriptTarget.Latest,true);
 const selected=ast.statements.filter(ts.isVariableStatement).filter(n=>n.declarationList.declarations.some(d=>names.includes(d.name.getText(ast))));
 assert.equal(selected.length,names.length,'V3 actual declaration count '+file);
 const exports={};vm.runInNewContext(ts.transpileModule(selected.map(n=>n.getText(ast)).join('\n')+'\nexport { '+names.join(',')+' };',{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,{exports,...context});return exports;
}
const publicApi=load('woo.v3.public.ts',['requireFiniteNumber','mapWooV3FuturesStats']);
const tickers=load('woo.v3.ticker.ts',['normalizePerpetualSymbol','requirePositiveFinitePrice','composeWooV3AuthoritativeTicker'],publicApi);
function observe(value){
 if(value===undefined)return {$undefined:true};
 if(typeof value==='number'){const bytes=Buffer.alloc(8);bytes.writeDoubleBE(value);return {$bits:Number.isNaN(value)?'7ff8000000000000':bytes.toString('hex')};}
 if(Array.isArray(value))return Array.from(value,observe);
 if(value&&typeof value==='object')return Object.fromEntries(Object.entries(value).map(([key,value])=>[key,observe(value)]));
 return value;
}
function nativeRaw(value){
 if(typeof value==='number')return Object.is(value,-0)?'-0':value===Infinity?'1e400':value===-Infinity?'-1e400':JSON.stringify(value);
 if(Array.isArray(value))return '['+value.map(nativeRaw).join(',')+']';
 if(value&&typeof value==='object')return '{'+Object.entries(value).map(([key,value])=>JSON.stringify(key)+':'+nativeRaw(value)).join(',')+'}';
 return JSON.stringify(value);
}
function capture(fn,...args){try{return {ok:observe(fn(...args))};}catch(e){return {error:{name:e.name,message:e.message}};}}
const inputRaw='{"method":"POST","path":"/v3/trade/order","query":{"queryField":-0},"body":{"bodyField":1e400}}';
const futures={symbol:'PERP_BTC_USDT',indexPrice:'111',markPrice:'113',estFundingRate:'-0.0125',lastFundingRate:'0.025',openInterest:'617','24hOpen':'100','24hClose':'108.27','24hHigh':'139','24hLow':'97','24hVolume':'701','24hAmount':'809',nextFundingTime:11003};
const bbo={symbol:'PERP_BTC_USDT',askPrice:'131',askQuantity:'17',bidPrice:'103',bidQuantity:'19',timestamp:-0};
const trade={symbol:'PERP_BTC_USDT',price:'117',quantity:'23',side:'BUY',source:1,matchedRpi:true,timestamp:12007};
const market={id:'market-distinct',symbol:'BTCUSDT',base:'BTC',quote:'USDT',active:true,precision:{amount:0.1,price:0.2},limits:{amount:{min:0.1,max:10000},leverage:{min:1,max:20}}};
const composed=tickers.composeWooV3AuthoritativeTicker(futures,bbo,trade,market);
assert.equal(composed.bid,103,'V3 source bid');assert.equal(composed.ask,131,'V3 source ask');assert.equal(composed.last,117,'V3 source last');
assert.equal(composed.info.bbo,bbo,'V3 source BBO alias');assert.equal(composed.info.trade,trade,'V3 source Trade alias');
assert.equal(Object.isFrozen(composed),true,'V3 source ticker frozen');assert.equal(Object.isFrozen(composed.info),true,'V3 source info frozen');
const records={input:{raw:inputRaw,native:observe(JSON.parse(inputRaw))},bbo:{raw:'{"symbol":"PERP_BTC_USDT","askPrice":"131","askQuantity":"17","bidPrice":"103","bidQuantity":"19","timestamp":-0}',native:observe(bbo)},trade:{raw:JSON.stringify(trade),native:observe(trade)},info:{raw:nativeRaw(composed.info),native:observe(composed.info)},ticker:{raw:nativeRaw(composed),native:observe(composed)}};
assert.equal(Object.keys(records).length,5,'V3 independent record count');
const normalizations=['PERP_BTC_USDT','PERP_1000SHIB_USDT','PERP_BTC2_USDC','BTCUSDT','PERP_btc_USDT','PERP__USDT','PERP_BTC_','PERP_BTC_USDT_EXTRA','PERP_BTC_USDT\n','PERP_É_USDT'];
const normalized=normalizations.map(input=>({input,...capture(tickers.normalizePerpetualSymbol,input,'symbol-path')}));
const prices=['103','0x64','0b10','\ufeff 3.5 ','1e308','5e-324','0','-0','-1','1e400','-1e-4000','Infinity','NaN','','\u008510'];
const priceCases=prices.map(input=>({input,...capture(tickers.requirePositiveFinitePrice,input,'price-path')}));
const tickerSpecs=[['ordinary',{}],['futures-symbol',{futures:{symbol:'bad'}}],['bbo-symbol',{bbo:{symbol:'bad'}}],['trade-symbol',{trade:{symbol:'bad'}}],['bbo-mismatch',{bbo:{symbol:'PERP_ETH_USDT'}}],['trade-mismatch',{trade:{symbol:'PERP_ETH_USDT'}}],['market-mismatch',{market:{symbol:'ETHUSDT'}}],['stats-before-bid',{futures:{'24hOpen':'0'},bbo:{bidPrice:'0'}}],['bid-before-ask',{bbo:{bidPrice:'0',askPrice:'0'}}],['ask-before-last',{bbo:{askPrice:'0'},trade:{price:'0'}}],['last-refusal',{trade:{price:'0'}}],['overflow-percentage',{futures:{'24hOpen':'1e-300','24hClose':'1e308'}}],['radix-price',{bbo:{bidPrice:'0x64',askPrice:'0b10000000'}}],['whitespace-price',{trade:{price:'\ufeff117'}}]];
const tickerCases=tickerSpecs.map(([label,patch])=>{const args=[{...futures,...patch.futures},{...bbo,...patch.bbo},{...trade,...patch.trade},{...market,...patch.market}];return {label,args:args.map(nativeRaw),...capture(tickers.composeWooV3AuthoritativeTicker,...args)};});
assert.equal(tickerCases.length,14,'V3 independent ticker count');
const before=observe(composed);bbo.askPrice='changed-after-composition';trade.quantity='changed-after-composition';
assert.equal(composed.ask,131,'V3 source computed ask snapshot');assert.equal(composed.info.bbo.askPrice,'changed-after-composition','V3 source live BBO info');
let freezeError;try{composed.bid=999;}catch(error){freezeError=error.name;}
assert.equal(freezeError,'TypeError','V3 ticker frozen write');
process.stdout.write(JSON.stringify({capture:{architecture:process.arch,node:process.version,nan:'canonical-only',scope:'actual ticker functions and public stats mapper; no I/O'},records,normalized,priceCases,tickerCases,alias:{before,after:observe(composed),freezeError}}));
