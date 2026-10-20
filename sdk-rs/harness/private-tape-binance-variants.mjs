// Binance documented payloads; synthetic accounts, no credentials/live orders.
// Spot: https://developers.binance.com/docs/binance-spot-api-docs/user-data-stream
// COIN-M: https://developers.binance.com/docs/derivatives/coin-margined-futures/user-data-streams
// PM: https://developers.binance.com/docs/derivatives/portfolio-margin/user-data-streams
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname } from 'node:path';
import { parseArgs } from 'node:util';
const { values } = parseArgs({ options: { venue: { type: 'string' }, out: { type: 'string' } } });
const name = values.venue;
if (!['binancespot', 'binanceinverse', 'binancepm'].includes(name)) throw new Error('unknown Binance variant');
const spot = name === 'binancespot';
const inverse = name === 'binanceinverse';
const REST = spot ? 'https://api.binance.com' : inverse ? 'https://dapi.binance.com' : 'https://papi.binance.com';
const PUBLIC_REST = name === 'binancepm' ? 'https://fapi.binance.com' : REST;
const prefix = spot ? '/api/v3' : inverse ? '/dapi/v1' : '/papi/v1/um';
const publicPrefix = spot ? '/api/v3' : inverse ? '/dapi/v1' : '/fapi/v1';
const listenKey = 'documented-replay-listen-key';
const PUBLIC = spot ? 'wss://stream.binance.com:9443/ws' : inverse ? 'wss://dstream.binance.com/ws' : 'wss://fstream.binance.com/market/ws';
const PRIVATE = spot ? 'wss://ws-api.binance.com:443/ws-api/v3' : inverse ? `${PUBLIC}/${listenKey}` : `wss://fstream.binance.com/pm/ws/${listenKey}`;
const epoch = 1760880000000;
const symbols = inverse ? ['BTCUSD_PERP', 'ETHUSD_PERP'] : ['BTCUSDT', 'ETHUSDT'];
const records = [{ t: 0, type: 'meta', epoch, private: true }];
const http = (method, path, query, body, extra = {}, origin = REST) => records.push({
  t: 0, type: 'http', method, url: `${origin}${path}${query ? `?${new URLSearchParams(query)}` : ''}`,
  status: 200, body: JSON.stringify(body), ...extra,
});
let clock = 0;
const ws = (url, event, extra = {}) => records.push({ t: (clock += 400), type: 'ws', url, text: JSON.stringify(event), ...extra });
const command = (op, args) => records.push({ t: (clock += 400), type: 'command', op, ...args });
const markets = symbols.map((symbol, i) => ({
  symbol, baseAsset: i ? 'ETH' : 'BTC', quoteAsset: inverse ? 'USD' : 'USDT', marginAsset: inverse ? (i ? 'ETH' : 'BTC') : 'USDT',
  ...(inverse ? { contractStatus: 'TRADING' } : { status: 'TRADING' }), contractType: 'PERPETUAL', contractSize: i ? 10 : 100,
  baseAssetPrecision: 8, quoteAssetPrecision: 8,
  filters: [
    { filterType: 'PRICE_FILTER', tickSize: i ? '0.01' : '0.10', minPrice: '0.10', maxPrice: '1000000' },
    { filterType: 'LOT_SIZE', stepSize: inverse ? '1' : '0.001', minQty: inverse ? '1' : '0.001', maxQty: inverse ? '10000' : '1000' },
    { filterType: 'MARKET_LOT_SIZE', stepSize: inverse ? '1' : '0.001', minQty: inverse ? '1' : '0.001', maxQty: inverse ? '10000' : '120' },
  ],
}));
http('GET', `${publicPrefix}/exchangeInfo`, null, { timezone: 'UTC', symbols: markets }, {}, PUBLIC_REST);
if (!spot) http('GET', inverse ? '/dapi/v2/leverageBracket' : `${prefix}/leverageBracket`, null,
  symbols.map((symbol) => ({ symbol, brackets: [{ initialLeverage: 100, qtyCap: 10000, notionalCap: 1000000 }] })));
http('GET', `${publicPrefix}/ticker/24hr`, null, symbols.map((symbol, i) => ({
  symbol, lastPrice: i ? '3000' : '96000', priceChangePercent: '1.2', volume: '1000', baseVolume: '100', quoteVolume: '100',
  bidPrice: i ? '2999.99' : '95999.9', askPrice: i ? '3000' : '96000',
})), {}, PUBLIC_REST);
if (!spot) {
  http('GET', `${publicPrefix}/ticker/bookTicker`, null, symbols.map((symbol, i) => ({ symbol, bidPrice: i ? '2999.99' : '95999.9', askPrice: i ? '3000' : '96000' })), {}, PUBLIC_REST);
  http('GET', `${publicPrefix}/premiumIndex`, null, symbols.map((symbol, i) => ({ symbol, markPrice: i ? '3000' : '96000', indexPrice: i ? '3000' : '96000', lastFundingRate: '0.0001', nextFundingTime: epoch + 28800000 })), {}, PUBLIC_REST);
  http('POST', inverse ? `${prefix}/listenKey` : '/papi/v1/listenKey', null, { listenKey });
  http('GET', `${prefix}/positionSide/dual`, null, { dualSidePosition: false });
  http('GET', `${prefix}/positionRisk`, null, [
    { symbol: symbols[0], positionSide: 'BOTH', positionAmt: inverse ? '7' : '0.01', entryPrice: '95000', markPrice: '96000',
      unRealizedProfit: inverse ? '0.00001' : '10', leverage: '20', marginType: 'cross', isolatedMargin: '0',
      liquidationPrice: '80000', notional: '960', isolatedWallet: '0', updateTime: epoch },
    { symbol: symbols[1], positionSide: 'BOTH', positionAmt: '0', entryPrice: '0', markPrice: '3000',
      unRealizedProfit: '0', leverage: '10', marginType: 'cross', isolatedMargin: '0', liquidationPrice: '0', notional: '0', updateTime: epoch },
  ]);
}
const spotBalances = [{asset:'USDT',free:'4000',locked:'10'}, {asset:'BTC',free:'0.01',locked:'0.001'}, {asset:'ETH',free:'1',locked:'0'}];
if (spot) http('GET', `${prefix}/account`, null, { makerCommission:10, takerCommission:10, balances:spotBalances });
else if (inverse) http('GET', `${prefix}/account`, null, {
  assets: [{asset:'BTC',walletBalance:'0.1',availableBalance:'0.09',initialMargin:'0.01',unrealizedProfit:'0.00001'},
    {asset:'ETH',walletBalance:'2',availableBalance:'1.8',initialMargin:'0.2',unrealizedProfit:'0'}], positions:[],
});
else http('GET','/papi/v1/balance',null,[
  {asset:'USDT',totalWalletBalance:'4000',crossMarginFree:'3900',umUnrealizedPNL:'10',cmUnrealizedPNL:'0'},
  {asset:'USDC',totalWalletBalance:'1000',crossMarginFree:'950',umUnrealizedPNL:'0',cmUnrealizedPNL:'0'},
]);
const order = (extra = {}) => ({ symbol:symbols[0],orderId:1001,clientOrderId:'resting',price:'90000',origQty:inverse?'7':'0.01',executedQty:inverse?'2':'0.002',
  status:'PARTIALLY_FILLED',timeInForce:'GTC',type:'LIMIT',side:'BUY',positionSide:'BOTH',stopPrice:'0',reduceOnly:false,closePosition:false,workingType:'CONTRACT_PRICE',...extra });
http('GET', `${prefix}/openOrders`, null, [order()]);
if (!spot && !inverse) http('GET', `${prefix}/conditional/openOrders`, null, []);
if (spot) {
  const request = (id,method) => records.push({t:0,type:'ws-send',url:PRIVATE,text:JSON.stringify({id,method})});
  request('subscribe','userDataStream.subscribe.signature');
  ws(PRIVATE,{id:'subscribe',status:200,result:{subscriptionId:7}});
  request('heartbeat','session.subscriptions');
  ws(PRIVATE,{id:'heartbeat',status:200,result:[{subscriptionId:7}]});
}
ws(PUBLIC,spot ? {e:'24hrTicker',s:symbols[1],c:'3000',P:'1.2',v:'1000',q:'100',b:'2999.99',a:'3000'} :
  [{e:'24hrTicker',s:symbols[1],c:'3000',P:'1.2',v:'1000',q:'100'}], spot ? {socket:1} : {});
const streamOrder = (id, client, status, quantity, filled='0', execution='NEW') => spot ?
  {subscriptionId:7,event:{e:'executionReport',E:epoch+clock,s:symbols[1],c:client,S:'BUY',o:'LIMIT',f:'GTC',q:String(quantity),p:'2800',P:'0',F:'0',g:-1,C:'',x:execution,X:status,r:'NONE',i:id,l:execution==='TRADE'?String(filled):'0',z:String(filled),L:'2799',n:'0',N:null,T:epoch+clock,t:execution==='TRADE'?101:-1,I:1,w:true,m:false,M:false,Z:'0',Y:'0',Q:'0'}} :
  {e:'ORDER_TRADE_UPDATE',E:epoch+clock,T:epoch+clock,fs:'UM',o:{s:symbols[1],c:client,S:'BUY',o:'LIMIT',ot:'LIMIT',f:'GTC',q:String(quantity),p:'2800',ap:'2799',sp:'0',x:execution,X:status,i:id,l:execution==='TRADE'?String(filled):'0',z:String(filled),L:'2799',n:'0',N:inverse?'ETH':'USDT',T:epoch+clock,t:101,R:false,wt:'CONTRACT_PRICE',ps:'BOTH',cp:false,rp:'0'}};
ws(PRIVATE,streamOrder(2001,'event-order','NEW',inverse?9:0.5));
ws(PRIVATE,streamOrder(2001,'event-order','FILLED',inverse?9:0.5,inverse?9:0.5,'TRADE'));
const caller = spot ? 'caller-spot-1' : 'caller-pm-1';
command('placeOrders',{orders:[{symbol:symbols[0],type:'limit',side:'buy',amount:inverse?7:0.002,price:90500,...(!inverse?{clientOrderId:caller}:{})}]});
http('POST',`${prefix}/order`,{
  symbol:symbols[0],...(!spot?{positionSide:'BOTH'}:{}),side:'BUY',type:'LIMIT',quantity:inverse?'7':'0.002',price:'90500',timeInForce:'GTC',
  ...(inverse?{workingType:'MARK_PRICE'}:{newClientOrderId:caller}),
},{orderId:3001,clientOrderId:inverse?'$newClientOrderId':caller,symbol:symbols[0],status:'NEW',origQty:inverse?'7':'0.002',price:'90500'});
// Public cancellation uses the existing native order identity, not a generated placement ID.
command('cancelOrders',{ids:[inverse?'1001':'resting']});
http('DELETE',`${prefix}/order`,{symbol:symbols[0],...(inverse?{orderId:'1001'}:{origClientOrderId:'resting'})},order({status:'CANCELED',...(spot?{origClientOrderId:'resting',clientOrderId:'cancel-resting'}:{})}));
command('cancelSymbolOrders',{symbol:symbols[1]});
http('DELETE',spot?`${prefix}/openOrders`:`${prefix}/allOpenOrders`,{symbol:symbols[1]},spot?[]:{code:200,msg:'success'});
if (!spot && !inverse) http('DELETE',`${prefix}/conditional/allOpenOrders`,{symbol:symbols[1]},{code:200,msg:'success'},{impl:'rust'});
const out=values.out??`sdk-rs/harness/tapes/${name}-private.jsonl`;
mkdirSync(dirname(out),{recursive:true});records.sort((a,b)=>a.t-b.t);
writeFileSync(out,records.map((record)=>JSON.stringify(record)).join('\n')+'\n');
console.log(JSON.stringify({out,records:records.length}));
