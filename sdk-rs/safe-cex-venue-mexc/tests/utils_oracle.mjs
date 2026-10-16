import {load} from './source.mjs';
const api=load('packages/safe-cex/src/exchanges/mexc/mexc.utils.ts');
if(process.argv.includes('--currency-order')) {
  const rows=[{currency:'USDT',free:1},{currency:'USDC',free:2}];
  process.stdout.write(JSON.stringify(Object.keys(api.normalizeBalance(rows).currencies)));
  process.exit(0);
}
const number=value=>{
  if(Number.isNaN(value))return {$number:'7ff8000000000000'};
  const bytes=Buffer.alloc(8);bytes.writeDoubleBE(value);return {$number:bytes.toString('hex')};
};
const encode=value=>typeof value==='number'?number(value):Array.isArray(value)?value.map(encode):value&&typeof value==='object'?Object.fromEntries(Object.entries(value).map(([k,v])=>[k,encode(v)])):value;
const base={symbol:'BTC_USDT',contractSize:0.0001,volUnit:3,minVol:2,maxVol:100000,priceScale:5,minLeverage:4,maxLeverage:100};
const cases=[];
function run(name,args,marketRaw) {
  try {
    const market=marketRaw===undefined?undefined:api.normalizeMarket(marketRaw);
    const fnArgs=marketRaw===undefined?args:[...args,market];
    cases.push({name,args:encode(args),marketRaw,output:encode(api[name](...fnArgs))});
  } catch(error){cases.push({name,args:encode(args),marketRaw,error:{name:error.name,message:error.message}});}
}
const inputs=[undefined,null,'',0,-0,1,-2,true,false,'12abc','0x10','1e400','-1e400','NaN','Infinity','\uFEFF12\uFEFF','\u008512',[],[2],[1,2],{}, {toString:null},'short','SELL','one-way','isolated','isolate','constructor','__proto__','toString'];
for(const value of inputs)for(const name of ['toNumber','toIntegerString','fromMexcSymbol','normalizePositionSide','normalizePositionMode','normalizeMarginType','normalizePercentage'])run(name,[value]);
for(const symbol of ['', 'BTCUSDT','BTC_USDT','ETHUSDC','USDT','BUSD','BTCUSD','sélLUSDT','BTC___USDT'])run('toMexcSymbol',[symbol]);
for(const scale of [undefined,null,'',0,1,2,8,-1,1.5,'2foo','1e2','0x2','constructor',{}, {toString:null}])run('stepFromScale',[scale,0.123]);
const markets=[base,{}, {symbol:'EMPTY_USDT',maxVol:'',contractSize:-0,volUnit:0.5}, {symbol:'BAD_USDT',maxVol:'NaN'}, {symbol:'ETH_USDC',state:1,contractSize:0,volUnit:0.5}, {symbol:'ONE_TWO_THREE',baseCoin:'',quoteCoin:'USDT',contractSize:-2,volUnit:3,minVol:-3}, {symbol:123,contractSize:'1e308',volUnit:20,maxVol:'5e307'}, {symbol:'X_USDT',contractSize:0.1,priceScale:2,priceUnit:'0x10',maxVol:null,limitMaxVol:9}, {symbol:'Z_USDC',state:null,apiAllowed:false}, {symbol:'Q_USDC',quoteCoin:'usdc',isHidden:true}, {settleCoin:'USDT',priceScale:{toString:null}}, {symbol:'A_USDC',settleCoin:'usdc',apiAllowed:0,isHidden:1}];
for(const raw of markets){run('normalizeMarket',[raw]);run('isTradableMexcMarket',[raw]);}
for(const raw of markets.filter(m=>typeof m.priceScale !== 'object')) {
  run('getContractSize',[],raw);
  for(const v of ['1e308',0,-0,3,'12abc'])run('nativeVolumeToAmount',[v],raw);
  for(const v of [1e308,0,-0,NaN,Infinity])run('amountToNativeVolume',[v],raw);
}
for(const raw of [
  {bids:[[1,2],[3,4],[1,5],[0,9],[-1,2],[2,-1]],asks:[[5,2],[4,1],[4,2]]},
  {data:{bid:[{p:'2',v:'3'},{price:'1',vol:'4'},{price:5,amount:'1e308'},null,{},[3]],ask:[[8,9]]}},
  {data:false,bids:[[2,3]],asks:[]},{data:true,bids:[[2,3]]},{data:[]},
  {bids:[[1,'1e308'],[2,'1e308']],asks:[[3,'1e308'],[4,'1e308']]},
])for(const m of [base,{symbol:'X_USDT',contractSize:'1e308'}])run('normalizeOrderBook',[raw],m);
for(const raw of [[],null,{},[{currency:'USDT',availableBalance:0,equity:2},{currency:'USDC',availableBalance:5,equity:10}], [{currency:'USDC',available:3,frozen:8},{currency:'USDT',free:7},{currency:'USDC',free:4}], [{currency:'__proto__',free:1},{currency:'constructor',free:2},{currency:'BTC',free:3}], [null],[true],[{currency:{toString:null}},null],[{currency:'USDT',free:{toString:null}},null],[{currency:'USDT',free:1,unrealized:{toString:null}},null], [{currency:'USDT',free:'1e308',total:'-1e308',frozen:'5e307'}]])run('normalizeBalance',[raw]);
for(const state of [undefined,null,0,1,2,3,4,5,6,'2','2bad','constructor'])run('normalizeTriggerOrderStatus',[{state}]);
for(const type of [undefined,null,0,1,2,'1abc','constructor']){
 for(const side of ['buy','sell'])run('normalizePlanOrderType',[{triggerType:type},side]);
 run('normalizeStopOrderType',[{triggerSide:type,takeProfitPrice:10}]);
}
for(const mode of [undefined,'hedged','oneway'])for(const margin of [undefined,'cross','isolated'])for(const leverage of [undefined,0,20,NaN,Infinity,{long:3,short:9},{short:0}]) {
  const market=api.normalizeMarket(base),args=[market,mode,margin,leverage];
  cases.push({name:'fakePositionsForMarket',args:encode([mode,margin,leverage]),marketRaw:base,output:encode(api.fakePositionsForMarket(...args))});
}
const dateValues=[undefined,null,'',0,-0,1,-1,-2,999999999999,1000000000000,1700000000.999,1e308,'1700000000','0x10','12abc','1e400',true,false,[],[2024,1,2],{}, {toString:null},
 '2024-01-02','2024-01-02T03:04:05Z','2024-01-02T03:04:05','2024-03-10T02:30:00','2024-11-03T01:30:00','2024-01-02T03:04:05+08:00','Tue, 02 Jan 2024 03:04:05 GMT','01/02/2024','2024-02-30','2024-13-01','\uFEFF2024-01-02','\u00852024-01-02','-000001-01-01','+275760-09-13','+275760-09-14'];
for(const value of dateValues)for(const name of ['normalizeTimestampToMs','normalizeTimestampToSeconds'])run(name,[value]);
for(const time of ['2024-01-02T03:04:05Z','2024-01-02T03:04:05','bad',0,-1,'1e308']) {
 run('normalizeTicker',[{lastPrice:'1e308',fairPrice:0,holdVol:'5e307',fundingTime:time,riseFallRate:0.03}],base);
 run('normalizeCandle',[{time,open:'1',high:2,low:0,close:'12abc',vol:5}]);
}
for(const raw of [{},null,[],{data:[{time:1700000000,o:1,h:2,l:0,c:1.25,q:3},{time:0}]},{data:[null]},
 {time:[1700000000,'2024-01-02',0],open:[1,2],high:[3],low:[],close:[2,3],amount:[4,5]},
 {data:{time:[1700000000,1700000001],vol:[null,2],amount:[3,4]}}, {data:{time:[1700000000],open:'12',high:{0:4},close:true}}])run('normalizeKlineResponse',[raw]);
for(const open of ['😃','😃7'])run('normalizeKlineResponse',[{time:[1700000000,1700000001,1700000002],open}]);
// Undefined positional arguments must survive the JSON transport to the Rust test.
function preserve(value){if(value===undefined)return {$undefined:true};if(Array.isArray(value))return value.map(preserve);if(value&&typeof value==='object')return Object.fromEntries(Object.entries(value).map(([k,v])=>[k,preserve(v)]));return value;}
process.stdout.write(JSON.stringify(preserve(cases).map(item=>({captureArchitecture:process.arch,captureNodeMajor:Number(process.versions.node.split('.')[0]),...item}))));
