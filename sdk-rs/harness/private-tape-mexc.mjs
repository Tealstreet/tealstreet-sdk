// Documented MEXC futures payloads, with synthetic account data and no secrets.
// Includes separate sockets on /edge, exact mutation bodies and native ids >2^53.
//   corepack yarn rust:private-tape-mexc
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname } from 'node:path';
import { parseArgs } from 'node:util';

const { values } = parseArgs({
  options: { out: { type: 'string', default: 'sdk-rs/harness/tapes/mexc-private.jsonl' } },
});
const REST = 'https://api.mexc.com';
const SOCKET = 'wss://contract.mexc.com/edge';
const EPOCH = 1792454400000;
const ids = {
  long: '739113577038255616',
  short: '739113577038255617',
  resting: '739113577038255618',
  placed: '739113577038255619',
  plan: '739113577038255620',
  stop: '739113577038255621',
};
const records = [{ t: 0, type: 'meta', private: true, epoch: EPOCH }];
const payload = (value) => JSON.stringify(value).replace(/"@(\d+)"/g, '$1');
const envelope = (data) => ({ success: true, code: 0, data });
const http = (method, path, query, response, requestBody, stage = {}) =>
  records.push({
    t: 0,
    type: 'http',
    method,
    url: `${REST}${path}${query ? `?${new URLSearchParams(query)}` : ''}`,
    status: 200,
    body: payload(response),
    ...(requestBody === undefined ? {} : { requestBody: JSON.stringify(requestBody) }),
    ...stage,
  });
const ws = (t, socket, channel, data) =>
  records.push({ t, type: 'ws', url: SOCKET, socket, text: payload({ channel, symbol: data?.symbol, data }) });
const command = (t, op, args) => records.push({ t, type: 'command', op, ...args });
const market = (symbol, baseCoin, contractSize, priceUnit) => ({
  symbol,
  baseCoin,
  quoteCoin: 'USDT',
  settleCoin: 'USDT',
  state: 0,
  apiAllowed: true,
  isHidden: false,
  contractSize,
  priceUnit,
  volUnit: 1,
  minVol: 1,
  maxVol: 400000,
  minLeverage: 1,
  maxLeverage: 125,
  makerFeeRate: 0,
  takerFeeRate: 0.0002,
});
const ticker = (symbol, price) => ({
  symbol,
  lastPrice: price,
  fairPrice: price,
  indexPrice: price,
  bid1: price - 0.1,
  ask1: price,
  riseFallRate: 0.01,
  holdVol: 10000,
  fundingRate: 0.0001,
  fundingTime: EPOCH + 28800000,
  volume24: 50000,
  amount24: 300000,
});
const position = (positionId, positionType, leverage = 10, openType = 2) => ({
  positionId: `@${positionId}`,
  symbol: 'BTC_USDT',
  positionType,
  positionMode: 1,
  holdVol: positionType === 1 ? 60 : 20,
  holdAvgPrice: 60000,
  leverage,
  openType,
  unrealized: positionType === 1 ? 6 : -2,
  realised: 0,
  liquidatePrice: 50000,
  im: positionType === 1 ? 36 : 8,
  updateTime: EPOCH,
});
const regular = (id, vol, price, dealVol = 0, state = 2) => ({
  orderId: `@${id}`,
  symbol: 'BTC_USDT',
  side: 1,
  type: 1,
  vol,
  dealVol,
  price,
  state,
  updateTime: EPOCH + 300,
});
const long = position(ids.long, 1, 10);
const short = position(ids.short, 2, 15);
const resting = regular(ids.resting, 30, 58000, 10);
const plan = {
  id: `@${ids.plan}`,
  symbol: 'BTC_USDT',
  side: 4,
  triggerPrice: 55000,
  triggerType: 2,
  vol: 10,
  state: 1,
  orderType: 5,
  reduceOnly: true,
  updateTime: EPOCH + 1300,
};
const stop = {
  id: `@${ids.stop}`,
  symbol: 'BTC_USDT',
  positionType: 1,
  positionId: `@${ids.long}`,
  triggerSide: 1,
  takeProfitPrice: 65000,
  stopLossPrice: 0,
  vol: 60,
  volType: 2,
  state: 1,
  updateTime: EPOCH,
};

http(
  'GET',
  '/api/v1/contract/detail',
  null,
  envelope([market('BTC_USDT', 'BTC', 0.0001, 0.1), market('ETH_USDT', 'ETH', 0.01, 0.01)]),
);
http('GET', '/api/v1/contract/ticker', null, envelope([ticker('BTC_USDT', 60000), ticker('ETH_USDT', 3000)]));
http('GET', '/api/v1/private/position/position_mode', null, envelope(1));
http(
  'GET',
  '/api/v1/private/account/assets',
  null,
  envelope([
    { currency: 'USDT', availableBalance: 956, equity: 1000, cashBalance: 996, unrealized: 4, frozenBalance: 0 },
  ]),
);
http('GET', '/api/v1/private/position/open_positions', null, envelope([long, short]));
http('GET', '/api/v1/private/order/list/open_orders', { page_num: 1, page_size: 100 }, envelope([resting]));
// Full-order refreshes may be account-wide or scoped after a stop mutation.
for (const symbol of [undefined, 'BTC_USDT']) {
  const window = {
    start_time: EPOCH - 89 * 86400000,
    end_time: EPOCH,
    page_num: 1,
    page_size: 100,
    ...(symbol ? { symbol } : {}),
  };
  http('GET', '/api/v1/private/planorder/list/orders', { ...window, states: '1' }, envelope([]));
  http('GET', '/api/v1/private/stoporder/list/orders', { ...window, is_finished: 0 }, envelope([stop]));
}
http('GET', '/api/v1/private/order/list/open_orders/BTC_USDT', { page_num: 1, page_size: 100 }, envelope([resting]));
http(
  'GET',
  '/api/v1/private/position/leverage',
  { symbol: 'BTC_USDT' },
  envelope([
    { positionType: 1, leverage: 10, openType: 2, maxLeverageView: 100, maxLeverage: 125 },
    { positionType: 2, leverage: 15, openType: 2, maxLeverageView: 100, maxLeverage: 125 },
  ]),
);
http(
  'GET',
  '/api/v1/private/position/leverage',
  { symbol: 'ETH_USDT' },
  envelope([
    { positionType: 1, leverage: 6, openType: 2, maxLeverageView: 75, maxLeverage: 125 },
    { positionType: 2, leverage: 9, openType: 2, maxLeverageView: 75, maxLeverage: 125 },
  ]),
);

ws(100, 0, 'push.ticker', ticker('BTC_USDT', 60000));
ws(200, 1, 'rs.login', 'success');
ws(300, 1, 'push.personal.position', [long, short]);
ws(400, 1, 'push.personal.order', resting);
ws(500, 1, 'push.personal.order.deal', {
  tradeId: '@987654321123456789',
  orderId: `@${ids.resting}`,
  symbol: 'BTC_USDT',
  side: 1,
  price: 58000,
  vol: 10,
  fee: 0.001,
  createTime: EPOCH + 500,
});
command(550, 'placeOrders', { orders: [
  { symbol: 'BTCUSDT', type: 'limit', side: 'buy', amount: 20, price: 59000 },
  { symbol: 'BTCUSDT', type: 'limit', side: 'buy', amount: 20, price: 59000, clientOrderId: 'caller-must-be-rejected' },
] });
command(600, 'fetchLeverage', { symbol: 'BTCUSDT' });
command(700, 'placeOrders', { orders: [{ symbol: 'BTCUSDT', type: 'limit', side: 'buy', amount: 20, price: 59000 }] });
// The TS batch API attempts the market-maker route even for a singleton.
// Rust uses the paced single-submit path directly; TS's rejected attempt is
// independently required and checked, while the accepted submit is shared.
http(
  'POST',
  '/api/v1/private/order/submit_batch',
  null,
  { success: false, code: 511, message: 'You do not have access to the interface' },
  [
    {
      symbol: 'BTC_USDT',
      price: '59000',
      vol: 20,
      side: 1,
      type: 1,
      openType: 2,
      leverage: 10,
      externalOid: 'generated',
      stpMode: 0,
    },
  ],
  { impl: 'ts' },
);
http('POST', '/api/v1/private/order/submit', null, envelope(`@${ids.placed}`), {
  symbol: 'BTC_USDT',
  price: '59000',
  vol: 20,
  side: 1,
  type: 1,
  openType: 2,
  leverage: 10,
  externalOid: 'generated',
});
ws(900, 1, 'push.personal.order', regular(ids.placed, 20, 59000));
command(1100, 'cancelOrders', { ids: [ids.placed] });
http('POST', '/api/v1/private/order/cancel', null, envelope([{ orderId: `@${ids.placed}`, errorCode: 0 }]), [
  ids.placed,
]);
command(1300, 'placeOrders', {
  orders: [{ symbol: 'BTCUSDT', type: 'stop_market', side: 'sell', amount: 10, price: 55000 }],
});
http('POST', '/api/v1/private/planorder/place/v2', null, envelope(`@${ids.plan}`), {
  symbol: 'BTC_USDT',
  vol: 10,
  leverage: 10,
  side: 4,
  openType: 2,
  triggerPrice: '55000',
  triggerType: 2,
  executeCycle: 2,
  orderType: 5,
  trend: 1,
  positionMode: 1,
  reduceOnly: true,
});
ws(1500, 1, 'push.personal.plan.order', plan);
command(1700, 'cancelOrders', { ids: [ids.plan] });
http('POST', '/api/v1/private/planorder/cancel', null, envelope(null), [{ symbol: 'BTC_USDT', orderId: ids.plan }]);
ws(1900, 1, 'push.personal.plan.order', { ...plan, state: 2 });
command(2100, 'fetchLeverage', { symbol: 'ETHUSDT' });
// Flat ETH settings exercise both rails independently of live positions.
command(2300, 'setLeverage', { symbol: 'ETHUSDT', leverage: 12 });
for (const positionType of [1, 2])
  http('POST', '/api/v1/private/position/change_leverage', null, envelope(null), {
    symbol: 'ETH_USDT',
    positionType,
    leverage: 12,
    openType: 2,
  });
command(2500, 'setMarginType', { symbol: 'ETHUSDT', isolated: true });
for (const positionType of [1, 2])
  http('POST', '/api/v1/private/position/change_leverage', null, envelope(null), {
    symbol: 'ETH_USDT',
    positionType,
    leverage: 12,
    openType: 1,
  });
command(2700, 'setPositionMode', { hedged: true });
http('POST', '/api/v1/private/position/change_position_mode', null, envelope(null), { positionMode: 1 });
command(2900, 'cancelSymbolOrders', { symbol: 'BTCUSDT' });
http('POST', '/api/v1/private/order/cancel_all', null, envelope(null), { symbol: 'BTC_USDT' });
http('POST', '/api/v1/private/planorder/cancel_all', null, envelope(null), { symbol: 'BTC_USDT' });
http('POST', '/api/v1/private/stoporder/cancel_all', null, envelope(null), { symbol: 'BTC_USDT' });
ws(3100, 1, 'push.personal.order', { ...resting, state: 4 });
// The denied-batch fallback waits 1500ms before its first regular submit,
// delaying the later plan acknowledgement and scoped refresh beyond 3300ms.
// The final pong observations below keep the tape alive through reconciliation.
ws(3300, 1, 'push.personal.stop.order', { ...stop, state: 2 });

// TS has no opening conditional/trailing implementation. A valid first leg
// must not leak a regular write when a later leg requests either capability.
for (const [index, type] of ['conditional', 'trailing_stop_market'].entries()) {
  command(575 + index * 50, 'placeOrders', { orders: [
    { symbol: 'BTCUSDT', type: 'limit', side: 'buy', amount: 20, price: 59000 },
    { symbol: 'BTCUSDT', type, side: 'buy', amount: 20, price: 59000, triggerPrice: 61000 },
  ] });
}

// Open-order reads reflect the accepted cancel-all instead of replaying startup
// rows forever. Gate the snapshots on command order, including the early rejects.
const afterCommand = records.filter((r) => r.type === 'command' && r.t <= 2900).length;
for (const path of ['/api/v1/private/order/list/open_orders', '/api/v1/private/order/list/open_orders/BTC_USDT']) {
  http('GET', path, { page_num: 1, page_size: 100 }, envelope([]), undefined, { afterCommand });
}
for (const symbol of [undefined, 'BTC_USDT']) {
  const window = {
    start_time: EPOCH - 89 * 86400000,
    end_time: EPOCH,
    page_num: 1,
    page_size: 100,
    ...(symbol ? { symbol } : {}),
  };
  http('GET', '/api/v1/private/planorder/list/orders', { ...window, states: '1' }, envelope([]), undefined, { afterCommand });
  http('GET', '/api/v1/private/stoporder/list/orders', { ...window, is_finished: 0 }, envelope([]), undefined, { afterCommand });
}
// Both polling loops observe a post-cancel snapshot after the terminal WS frames
// before the gate compares their reconciled account state.
ws(4000, 1, 'pong', {});
ws(6000, 1, 'pong', {});

records.sort((a, b) => a.t - b.t);
mkdirSync(dirname(values.out), { recursive: true });
writeFileSync(values.out, `${records.map((r) => JSON.stringify(r)).join('\n')}\n`);
console.log(JSON.stringify({ out: values.out, records: records.length }));
