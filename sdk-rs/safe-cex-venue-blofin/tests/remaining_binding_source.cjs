// Full maintained constructor, real DefaultStore and API; only terminal Axios is scripted.
const assert = require('node:assert/strict');
const path = require('node:path');
const root = path.resolve(__dirname, '../../..');
const {BlofinExchange} = require(path.join(root, 'packages/safe-cex/src/exchanges/blofin/blofin.exchange.ts'));
const {DefaultStore} = require(path.join(root, 'packages/safe-cex/src/store/store.base.ts'));
const market = {id:'BTC-USDT',symbol:'BTCUSDT',base:'BTC',quote:'USDT',active:true,precision:{amount:1,price:0.1},limits:{amount:{min:1,max:100},leverage:{min:1,max:100}},contractSize:1000,info:{lotSize:'1',minSize:'1',contractValue:1000}};
const order = kind => ({id:kind==='algo'?'a__stop_loss':kind==='trigger'?'t':'n',symbol:'BTCUSDT',status:'open',type:kind==='algo'?'stop_market':kind==='trigger'?'conditional':'limit',side:'buy',amount:2,filled:0,remaining:2,price:100,lastUpdate:1710000000000,reduceOnly:false,...(kind==='trigger'?{info:{instId:'BTC-USDT',orderType:'trigger',algoId:'t'}}:{})});
const closed = {instId:'BTC-USDT',historyId:'h',positionId:'p',positionSide:'long',closePositions:'2',maxPositions:'3',fee:'-0.5',realizedPnl:'9.5',createTime:'1700000000000',updateTime:'1700000001000',openAveragePrice:'100',closeAveragePrice:'105',leverage:'4'};
const cases = ['active-orders','tickers','history','leverage','set-leverage','margin','cancel-normal','cancel-algo','cancel-trigger','journal'];
(async()=>{
 const results=[];
 for(const kind of cases){
  const store=new DefaultStore(); store.update({markets:[market]});
  const venue=new BlofinExchange('blofin',{key:'k',secret:'s',passphrase:'p',tuning:{consoleLogging:false}},store);
  const requests=[],events=[];
  venue.on('log',(...args)=>events.push(args));
  venue.afterMethodCalledInWorker(null,'changePositionMode',kind==='set-leverage');
  venue.afterMethodCalledInWorker(null,'changeMarginType',kind==='set-leverage');
  if(kind.startsWith('cancel-'))store.update({orders:[order(kind.slice(7))]});
  for(const client of new Set([venue.xhr,venue.unlimitedXHR,venue._primaryXhr,venue._secondaryXhr].filter(Boolean)))client.defaults.adapter=async config=>{
   const body=typeof config.data==='string'?JSON.parse(config.data):config.data;
   requests.push({method:config.method.toUpperCase(),path:config.url,params:config.params??null,body:body??null});
   let data=[],code='0';
   switch(config.url){
    case '/api/v1/market/tickers':data=[{instId:'BTC-USDT',last:'110',open24h:'100',bidPrice:'109',askPrice:'111',volCurrency24h:'12',vol24h:'1320'}];break;
    case '/api/v1/trade/orders-history':break;
    case '/api/v1/trade/orders-pending':data=config.params?.after?[]:[{orderId:'npage'}];break;
    case '/api/v1/trade/orders-tpsl-pending':data=config.params?.after?[]:[{tpslId:'apage'}];break;
    case '/api/v1/trade/orders-algo-pending':data=config.params?.after?[]:[{algoId:'tpage'}];break;
    case '/api/v1/account/batch-leverage-info':data=[{instId:'BTC-USDT',positionSide:'net',marginMode:'cross',leverage:'7'}];break;
    case '/api/v1/account/set-leverage':break;
    case '/api/v1/account/set-margin-mode': data={marginMode:'isolated'};break;
    case '/api/v1/account/margin-mode':data={marginMode:'isolated'};break;
    case '/api/v1/trade/cancel-batch-orders':code='1';data=[{orderId:'n',code:'1000',msg:'does not exist'}];break;
    case '/api/v1/trade/cancel-tpsl':code='1';data=[{tpslId:'a',code:'1000',msg:'does not exist'}];break;
    case '/api/v1/trade/cancel-algo':data={algoId:'t',code:'1000',msg:'does not exist'};break;
    case '/api/v1/account/positions-history':data=[closed];break;
    default:throw new Error('unscripted '+config.url);
   }
   return {data:{code,data,msg:'success'},status:200,statusText:'OK',headers:{},config};
  };
  try{
   let pending;
   if(kind==='active-orders')pending=venue.fetchOrders();
   else if(kind==='tickers')pending=venue.fetchTickers();
   else if(kind==='history')pending=venue.fetchHistoricalOrders({symbol:'BTCUSDT',from:100,to:200,limit:7});
   else if(kind==='leverage')pending=venue.fetchLeverageForSymbol('BTCUSDT');
   else if(kind==='set-leverage')pending=venue.setLeverage('BTCUSDT',300);
   else if(kind==='margin')pending=venue.changeMarginType(true);
   else if(kind==='journal')pending=venue.fetchHistoricalPositions({account:'account'});
   else pending=venue.cancelOrders(store.orders);
   const prefix={events:events.length,requests:requests.length};
   const result=await pending;
   if(kind==='margin')assert.equal(result,undefined,'actual worker margin producer returns void');
   assert.equal(prefix.requests,0,'Axios terminal adapter runs in a reaction');
   if(kind==='active-orders'){assert.equal(requests.length,6);assert.equal(result.length,0);}
   if(kind==='set-leverage'){assert.equal(requests.length,2);assert.deepEqual(requests.map(r=>r.body.positionSide),['long','short']);assert.equal(store.fetchedLeverage.BTCUSDT.buyLeverage,100);}
   if(kind==='leverage')assert.equal(store.fetchedLeverage.BTCUSDT.buyLeverage,7);
   if(kind.startsWith('cancel-'))assert.equal(store.orders.length,0);
   if(kind==='journal'){assert.equal(result.length,1);assert.equal(result[0].closedSize,2);assert.equal(result[0].settleCurrency,'USDT');}
   results.push({kind,prefix,requests,result:result??null,leverage:store.fetchedLeverage,orders:store.orders.map(o=>o.id)});
  }finally{venue.dispose();}
 }
 process.stdout.write(JSON.stringify({market,cases:results}));
})().catch(error=>{console.error(error);process.exitCode=1});
