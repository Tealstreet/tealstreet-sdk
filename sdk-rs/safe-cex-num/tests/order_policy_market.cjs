const {root,load}=require('../../safe-cex-types/scripts/accounts-source.cjs');
const pricing=load(root+'/packages/script-api-core/src/order/orderPricing.ts');
const prep=load(root+'/packages/script-api-core/src/order/prepareOrder.ts');
const downsizing=load(root+'/packages/script-api-core/src/order/downSizeReduceOrder.ts');
const metadata=load(root+'/packages/safe-cex/src/exchangeMetadata.ts');
const {ScriptValidationError}=load(root+'/packages/script-api-core/src/order/ScriptValidationError.ts');
function encode(value){
 if(typeof value==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(Number.isNaN(value)?NaN:value);return {$bits:b.toString('hex')};}
 if(value===undefined)return {$undefined:true};
 if(Array.isArray(value))return value.map(encode);
 if(value&&typeof value==='object')return Object.fromEntries(Object.entries(value).map(([k,v])=>[k,encode(v)]));
 return value;
}
const rows=[];
function capture(op,input,run){const logs=[];const old=console.log;console.log=x=>logs.push(x);try{const out=run();rows.push({op,input:encode(input),expected:encode(out),logs});}catch(error){rows.push({op,input:encode(input),error:{name:error.name,message:error.message},logs});}finally{console.log=old;}}
const base={id:'m',symbol:'BTC',base:'BTC',quote:'USDT',active:true,precision:{amount:0.1,price:0.01},limits:{amount:{min:0,max:100},leverage:{min:1,max:100}}};
const nums=[0,-0,0.3,0.35,0.29999999,-0.5,1e-11,5e-324,1e21,NaN,Infinity,-Infinity];
const steps=[0,-0,0.1,0.01,-0.1,NaN,Infinity,-Infinity];
for(const raw of nums)for(const step of steps)for(const isSpot of [false,true])for(const exchangeName of ['hyperliquid','bybit'])for(const op of ['roundOrderPrice','roundOrderSize','roundOrderSizeToNearest','roundMarketCloseSize','roundMarketOrderSize'])for(const reduceOnly of [false,true]){
 const market={...base,isSpot,precision:{amount:step,price:step}};
 capture(op,{raw,market,exchangeName,reduceOnly},()=>op==='roundMarketOrderSize'?pricing[op](raw,market,{reduceOnly}):pricing[op](raw,market,exchangeName));
}
for(const name of [...Object.keys(metadata.exchangeMetadata),'','unknown','constructor','__proto__','toString','hasOwnProperty','__defineGetter__','__defineSetter__','__lookupGetter__','__lookupSetter__','isPrototypeOf','propertyIsEnumerable','toLocaleString','valueOf'])for(const op of ['venueSupportsInfinityStops','venueAutoDownsizesReduceOrders'])capture(op,{name},()=>pricing[op](name));
for(const type of ['market','limit','conditional','stop_market','take_profit_market','trailing_stop_market'])capture('isStopLikeOrderType',{type},()=>pricing.isStopLikeOrderType(type));
for(const type of ['market','limit','conditional','stop_market','take_profit_market','trailing_stop_market'])for(const amount of [0,-0,0.03,0.35,-1,NaN,Infinity])for(const exchangeName of [undefined,'','unknown','bybit','binancespot','hyperliquid','constructor'])for(const reduceOnly of [false,true]){
 const opts={symbol:'BTC',type,side:'buy',amount,reduceOnly,price:123.456,stopLoss:101.234,takeProfit:203.456,triggerPrice:98.765,clientOrderId:'native-client',extension:{tag:'keep-me'}};
 const context={market:base,exchangeName};capture('prepareOrder',{opts,context},()=>prep.prepareOrder(opts,context));
}
for(const field of ['price','stopLoss','takeProfit','triggerPrice'])for(const value of [undefined,0,-0,-1,NaN,Infinity]){
 const opts={symbol:'BTC',type:'limit',side:'sell',amount:1,price:123.456,stopLoss:101.234,takeProfit:203.456,triggerPrice:98.765,[field]:value};
 const context={market:base,exchangeName:'bybit'};capture('prepareOrder',{opts,context},()=>prep.prepareOrder(opts,context));
}
const p={id:'p',symbol:'BTC',side:'short',entryPrice:100,notional:1000,leverage:10,unrealizedPnl:0,contracts:0.006,liquidationPrice:120};
const o={id:'o',symbol:'BTC',status:'open',type:'limit',side:'buy',price:102,amount:0.001,filled:0,remaining:0.001,reduceOnly:true,lastUpdate:0};
for(const contractSize of [0.0001,1,1000,0,-1,NaN])for(const isInverse of [false,true])for(const side of ['buy','sell'])for(const positionMode of ['oneway','hedged'])for(const variant of ['none','one','many','ahead','behind','wrong-symbol','non-reduce','creating','closed','hedged-open','overcommitted']){
 const market={...base,contractSize,isInverse,precision:{amount:0.0001,price:0.01}};
 const order={symbol:'BTC',type:'limit',side,amount:60,reduceOnly:variant!=='non-reduce',price:100,clientOrderId:'native-client'};
 const position={...p,side:side==='buy'?'short':'long',positionMode};
 const positions=variant==='none'?[]:variant==='many'?[position,{...position,id:'p2',contracts:100}]:[position];
 const ahead={...o,side,price:side==='buy'?102:98};
 let orders=[];
 if(variant==='ahead')orders=[ahead];
 if(variant==='behind')orders=[{...ahead,price:side==='buy'?98:102}];
 if(variant==='wrong-symbol')orders=[{...ahead,symbol:'ETH'}];
 if(variant==='creating')orders=[{...ahead,status:'creating'}];
 if(variant==='closed')orders=[{...ahead,status:'closed'}];
 if(variant==='hedged-open')orders=[{...ahead,reduceOnly:false}];
 if(variant==='overcommitted')orders=[{...ahead,amount:1000}];
 capture('downSizeReduceOrder',{market,order,positions,orders,price:100},()=>downsizing.downSizeReduceOrder(market,order,positions,orders,100));
 for(const exchangeName of ['hyperliquid','hyperliquidspot','woo','unknown']){
  const context={market,exchangeName,positions,restingOrders:orders,ticker:{id:'t',symbol:'BTC',bid:99,ask:101,last:101,mark:100,index:100,percentage:0,openInterest:0,fundingRate:0,fundingTime:0,volume:0,quoteVolume:0}};
  capture('clampReduceOnlyOrder',{order,context},()=>prep.clampReduceOnlyOrder(order,context));
 }
}
const fallbackCases=[
 [undefined,undefined,undefined],[0,2,3],[NaN,0,3],[-0,NaN,0],
 [0,NaN,Infinity],[0,-2,3],[0,0,NaN],[1,0,0],
];
for(const [price,mark,last] of fallbackCases){
 const order={symbol:'BTC',type:'limit',side:'buy',amount:10,reduceOnly:true,price};
 const positions=[{...p,contracts:5}];
 const restingOrders=[{...o,price:2.5,amount:1}];
 const ticker=mark===undefined?undefined:{id:'t',symbol:'BTC',bid:1,ask:4,last,mark,index:2,percentage:0,openInterest:0,fundingRate:0,fundingTime:0,volume:0,quoteVolume:0};
 const context={market:base,exchangeName:'hyperliquid',positions,restingOrders,ticker};
 capture('clampReduceOnlyOrder',{order,context},()=>prep.clampReduceOnlyOrder(order,context));
}
for(const message of ['', 'refusal', 'invalid 😀 price'])capture('ScriptValidationError',{message},()=>{const e=new ScriptValidationError(message);return {name:e.name,message:e.message};});
process.stdout.write(JSON.stringify(rows));
