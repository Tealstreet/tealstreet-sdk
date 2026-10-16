'use strict';
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript'),runtime=require('./oracle-runtime.cjs');
const root=path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/woo');
function load(file,names,context={}){
 const ast=ts.createSourceFile(file,fs.readFileSync(path.join(root,file),'utf8'),ts.ScriptTarget.Latest,true);
 const selected=ast.statements.filter(ts.isVariableStatement).filter(n=>n.declarationList.declarations.some(d=>names.includes(d.name.getText(ast))));
 assert.equal(selected.length,names.length,'V3 actual declaration count '+file);
 const exports={};vm.runInNewContext(ts.transpileModule(selected.map(n=>n.getText(ast)).join('\n')+'\nexport { '+names.join(',')+' };',{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,{exports,...context});return exports;
}
const requests=load('woo.v3.request.ts',['serializeQuery','prepareWooV3Request'],{qs:runtime('qs'),createHmac:runtime('create-hmac')});
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
const authRaw='{"apiKey":"key-distinct","apiSecret":"secret-distinct","timestamp":-0}';
const inputRaw='{"method":"POST","path":"/v3/trade/order","query":{"queryField":-0},"body":{"bodyField":1e400}}';
const preparedRaw='{"method":"DELETE","path":"/v3/trade/order?id=19","body":"body-distinct","headers":{"x-api-key":"record-key","x-api-signature":"record-signature"},"signaturePayload":"payload-distinct"}';
const futures={symbol:'PERP_BTC_USDT',indexPrice:'111',markPrice:'113',estFundingRate:'-0.0125',lastFundingRate:'0.025',openInterest:'617','24hOpen':'100','24hClose':'108.27','24hHigh':'139','24hLow':'97','24hVolume':'701','24hAmount':'809',nextFundingTime:11003};
const bbo={symbol:'PERP_BTC_USDT',askPrice:'131',askQuantity:'17',bidPrice:'103',bidQuantity:'19',timestamp:-0};
const trade={symbol:'PERP_BTC_USDT',price:'117',quantity:'23',side:'BUY',source:1,matchedRpi:true,timestamp:12007};
const market={id:'market-distinct',symbol:'BTCUSDT',base:'BTC',quote:'USDT',active:true,precision:{amount:0.1,price:0.2},limits:{amount:{min:0.1,max:10000},leverage:{min:1,max:20}}};
const composed=tickers.composeWooV3AuthoritativeTicker(futures,bbo,trade,market);
assert.equal(composed.bid,103,'V3 source bid');assert.equal(composed.ask,131,'V3 source ask');assert.equal(composed.last,117,'V3 source last');
assert.equal(composed.info.bbo,bbo,'V3 source BBO alias');assert.equal(composed.info.trade,trade,'V3 source Trade alias');
assert.equal(Object.isFrozen(composed),true,'V3 source ticker frozen');assert.equal(Object.isFrozen(composed.info),true,'V3 source info frozen');
const records={auth:{raw:authRaw,native:observe(JSON.parse(authRaw))},input:{raw:inputRaw,native:observe(JSON.parse(inputRaw))},prepared:{raw:preparedRaw,native:observe(JSON.parse(preparedRaw))},bbo:{raw:'{"symbol":"PERP_BTC_USDT","askPrice":"131","askQuantity":"17","bidPrice":"103","bidQuantity":"19","timestamp":-0}',native:observe(bbo)},trade:{raw:JSON.stringify(trade),native:observe(trade)},info:{raw:nativeRaw(composed.info),native:observe(composed.info)},ticker:{raw:nativeRaw(composed),native:observe(composed)}};
assert.equal(Object.keys(records).length,7,'V3 independent record count');
const queryRaws=['{}','{"z":2,"a":"space here"}','{"ids":[9,7,5],"nullable":null,"enabled":true}','{"É":1,"e":2,"é":3,"E":4,"中":5,"𐐀":6}','{"ids":[0,1,2,3,4,5,6,7,8,9,10,11]}','{"negative":-0,"overflow":1e400,"tiny":-1e-4000}','{"nested":{"b":2,"a":1}}'];
const queryCases=queryRaws.map(raw=>({raw,...capture(requests.serializeQuery,JSON.parse(raw))}));
assert.equal(queryCases[2].ok,'enabled=true&ids=9&ids=7&ids=5&nullable=','V3 repeat and null expectation');
const requestSpecs=[
 ['private-get','{"method":"get","path":"/v3/trade/orders","query":{"ids":[9,7],"symbol":"PERP_BTC_USDT"}}',authRaw],
 ['public-get','{"method":"GET","path":"/v3/public/futures","query":{"z":2,"a":"space here"}}',null],
 ['private-post','{"method":"POST","path":"/v3/trade/order","body":{"z":2,"a":-0,"overflow":1e400}}',authRaw],
 ['private-put','{"method":"PUT","path":"/v3/trade/order","body":{"symbol":"PERP_BTC_USDT","quantity":3}}',authRaw],
 ['private-delete','{"method":"DELETE","path":"/v3/trade/order","query":{"id":19}}',authRaw],
 ['empty-secret','{"method":"POST","path":"/v3/trade/order","body":{}}','{"apiKey":"","apiSecret":"","timestamp":1e400}'],
 ['public-body','{"method":"POST","path":"/v3/public/futures","body":{"a":true}}',null],
 ['wrong-path','{"method":"GET","path":"/v1/order","body":{}}',authRaw],
 ['query-body','{"method":"GET","path":"/v3/trade/order","body":{}}',null],
 ['body-query','{"method":"POST","path":"/v3/trade/order","query":{}}',null],
 ['no-auth','{"method":"GET","path":"/v3/account/info"}',null],
 ['null-fields','{"method":"GET","path":"/v3/public/futures","query":null,"body":false}',null],
 ['query-at-public-root','{"method":"GET","path":"/v3/public"}',null],
 ['existing-question','{"method":"GET","path":"/v3/public/futures?seed=1","query":{"a":2}}',null],
 ['body-string','{"method":"POST","path":"/v3/trade/order","body":"raw text"}',authRaw],
 ['unicode-secret','{"method":"POST","path":"/v3/trade/order","body":{"message":"é𐐀"}}','{"apiKey":"clé","apiSecret":"secret𐐀","timestamp":17.5}'],
 ['missing-method','{"path":"/v3/public/futures"}',null],
 ['null-method','{"method":null,"path":"/v3/public/futures"}',null],
 ['number-method','{"method":3,"path":"/v3/public/futures"}',null],
 ['missing-path','{"method":"GET"}',null],
 ['null-path','{"method":"GET","path":null}',null],
 ['number-path','{"method":"GET","path":3}',null],
 ['unsupported-method','{"method":"HEAD","path":"/v3/public/futures"}',null],
];
const requestCases=requestSpecs.map(([label,raw,auth])=>({label,raw,auth,...capture(requests.prepareWooV3Request,JSON.parse(raw),auth?JSON.parse(auth):undefined)}));
assert.equal(requestCases.length,23,'V3 independent request count');
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
const prepared=requests.prepareWooV3Request(JSON.parse(requestSpecs[0][1]),JSON.parse(authRaw));
assert.equal(Object.isFrozen(prepared),true,'V3 prepared frozen');assert.equal(Object.isFrozen(prepared.headers),true,'V3 prepared headers frozen');
let preparedFreezeError;try{prepared.path='changed';}catch(error){preparedFreezeError=error.name;}
assert.equal(preparedFreezeError,'TypeError','V3 prepared frozen write');
process.stdout.write(JSON.stringify({capture:{architecture:process.arch,node:process.version,nan:'canonical-only',scope:'actual request and ticker functions; actual qs/HMAC and public stats mapper; no I/O'},records,queryCases,requestCases,normalized,priceCases,tickerCases,alias:{before,after:observe(composed),freezeError,preparedFreezeError}}));
