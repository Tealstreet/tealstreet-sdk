// The maintained complete venue/Store; only terminal Axios I/O is scripted.
const assert = require('node:assert/strict');
const path = require('node:path');
const root = path.resolve(__dirname,'../../..');
const {BlofinExchange} = require(path.join(root,'packages/safe-cex/src/exchanges/blofin/blofin.exchange.ts'));
const {DefaultStore} = require(path.join(root,'packages/safe-cex/src/store/store.base.ts'));
const market = {id:'BTC-USDT',symbol:'BTCUSDT',base:'BTC',quote:'USDT',active:true,precision:{amount:1,price:0.1},limits:{amount:{min:1,max:100},leverage:{min:1,max:100}},contractSize:0.001,info:{lotSize:'1',minSize:'1',contractValue:0.001}};
const profiles = ['normal','algo','trigger','batch'];
(async()=>{
 const results = [];
 for(const kind of profiles){
  const store = new DefaultStore(); store.update({markets:[market]});
  const venue = new BlofinExchange('blofin',{key:'k',secret:'s',passphrase:'p',tuning:{consoleLogging:false}},store);
  const requests = [];
  for(const client of new Set([venue.xhr,venue.unlimitedXHR,venue._primaryXhr,venue._secondaryXhr].filter(Boolean))) client.defaults.adapter = async config=>{
   const data = typeof config.data === 'string' ? JSON.parse(config.data) : config.data;
   const row = Array.isArray(data) ? data[0] : data;
   requests.push({path:config.url,marginMode:row.marginMode,positionSide:row.positionSide,side:row.side,size:row.size,type:row.orderType??null,trigger:row.triggerPrice??null});
   let response;
   if(config.url === '/api/v1/trade/batch-orders') response=[{orderId:'n1',code:'0',msg:'success'}];
   else if(config.url === '/api/v1/trade/order-tpsl') response={tpslId:'a1',code:'0',msg:'success'};
   else if(config.url === '/api/v1/trade/order-algo') response={algoId:'t1',code:'0'};
   else throw new Error('unscripted '+config.url);
   return {data:{code:'0',msg:'success',data:response},status:200,statusText:'OK',headers:{},config};
  };
  try {
   venue.afterMethodCalledInWorker(null,'changePositionMode',false);
   venue.afterMethodCalledInWorker(null,'changeMarginType',false);
   const makeOpts = kind=>({symbol:'BTCUSDT',type:kind==='normal'?'limit':kind==='algo'?'stop_market':'conditional',side:'buy',amount:2,price:100, ...(kind==='trigger'?{triggerPrice:105}:{})});
   const pending = kind==='batch' ? venue.placeOrders(['normal','algo','trigger'].map(makeOpts)) : venue.placeOrder(makeOpts(kind));
   const prefix = requests.length;
   // A legitimate worker acknowledgement arrives before the next Promise reaction.
   venue.afterMethodCalledInWorker(null,kind==='algo'?'changeMarginType':'changePositionMode',true);
   if(kind==='batch') venue.afterMethodCalledInWorker(null,'changeMarginType',true);
   const ids = await pending;
   assert.equal(prefix,0); assert.equal(requests.length,kind==='batch'?3:1);
   if(kind==='algo') assert.equal(requests[0].marginMode,'isolated');
   else assert.equal(requests[0].positionSide,'long');
   if(kind==='batch') assert.ok(requests.every(row=>row.marginMode==='isolated'));
   results.push({kind,prefix,ids,requests});
  } finally {venue.dispose();}
 }
 process.stdout.write(JSON.stringify(results));
})().catch(error=>{console.error(error);process.exitCode=1});
