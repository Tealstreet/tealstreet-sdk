// Construct maintained venue/Base/Store/API/socket classes. Only terminal Axios
// I/O is scripted; every request still crosses the actual source interceptors.
const assert = require('node:assert/strict');
const path = require('node:path');
const {root, load} = require('./inverse-source-loader.cjs');
const {BinanceInverseExchange} = load(path.join(root, 'packages/safe-cex/src/exchanges/binance/binanceinverse.exchange.ts'));
const {DefaultStore} = load(path.join(root, 'packages/safe-cex/src/store/store.base.ts'));
const market = {symbol:'BTCUSD_PERP',baseAsset:'BTC',quoteAsset:'USD',marginAsset:'BTC',contractSize:100,contractType:'PERPETUAL',contractStatus:'TRADING',filters:[{filterType:'PRICE_FILTER',tickSize:'0.1'},{filterType:'LOT_SIZE',stepSize:'1',minQty:'1',maxQty:'10'},{filterType:'MARKET_LOT_SIZE',minQty:'2',maxQty:'20'}]};
const position = {symbol:'BTCUSD_PERP',positionAmt:'100',positionSide:'BOTH',entryPrice:'4000',markPrice:'5000',unRealizedProfit:'0.25',liquidationPrice:'2500',leverage:'10',isolatedMargin:'0',marginType:'cross'};
const routes = new Map([
  ['/dapi/v1/exchangeInfo',{symbols:[market,{contractType:'CURRENT_QUARTER',contractStatus:'TRADING'}]}],
  ['/dapi/v2/leverageBracket',[{symbol:'BTCUSD_PERP',brackets:[{initialLeverage:75}]}]],
  ['/dapi/v1/positionRisk',[position]],
  ['/dapi/v1/order','{"orderId":9007199254740993}'],
]);
(async () => {
  const store = new DefaultStore();
  const venue = new BinanceInverseExchange('binanceinverse', {key:'synthetic-key',secret:'synthetic-secret'}, store);
  const calls = [];
  for (const client of new Set([venue.xhr,venue._primaryXhr,venue._secondaryXhr,venue.publicXhr,venue.unlimitedXHR])) {
    client.defaults.adapter = async config => {
      assert.ok(routes.has(config.url), 'unscripted source URL ' + config.url);
      calls.push({url:config.url,method:config.method,baseURL:config.baseURL,headers:config.headers.toJSON(),skip:config.safeCexSkipRemoteSigning === true,params:config.params});
      const data = routes.get(config.url);
      return {status:200,statusText:'OK',headers:{'content-type':'application/json'},config,data:typeof data === 'string' ? data : JSON.stringify(data)};
    };
  }
  try {
    const markets = await venue.fetchMarkets();
    assert.equal(markets.length,1);
    store.update({markets,tickers:[{id:'BTC/USD:BTC',symbol:'BTCUSD_PERP',bid:4999,ask:5001,last:5000,mark:5000,index:5000,percentage:0,fundingRate:0,fundingTime:0,volume:1,quoteVolume:5000,openInterest:0}]});
    const positions = await venue.fetchPositions();
    const ids = await venue.placeOrder({symbol:'BTCUSD_PERP',side:'buy',type:'limit',amount:2,price:5000.16});
    assert.equal(calls.length,4);
    assert.equal(calls[0].skip,true);
    assert.equal(calls[0].headers['X-MBX-APIKEY'],undefined);
    assert.ok(calls[1].params.signature);
    assert.equal(calls[3].params.quantity,'2');
    assert.equal(calls[3].params.price,'5000.2');
    assert.deepEqual(ids,['9007199254740993']);
    routes.set('/dapi/v1/openOrders', '{"code":-2011,"msg":"\\ud800"}');
    let rejected;
    try { await venue.xhr.get('/dapi/v1/openOrders'); } catch (error) { rejected = error; }
    assert.equal(rejected.name, 'Error');
    assert.equal(rejected.message.charCodeAt(0), 0xd800);
    assert.equal(rejected.message.length, 1);
    assert.equal(rejected.message, rejected.response.data.msg);
    assert.equal(rejected.code, -2011);
    assert.equal(rejected.__binanceEmitted, true);
    assert.equal(calls.length, 5);
    process.stdout.write(JSON.stringify({markets,positions,ids,order:{quantity:calls[3].params.quantity,positionSide:calls[3].params.positionSide,price:calls[3].params.price}}));
  } finally { venue.dispose(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
