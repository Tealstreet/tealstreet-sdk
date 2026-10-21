// BloFin futures examples: https://docs.blofin.com/index.html .
// Deterministic credentials are replay-only. No live account is contacted.
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname } from 'node:path';
import { parseArgs } from 'node:util';
const { values } = parseArgs({
  options: { out: { type: 'string', default: 'sdk-rs/harness/tapes/blofin-private.jsonl' } },
});
const REST = 'https://openapi.blofin.com';
const PUBLIC = 'wss://openapi.blofin.com/ws/public';
const PRIVATE = 'wss://openapi.blofin.com/ws/private';
// Anchor signing and TS fallback fill timestamps to the authored history window.
const records = [{ t: 0, type: 'meta', private: true, epoch: 1700000000000 }];
const ok = (data) => ({ code: '0', msg: 'success', data });
const http = (method, path, query, data, requestBody) =>
  records.push({
    t: 0,
    type: 'http',
    method,
    url: `${REST}${path}${query ? `?${query}` : ''}`,
    status: 200,
    body: JSON.stringify(data),
    ...(requestBody === undefined ? {} : { requestBody }),
  });
let time = 0;
const ws = (url, event) => records.push({ t: (time += 100), type: 'ws', url, text: JSON.stringify(event) });
const command = (body) => records.push({ t: (time += 100), type: 'command', ...body });
const instrument = (base, contractValue, lot, tick) => ({
  instId: `${base}-USDT`,
  baseCurrency: base,
  quoteCurrency: 'USDT',
  instType: 'SWAP',
  contractType: 'linear',
  contractValue,
  minSize: lot,
  lotSize: lot,
  tickSize: tick,
  maxLimitSize: '1000',
  maxMarketSize: '500',
  maxLeverage: '100',
  state: 'live',
});
http(
  'GET',
  '/api/v1/market/instruments',
  '',
  ok([instrument('BTC', '0.001', '0.1', '0.1'), instrument('ETH', '0.01', '0.1', '0.01')]),
);
http(
  'GET',
  '/api/v1/market/tickers',
  '',
  ok([
    {
      instId: 'BTC-USDT',
      last: '60000',
      open24h: '59000',
      bidPrice: '59999.9',
      askPrice: '60000.1',
      volCurrency24h: '1000',
      vol24h: '60000000',
    },
    {
      instId: 'ETH-USDT',
      last: '3000',
      open24h: '2900',
      bidPrice: '2999.99',
      askPrice: '3000.01',
      volCurrency24h: '100',
      vol24h: '300000',
    },
  ]),
);
http('GET', '/api/v1/account/margin-mode', '', ok({ marginMode: 'cross' }));
http('GET', '/api/v1/account/position-mode', '', ok({ positionMode: 'net_mode' }));
const position = {
  positionId: '395624',
  instId: 'BTC-USDT',
  instType: 'SWAP',
  positionSide: 'net',
  marginMode: 'cross',
  positions: '4',
  averagePrice: '59965.8',
  markPrice: '60000',
  maintenanceMargin: '0.72',
  unrealizedPnl: '0.5',
  leverage: '20',
  liquidationPrice: '26914.9',
};
http('GET', '/api/v1/account/positions', '', ok([position]));
http(
  'GET',
  '/api/v1/asset/balances',
  'accountType=futures',
  ok([{ currency: 'USDT', available: '1000', frozen: '10' }]),
);
const normal = (id, patch = {}) => ({
  instId: 'BTC-USDT',
  instType: 'SWAP',
  orderCategory: 'normal',
  orderId: id,
  clientOrderId: `web${id}`,
  marginMode: 'cross',
  positionSide: 'net',
  orderType: 'limit',
  state: 'live',
  side: 'buy',
  price: '59000',
  size: '2',
  filledSize: '0',
  averagePrice: '0',
  ...patch,
});
const tpsl = {
  instId: 'BTC-USDT',
  instType: 'SWAP',
  tpslId: '900',
  marginMode: 'cross',
  positionSide: 'net',
  side: 'sell',
  size: '0',
  state: 'live',
  tpTriggerPrice: '65000',
  slTriggerPrice: '55000',
};
http('GET', '/api/v1/trade/orders-pending', 'limit=100', ok([normal('100')]));
http('GET', '/api/v1/trade/orders-pending', 'limit=100&after=100', ok([]));
http('GET', '/api/v1/trade/orders-tpsl-pending', 'limit=100', ok([tpsl]));
http('GET', '/api/v1/trade/orders-tpsl-pending', 'limit=100&after=900', ok([]));
http('GET', '/api/v1/trade/orders-algo-pending', 'limit=100&orderType=trigger', ok([]));
ws(PRIVATE, { event: 'login', code: '0', msg: '' });
ws(PUBLIC, {
  arg: { channel: 'tickers', instId: 'ETH-USDT' },
  data: [
    {
      instId: 'ETH-USDT',
      last: '3001',
      open24h: '2900',
      bidPrice: '3000.99',
      askPrice: '3001.01',
      volCurrency24h: '101',
      vol24h: '303101',
    },
  ],
});
const orderEvent = (row) => ws(PRIVATE, { arg: { channel: 'orders' }, data: [row] });
// The documented live/unfilled push proves order200 has no historical fills.
// Keep every existing event time: the first execution remains at t300.
records.push({
  t: 250,
  type: 'ws',
  url: PRIVATE,
  text: JSON.stringify({ arg: { channel: 'orders' }, data: [normal('200')] }),
});
orderEvent(normal('200', { filledSize: '0.1', averagePrice: '100', state: 'partially_filled' }));
orderEvent(normal('200', { filledSize: '0.3', averagePrice: '120', state: 'partially_filled' }));
orderEvent(normal('200', { filledSize: '0.3', averagePrice: '120', state: 'partially_filled' }));
orderEvent(normal('200', { filledSize: '0.2', averagePrice: '110', state: 'partially_filled' }));
orderEvent(normal('200', { filledSize: '0.5', averagePrice: '140', state: 'filled' }));
orderEvent(normal('200', { filledSize: '0.5', averagePrice: '140', state: 'filled' }));
command({
  op: 'placeOrders',
  orders: [
    {
      symbol: 'BTCUSDT',
      type: 'limit',
      side: 'buy',
      amount: 1.23,
      price: 60000.04,
      clientOrderId: 'b3cdedc0a20880baCaller123',
    },
  ],
});
http('POST', '/api/v1/trade/batch-orders', '', ok([{ orderId: '300', code: '0', msg: 'Order placed' }]), [
  {
    instId: 'BTC-USDT',
    marginMode: 'cross',
    positionSide: 'net',
    side: 'buy',
    orderType: 'limit',
    size: '1.2',
    price: '60000',
    clientOrderId: 'b3cdedc0a20880baCaller123',
  },
]);
orderEvent(normal('300', { price: '60000', size: '1.2', clientOrderId: 'b3cdedc0a20880baCaller123' }));
command({ op: 'cancelOrders', ids: ['300'] });
http('POST', '/api/v1/trade/cancel-batch-orders', '', ok([{ orderId: '300', code: '0', msg: 'success' }]), [
  { instId: 'BTC-USDT', orderId: '300' },
]);
orderEvent(
  normal('300', { price: '60000', size: '1.2', clientOrderId: 'b3cdedc0a20880baCaller123', state: 'canceled' }),
);
command({ op: 'setLeverage', symbol: 'BTCUSDT', leverage: 30 });
http(
  'POST',
  '/api/v1/account/set-leverage',
  '',
  ok({ leverage: '30', marginMode: 'cross', instId: 'BTC-USDT', positionSide: 'net' }),
  { instId: 'BTC-USDT', leverage: '30', marginMode: 'cross' },
);
ws(PRIVATE, {
  arg: { channel: 'positions' },
  data: [{ ...position, leverage: '30', maintenanceMargin: '0.8', unrealizedPnl: '2' }],
});
ws(PRIVATE, { arg: { channel: 'account' }, data: { totalEquity: '1012' } });

command({
  op: 'placeOrders',
  orders: [
    { symbol: 'BTCUSDT', type: 'limit', side: 'buy', amount: 2, price: 59000, takeProfit: 65000, stopLoss: 55000 },
  ],
});
http('POST', '/api/v1/trade/batch-orders', '', ok([{ orderId: '301', code: '0', msg: 'success' }]), [
  {
    instId: 'BTC-USDT',
    marginMode: 'cross',
    positionSide: 'net',
    side: 'buy',
    orderType: 'limit',
    size: '2',
    price: '59000',
    tpTriggerPrice: '65000',
    tpOrderPrice: '-1',
    slTriggerPrice: '55000',
    slOrderPrice: '-1',
    attachAlgoOrders: [{ tpTriggerPrice: '65000', tpOrderPrice: '-1', slTriggerPrice: '55000', slOrderPrice: '-1' }],
  },
]);
orderEvent(normal('301', { tpTriggerPrice: '65000', slTriggerPrice: '55000' }));
command({
  op: 'placeOrders',
  orders: [{ symbol: 'BTCUSDT', type: 'stop_market', side: 'sell', amount: 0.2, price: 55000 }],
});
http('POST', '/api/v1/trade/order-tpsl', '', ok({ tpslId: '700', code: '0', msg: null }), {
  instId: 'BTC-USDT',
  marginMode: 'cross',
  positionSide: 'net',
  side: 'sell',
  size: '0.2',
  reduceOnly: 'true',
  slTriggerPrice: '55000',
  slOrderPrice: '-1',
});
ws(PRIVATE, {
  arg: { channel: 'orders-algo' },
  data: [
    {
      instId: 'BTC-USDT',
      instType: 'SWAP',
      tpslId: '700',
      side: 'sell',
      positionSide: 'net',
      marginMode: 'cross',
      size: '0.2',
      slTriggerPrice: '55000',
      // Documented inactive sibling fields may be null in native TPSL rows.
      tpTriggerPrice: null,
      tpOrderPrice: null,
      tpTriggerPriceType: null,
      state: 'live',
    },
  ],
});
command({
  op: 'placeOrders',
  orders: [{ symbol: 'BTCUSDT', type: 'conditional', side: 'buy', amount: 1, triggerPrice: 61000 }],
});
http('POST', '/api/v1/trade/order-algo', '', ok({ algoId: '800', code: '0', msg: null }), {
  instId: 'BTC-USDT',
  marginMode: 'cross',
  positionSide: 'net',
  side: 'buy',
  size: '1',
  orderPrice: '-1',
  orderType: 'trigger',
  triggerPrice: '61000',
  triggerPriceType: 'last',
});
const conditional = {
  instId: 'BTC-USDT',
  instType: 'SWAP',
  algoId: '800',
  side: 'buy',
  positionSide: 'net',
  marginMode: 'cross',
  size: '1',
  orderType: 'trigger',
  orderPrice: '-1',
  triggerPrice: '61000',
  triggerPriceType: 'last',
  state: 'live',
};
ws(PRIVATE, { arg: { channel: 'orders-algo' }, data: [conditional] });
// Cancel-all covers normal and native combined TP/SL, without sibling recreation.
command({ op: 'cancelSymbolOrders', symbol: 'BTCUSDT' });
http(
  'POST',
  '/api/v1/trade/cancel-batch-orders',
  '',
  ok([
    { orderId: '100', code: '0', msg: 'success' },
    { orderId: '301', code: '0', msg: 'success' },
  ]),
  [
    { instId: 'BTC-USDT', orderId: '100' },
    { instId: 'BTC-USDT', orderId: '301' },
  ],
);
http(
  'POST',
  '/api/v1/trade/cancel-tpsl',
  '',
  ok([
    { tpslId: '900', code: '0', msg: 'success' },
    { tpslId: '700', code: '0', msg: 'success' },
  ]),
  [
    { instId: 'BTC-USDT', tpslId: '900' },
    { instId: 'BTC-USDT', tpslId: '700' },
  ],
);
http('POST', '/api/v1/trade/cancel-algo', '', ok({ algoId: '800', code: '0', msg: null }), {
  instId: 'BTC-USDT',
  algoId: '800',
});
orderEvent(normal('100', { state: 'canceled' }));
ws(PRIVATE, { arg: { channel: 'orders-algo' }, data: [{ ...tpsl, state: 'canceled' }] });
orderEvent(normal('301', { tpTriggerPrice: '65000', slTriggerPrice: '55000', state: 'canceled' }));
ws(PRIVATE, {
  arg: { channel: 'orders-algo' },
  data: [
    {
      instId: 'BTC-USDT',
      tpslId: '700',
      side: 'sell',
      positionSide: 'net',
      marginMode: 'cross',
      size: '0.2',
      slTriggerPrice: '55000',
      state: 'canceled',
    },
    { ...conditional, state: 'canceled' },
  ],
});
// Read adapters must preserve windows rather than relying on final account state.
const execution = {
  instId: 'BTC-USDT',
  orderId: '200',
  tradeId: '9001',
  side: 'buy',
  positionSide: 'net',
  fillPrice: '60000',
  fillSize: '2',
  fee: '0.5',
  feeCurrency: 'USDT',
  fillPnl: '0',
  ts: '1700000050000',
};
command({ op: 'fetchFills', symbol: 'BTCUSDT', from: 1700000000000, to: 1700000100000, limit: 1 });
http(
  'GET',
  '/api/v1/trade/fills-history',
  'instId=BTC-USDT&begin=1700000000000&end=1700000100000&limit=1',
  ok([execution]),
);
command({ op: 'fetchIncome', symbol: 'BTCUSDT', from: 1700000000000, to: 1700000100000 });
http(
  'GET',
  '/api/v1/trade/fills-history',
  'instId=BTC-USDT&begin=1700000000000&end=1700000100000&limit=100',
  ok([execution, { ...execution, tradeId: '9000', ts: '1699999999000', fee: '99' }]),
);
command({ op: 'fetchLeverage', symbol: 'BTCUSDT' });
http(
  'GET',
  '/api/v1/account/batch-leverage-info',
  'instId=BTC-USDT&marginMode=cross',
  ok([{ instId: 'BTC-USDT', marginMode: 'cross', positionSide: 'net', leverage: '30' }]),
);
mkdirSync(dirname(values.out), { recursive: true });
records.sort((a, b) => a.t - b.t);
writeFileSync(values.out, `${records.map((r) => JSON.stringify(r)).join('\n')}\n`);
console.log(JSON.stringify({ out: values.out, records: records.length }));
