// Whole maintained constructor/Store/API; only Axios' terminal adapter is scripted.
const assert = require('node:assert/strict');
const path = require('node:path');
const root = path.resolve(__dirname, '../../..');
const { WeexExchange } = require(path.join(root, 'packages/safe-cex/src/exchanges/weex/weex.exchange.ts'));
const { DefaultStore } = require(path.join(root, 'packages/safe-cex/src/store/store.base.ts'));
const market = {id:'BTCUSDT',symbol:'BTCUSDT',base:'BTC',quote:'USDT',active:true,precision:{amount:.001,price:.1},limits:{amount:{min:.001,max:50},leverage:{min:1,max:80}}};
const order = {orderId:'audit-1',symbol:'BTCUSDT',side:'SELL',type:'LIMIT',status:'PARTIALLY_FILLED',origQty:'2',price:'101.5',executedQty:'.25',timeInForce:'GTC',updateTime:1700000000000,tpOrderType:'MARK_PRICE',workingType:'CONTRACT_PRICE'};
const deep = JSON.parse('['.repeat(150)+'0'+']'.repeat(150));
const cases = [
 {id:'candle-aliases',method:'candles',routes:{'/capi/v3/market/klines':[{t:1700000000000,o:'11',h:'13',l:'7',c:'12',v:'5'}]}},
 {id:'candle-defaults',method:'candles',routes:{'/capi/v3/market/klines':[{time:1700000000,open:null,high:'bad',low:'',close:false}]}},
 {id:'book-levels',method:'book',routes:{'/capi/v3/market/depth':{bids:[['1','2'],['.5','3']],asks:[['3','4']]}}},
 {id:'ticker-percent',method:'tickers',routes:{'/capi/v3/market/ticker/24hr':[{symbol:'BTCUSDT',lastPrice:'12',openPrice:'10',priceChangePercent:'.25',volume:'5'}],'/capi/v3/market/ticker/bookTicker':[]}},
 {id:'ticker-fallback',method:'tickers',routes:{'/capi/v3/market/ticker/24hr':[{symbol:'BTCUSDT',lastPrice:'12',openPrice:'10',priceChangePercent:'bad'}],'/capi/v3/market/ticker/bookTicker':[]}},
 {id:'order-enums',method:'orders',routes:{'/capi/v3/openOrders':[order],'/capi/v3/openAlgoOrders':[]}},
 {id:'order-defaults',method:'orders',routes:{'/capi/v3/openOrders':[{...order,type:'UNKNOWN',status:'UNKNOWN',side:'BUY',tpOrderType:'UNKNOWN',workingType:'UNKNOWN'}],'/capi/v3/openAlgoOrders':[]}},
 {id:'position-enums',method:'positions',routes:{'/capi/v3/account/position/allPosition':[{symbol:'BTCUSDT',positionSide:'SHORT',size:'-2',openValue:'200',leverage:'11',marginType:'ISOLATED',updatedTime:1700000000000}]}},
 {id:'config-enums',method:'leverage',routes:{'/capi/v3/account/symbolConfig':[{symbol:'BTCUSDT',marginType:'ISOLATED',separatedMode:'SEPARATED',crossLeverage:'20',isolatedLongLeverage:'11',isolatedShortLeverage:'13'}]}},
 {id:'fills-typed',method:'fills',routes:{'/capi/v3/userTrades':[{symbol:'BTCUSDT',tradeId:'fill-audit',orderId:'audit-1',side:'BUY',price:'101.5',qty:'2',time:1700000000000,maker:false,commissionAsset:'USDT',commission:'.1'}]}},
 {id:'candle-unread-deep',method:'candles',routes:{'/capi/v3/market/klines':[{timestamp:1700000000000,open:'11',high:'13',low:'7',close:'12',volume:'5',ignored:deep}]}},
 {id:'book-unread-deep',method:'book',routes:{'/capi/v3/market/depth':{bids:[['1','2'],['.5','3']],asks:[['3','4']],ignored:deep}}},
 {id:'book-unread-tuple',method:'book',routes:{'/capi/v3/market/depth':{bids:[['1','2',deep],['.5','3',deep]],asks:[['3','4',deep]]}}},
];
const keys = {
 candles:['timestamp','open','high','low','close','volume'],
 book:['bids','asks'],tickers:['symbol','last','percentage','volume'],
 orders:['id','side','type','status','lastUpdate','tpTriggerType','slTriggerType'],
 positions:['symbol','side','contracts','entryPrice','marginType'],
 leverage:['buyLeverage','sellLeverage'],fills:['id','orderId','side','price','qty','timestamp','maker','commissionAsset']
};
function view(method, value) {
 const one = x => Object.fromEntries(keys[method].filter(k=>Object.hasOwn(x,k)).map(k=>[k,x[k]]));
 return Array.isArray(value)?value.map(one):one(value);
}
(async()=>{
 const observations=[];
 for(const c of cases){
  const store=new DefaultStore();store.update({markets:[market]});
  const venue=new WeexExchange('weex',{key:'synthetic-key',secret:'synthetic-secret',passphrase:'synthetic-passphrase',tuning:{consoleLogging:false}},store);
  const requests=[];
  venue.xhr.defaults.adapter=async config=>{
   assert.ok(Object.hasOwn(c.routes,config.url),`${c.id}: unscripted ${config.url}`);
   requests.push(config.url);
   return {data:c.routes[config.url],status:200,statusText:'OK',headers:{},config};
  };
  try {
   let result;
   switch(c.method){
    case 'candles':result=await venue.fetchOHLCV({symbol:'BTCUSDT',interval:'1m'});break;
    case 'book':result=await venue.fetchOrderBook('BTCUSDT');break;
    case 'tickers':result=await venue.fetchTickers();break;
    case 'orders':result=await venue.fetchOrders();break;
    case 'positions':result=await venue.fetchPositions(['BTCUSDT']);break;
    case 'leverage':result=await venue.fetchLeverageForSymbol('BTCUSDT');break;
    case 'fills':result=await venue.fetchHistoricalFills({symbol:'BTCUSDT'});break;
    default:assert.fail(c.method);
   }
   assert.equal(requests.length,Object.keys(c.routes).length,c.id);
   observations.push({id:c.id,method:c.method,routeBodies:Object.fromEntries(Object.entries(c.routes).map(([k,v])=>[k,JSON.stringify(v)])),result:view(c.method,result),requests});
  }finally{venue.dispose();}
 }
 assert.equal(observations.length,13);
 assert.equal(observations[0].result[0].open,11);
 assert.equal(observations[1].result[0].volume,0);
 assert.equal(observations[3].result[0].percentage,25);
 assert.equal(observations[4].result[0].percentage,20);
 assert.equal(observations[5].result[0].side,'sell');
 assert.deepEqual(observations[8].result,{buyLeverage:11,sellLeverage:13});
 assert.equal(observations[9].result[0].maker,false);
 process.stdout.write(JSON.stringify(observations));
})().catch(error=>{console.error(error);process.exitCode=1;});
