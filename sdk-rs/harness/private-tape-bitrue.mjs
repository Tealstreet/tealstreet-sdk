// Futures v2 documented payloads: https://www.bitrue.com/api_docs_includes_file/futures/index.html
// Deterministic replay credentials; no live account is contacted.
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname } from 'node:path';
import { parseArgs } from 'node:util';
const { values } = parseArgs({ options: { out: { type: 'string', default: 'sdk-rs/harness/tapes/bitrue-private.jsonl' } } });
const REST = 'https://fapi.bitrue.com';
const PUBLIC = 'wss://fmarket-ws.bitrue.com/kline-api/ws';
const PRIVATE = 'wss://fapiws.bitrue.com/stream?listenKey=replay-listen-key';
const records = [{ t: 0, type: 'meta', epoch: 1700000000000, private: true }];
let time = 0;
const ok = (data) => ({ code: '0', msg: 'Success', data });
const http = (method, path, data, requestBody, afterCommand) => records.push({
  t: 0, type: 'http', method, url: path.startsWith('https://') ? path : REST + path,
  status: 200, body: JSON.stringify(data),
  ...(requestBody === undefined ? {} : { requestBody }),
  ...(afterCommand === undefined ? {} : { afterCommand }),
});
const ws = (url, event) => records.push({ t: (time += 100), type: 'ws', url, text: JSON.stringify(event).replace(/"i":"(2334151155877155281)"/, '"i":$1') });
const command = (body) => records.push({ t: (time += 100), type: 'command', ...body });
const contract = (symbol, multiplier) => ({ symbol, pricePrecision: 2, side: 1, maxMarketVolume: 1000, multiplier,
  minOrderVolume: 1, maxMarketMoney: 10000000, type: 'E', maxLimitVolume: 2000, maxValidOrder: 50,
  multiplierCoin: symbol.split('-')[1], minOrderMoney: 1, maxLimitMoney: 10000000, status: 1 });
http('GET', '/fapi/v1/contracts', [contract('E-BTC-USDT', 0.001), contract('E-ETH-USDT', 0.01)]);
http('POST', 'https://fapiws-auth.bitrue.com/user_stream/api/v1/listenKey', { code: 200, data: { listenKey: 'replay-listen-key' } });
const position = { side: 'BUY', volume: 0.2, openPrice: 60000, avgPrice: 60000, leverageLevel: 20,
  positionType: 1, positionBalance: 12000, forceLiquidationPrice: 30000 };
http('GET', '/fapi/v2/account', ok({ account: [{ marginCoin: 'USDT', totalEquity: 1010, accountNormal: 1000,
  accountLock: 5, sumMaintenanceMargin: 4, totalPositionNormal: 10, totalCost: 1, sumOpenRealizedAmount: 0,
  positionVos: [{ contractName: 'E-BTC-USDT', positions: [position] }] }, {
    marginCoin: 'USDC', totalEquity: 40, accountNormal: 35, accountLock: 2,
    sumMaintenanceMargin: 1, totalPositionNormal: 3, totalCost: 0,
    sumOpenRealizedAmount: -0.5, positionVos: [],
  }] }));
const normal = (id, patch = {}) => ({ orderId: id, clientOrderId: '', price: 59000, origQty: 0.004,
  executedQty: 0, avgPrice: 0, status: 'NEW', type: 'LIMIT', side: 'BUY', action: 'OPEN',
  transactTime: 1700000000000, ...patch });
http('GET', '/fapi/v2/openOrders?contractName=E-BTC-USDT', ok([normal('100')]));
http('GET', '/fapi/v2/openOrders?contractName=E-ETH-USDT', ok([]));
ws(PRIVATE, { event: 'user_account_update', status: 'ok' });
ws(PUBLIC, { ping: 1700000000 });
ws(PRIVATE, { event: 'ping', ts: '1700000000000' });
for (const symbol of ['btc', 'eth']) ws(PUBLIC, { channel: `market_e_${symbol}usdt_ticker`, tick: {
  close: symbol === 'btc' ? '60000' : '3000', rose: '0.02', vol: '1000', amount: '60000000',
} });
const orderEvent = (id, total, price, status = 'PART_FILLED', patch = {}) => ws(PRIVATE, {
  E: 1700000000000 + time + 100, e: 'ORDER_TRADE_UPDATE', o: {
    s: 'E_BTCUSDT', i: id, c: '', S: 'BUY', o: 'LIMIT', x: 'TRADE', X: status,
    q: '0.005', z: String(total), l: '1', p: '90', ap: String(price), L: String(price), R: false, ...patch,
  },
});
const fillId = '2334151155877155281';
orderEvent(fillId, 1, 100);
orderEvent(fillId, 3, 130);
orderEvent(fillId, 3, 130);
orderEvent(fillId, 2, 120);
orderEvent(fillId, 5, 170, 'FILLED');
orderEvent(fillId, 5, 170, 'FILLED');
const payload = (patch = {}) => ({ contractName: 'E-BTC-USDT', clientOrderId: 'generated-parent', leverage: 20, price: 59000, volume: '0.004',
  type: 'LIMIT', side: 'BUY', open: 'OPEN', positionType: 1, futureModel: 'normal', ...patch });
command({ op: 'placeOrders', orders: [{ symbol: 'E-BTC-USDT', type: 'limit', side: 'buy', amount: 0.004, price: 59000, clientOrderId: 'caller-bitrue-1' }] });
// A confirmed signature rejection permits exactly one newly signed resend.
http('POST', '/fapi/v2/order', { code: '-1022', msg: 'Signature for this request is not valid.' }, payload({ clientOrderId: 'caller-bitrue-1' }));
http('POST', '/fapi/v2/order', ok({ orderId: '300' }), payload({ clientOrderId: 'caller-bitrue-1' }));
http('GET', '/fapi/v2/openOrders?contractName=E-BTC-USDT', ok([normal('100'), normal('300', { clientOrderId: 'caller-bitrue-1' })]), undefined, 1);
orderEvent('300', 0, 59000, 'NEW', { q: '0.004', p: '59000', c: 'caller-bitrue-1', x: 'NEW' });
command({ op: 'findOrderByClientOrderId', symbol: 'E-BTC-USDT', clientOrderId: 'caller-bitrue-1' });
command({ op: 'cancelOrders', ids: ['300'] });
http('POST', '/fapi/v2/cancel', ok({ orderId: '300' }), { orderId: '300', contractName: 'E-BTC-USDT', conditionOrder: false });
http('GET', '/fapi/v2/openOrders?contractName=E-BTC-USDT', ok([normal('100')]), undefined, 3);
orderEvent('300', 0, 59000, 'CANCELED', { q: '0.004', p: '59000', c: 'caller-bitrue-1', x: 'CANCEL' });
command({ op: 'placeOrders', orders: [{ symbol: 'E-BTC-USDT', type: 'limit', side: 'buy', amount: 0.004, price: 59000, stopLoss: 55000, takeProfit: 65000 }] });
http('POST', '/fapi/v2/order', ok({ orderId: '301' }), payload({ triggerOrderCreateParams: [
  { clientOrderId: 'generated-child-sl', triggerType: 1, triggerPriceType: 2, triggerPrice: 55000, type: 'MARKET', price: 0 },
  { clientOrderId: 'generated-child-tp', triggerType: 2, triggerPriceType: 1, triggerPrice: 65000, type: 'MARKET', price: 0 },
] }));
// New order snapshots become authoritative only after their placement command.
http('GET', '/fapi/v2/openOrders?contractName=E-BTC-USDT', ok([normal('100'), normal('301')]), undefined, 4);
command({ op: 'placeOrders', orders: [{ symbol: 'E-BTC-USDT', type: 'stop_market', side: 'sell', amount: 0.004, price: 55000, reduceOnly: true }] });
http('POST', '/fapi/v2/order', ok({ orderId: 'unreliable-stop-id' }), payload({ price: 0, triggerPrice: 55000,
  volume: '0.004', type: 'MARKET', side: 'SELL', open: 'CLOSE', conditionOrder: true,
  triggerType: 1, triggerPriceType: 2 }));
const stop = { orderId: '700', clientOrderId: 'stop-server-id', price: 0, origAmount: 240, executedQty: 0.004,
  status: 'INIT', side: 'SELL', action: 'CLOSE', transactTime: 1700000000000, triggerPrice: 55000, triggerType: 1, conditionOrder: true };
http('GET', '/fapi/v2/openOrders?contractName=E-BTC-USDT', ok([normal('100'), normal('301'), stop]), undefined, 5);
command({ op: 'fetchFills', symbol: 'E-BTC-USDT', from: 1700000000000, to: 1700000100000, limit: 10 });
http('GET', '/fapi/v2/myTrades?contractName=E-BTC-USDT&limit=10&startTime=1700000000000&endTime=1700000100000', ok([
  { tradeId: '9001', bidId: fillId, askId: '400', side: 'BUY', price: 60000, qty: 3, amount: 180,
    contractName: 'E-BTC-USDT', fee: '-0.1', isMaker: true, ctime: 1700000050000 },
  { tradeId: '9000', bidId: fillId, askId: '400', side: 'BUY', price: 60000, qty: 3, amount: 180,
    contractName: 'E-BTC-USDT', fee: '-0.1', isMaker: true, ctime: 1699999999000 },
]));
// The inherited TS lookup only sees open orders; absence is never definitive.
command({ op: 'findOrderByClientOrderId', symbol: 'E-BTC-USDT', clientOrderId: 'missing-caller' });
command({ op: 'findOrderByClientOrderId', symbol: 'E-ETH-USDT', clientOrderId: 'missing-caller' });
// Flat-symbol maximum leverage is an authenticated bracket read, independent
// of current position leverage. Both implementations must expose the native cap.
command({ op: 'fetchMaxLeverage', symbol: 'E-ETH-USDT' });
http('GET', '/fapi/v2/leverageBracket?contractName=E-ETH-USDT', ok({
  contractName: 'E-ETH-USDT', brackets: [
    { bracket: 1, initialLeverage: 125, qtyCap: 50000, qtyFloor: 0, maintMarginRatio: 0.004, cum: 0 },
    { bracket: 2, initialLeverage: 50, qtyCap: 250000, qtyFloor: 50000, maintMarginRatio: 0.005, cum: 50 },
  ],
}));
ws(PRIVATE, { event: 'ACCOUNT_UPDATE' });
ws(PRIVATE, { event: 'ping', ts: '1700000005000' });
mkdirSync(dirname(values.out), { recursive: true });
records.sort((a, b) => a.t - b.t);
writeFileSync(values.out, records.map(JSON.stringify).join('\n') + '\n');
console.log(JSON.stringify({ out: values.out, records: records.length }));
