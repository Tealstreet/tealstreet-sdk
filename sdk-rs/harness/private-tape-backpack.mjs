// Deterministic Backpack perpetual tape. Payload field definitions/examples:
// https://docs.backpack.exchange/#tag/Streams/Order-update and Position-update.
// All signing uses the public deterministic test seed in venues.mjs.
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname } from 'node:path';
import { parseArgs } from 'node:util';
const { values } = parseArgs({
  options: { out: { type: 'string', default: 'sdk-rs/harness/tapes/backpack-private.jsonl' } },
});
const REST = 'https://api.backpack.exchange';
const WS = 'wss://ws.backpack.exchange';
const SYMBOL = 'SOL_USDC_PERP';
const BTC = 'BTC_USDC_PERP';
const BASE = 1765805280000;
const records = [];
let clock = 0;
let commands = 0;
const http = (method, path, body, requestBody, afterCommand) =>
  records.push({
    t: 0,
    ...(afterCommand === undefined ? {} : { afterCommand }),
    type: 'http',
    method,
    url: `${REST}${path}`,
    status: 200,
    body: JSON.stringify(body),
    ...(requestBody === undefined ? {} : { requestBody: JSON.stringify(requestBody) }),
  });
const ws = (stream, data) =>
  records.push({ t: (clock += 400), type: 'ws', url: WS, text: JSON.stringify({ stream, data }) });
const command = (data) => { commands++; records.push({ t: (clock += 400), type: 'command', ...data }); };
const market = (symbol, step, tick) => ({
  symbol,
  marketType: 'PERP',
  visible: true,
  filters: {
    quantity: { stepSize: step, minQuantity: step, maxQuantity: null },
    price: { tickSize: tick, minPrice: tick },
  },
  imfFunction: { base: '0.02' },
});
http('GET', '/api/v1/markets', [market(SYMBOL, '0.01', '0.01'), market(BTC, '0.001', '0.1')]);
http('GET', '/api/v1/tickers', [
  { symbol: SYMBOL, lastPrice: '141', priceChangePercent: '0.01', volume: '1000', quoteVolume: '141000' },
  { symbol: BTC, lastPrice: '90000', priceChangePercent: '-0.005', volume: '100', quoteVolume: '9000000' },
]);
http('GET', '/api/v1/markPrices', [
  {
    symbol: SYMBOL,
    markPrice: '141',
    indexPrice: '140.9',
    fundingRate: '0.0001',
    nextFundingTimestamp: BASE + 3600000,
  },
  { symbol: BTC, markPrice: '90000', indexPrice: '89990', fundingRate: '0.0001', nextFundingTimestamp: BASE + 3600000 },
]);
http('GET', '/api/v1/account', { leverageLimit: '10' });
http('GET', '/api/v1/position', [
  {
    symbol: SYMBOL,
    positionId: '1111343026172067',
    netQuantity: '5',
    entryPrice: '140',
    netExposureNotional: '705',
    pnlUnrealized: '5',
    estLiquidationPrice: '100',
  },
]);
http('GET', '/api/v1/capital/collateral', {
  netEquity: '1005',
  netEquityAvailable: '905',
  netEquityLocked: '100',
  pnlUnrealized: '5',
});
const resting = {
  id: '100',
  clientId: 42,
  postOnly: true,
  symbol: SYMBOL,
  side: 'Bid',
  orderType: 'Limit',
  price: '130',
  quantity: '5',
  executedQuantity: '0',
  timeInForce: 'GTC',
  status: 'New',
  createdAt: BASE,
};
http('GET', '/api/v1/orders', [resting]);
const placed = (id, request) => ({ ...resting, ...request, id, createdAt: BASE });
const order = (id, extra = {}) => ({
  e: 'orderAccepted',
  E: (BASE + clock) * 1000,
  T: (BASE + clock) * 1000,
  i: id,
  s: SYMBOL,
  S: 'Bid',
  o: 'Limit',
  p: '140',
  q: '1',
  z: '0',
  X: 'New',
  f: 'GTC',
  r: false,
  ...extra,
});
const position = (symbol, q, entry, notional, extra = {}) => ({
  s: symbol,
  i: '1111343026172067',
  b: entry,
  B: entry,
  q,
  Q: String(Math.abs(Number(q))),
  n: notional,
  P: '2',
  p: '0',
  l: '0',
  f: '0.05',
  E: (BASE + clock) * 1000,
  ...extra,
});
ws(`bookTicker.${SYMBOL}`, { e: 'bookTicker', s: SYMBOL, b: '140.99', a: '141.01', E: BASE * 1000 });
ws('account.positionUpdate', position(BTC, '-0.02', '89900', '1800'));
ws('account.orderUpdate', order('200', { s: BTC, c: 123, S: 'Bid', p: '90000', q: '0.01' }));
const fill = order('200', {
  e: 'orderFill',
  s: BTC,
  c: 123,
  p: '90000',
  q: '0.01',
  l: '0.004',
  L: '89999',
  z: '0.004',
  X: 'PartiallyFilled',
  t: 999,
});
ws('account.orderUpdate', fill);
ws('account.orderUpdate', fill); // duplicate replay must not emit a second execution
ws(
  'account.orderUpdate',
  order('200', {
    e: 'orderFill',
    s: BTC,
    c: 123,
    p: '90000',
    q: '0.01',
    l: '0.006',
    L: '89998',
    z: '0.01',
    X: 'Filled',
    t: 1000,
  }),
);
ws(
  'account.orderUpdate',
  order('300', {
    e: 'triggerPlaced',
    o: 'Market',
    S: 'Ask',
    p: undefined,
    P: '130',
    Y: '3',
    q: '0',
    X: 'TriggerPending',
    B: 'MarkPrice',
    r: true,
  }),
);
const limit = {
  symbol: SYMBOL,
  side: 'Bid',
  orderType: 'Limit',
  price: '140.12',
  quantity: '1.23',
  timeInForce: 'GTC',
  postOnly: false,
  clientId: 1234,
};
command({
  op: 'placeOrders',
  orders: [{ symbol: SYMBOL, type: 'limit', side: 'buy', amount: 1.234, price: 140.123, clientOrderId: '1234' }],
});
http('POST', '/api/v1/order', placed('400', limit), limit);
ws('account.orderUpdate', order('400', { c: 1234, p: '140.12', q: '1.23' }));
command({ op: 'amendOrder', id: '400', price: 142, amount: 1 });
// Cancel 200 returns the complete terminal order: https://docs.backpack.exchange/#tag/Order
http('DELETE', '/api/v1/order', placed('400', { ...limit, status: 'Cancelled' }), { symbol: SYMBOL, orderId: '400' });
const replacement = { ...limit, price: '142.00', quantity: '1.00' };
delete replacement.clientId;
http('POST', '/api/v1/order', placed('401', replacement), replacement);
ws('account.orderUpdate', order('400', { c: 1234, X: 'Cancelled', p: '140.12', q: '1.23' }));
ws('account.orderUpdate', order('401', { p: '142', q: '1' }));
command({ op: 'cancelOrders', ids: ['401'] });
http('DELETE', '/api/v1/order', placed('401', { ...replacement, status: 'Cancelled' }), { symbol: SYMBOL, orderId: '401' });
ws('account.orderUpdate', order('401', { e: 'orderCancelled', X: 'Cancelled', p: '142', q: '1' }));
command({
  op: 'placeOrders',
  orders: [{ symbol: SYMBOL, type: 'market', side: 'buy', amount: 2, takeProfit: 160, stopLoss: 120 }],
});
const attached = {
  symbol: SYMBOL,
  side: 'Bid',
  orderType: 'Market',
  quantity: '2.00',
  takeProfitTriggerPrice: '160.00',
  takeProfitTriggerBy: 'MarkPrice',
  stopLossTriggerPrice: '120.00',
  stopLossTriggerBy: 'MarkPrice',
};
http('POST', '/api/v1/order', placed('500', attached), attached);
// Known TS bug: attached protective exits become stop-limit orders. Keep its
// exact legacy request separately; Rust must send the market-protection body.
records.at(-1).tsRequestBody = JSON.stringify({
  ...attached,
  takeProfitLimitPrice: '160.00',
  stopLossLimitPrice: '120.00',
});
command({
  op: 'placeOrders',
  orders: [{ symbol: SYMBOL, type: 'limit', side: 'buy', amount: 1, price: 135, takeProfit: 150, stopLoss: 125 }],
});
const entry = { ...replacement, price: '135.00' };
http('POST', '/api/v1/order', placed('600', entry), entry);
for (const [id, price] of [
  ['601', '150.00'],
  ['602', '125.00'],
]) {
  const exit = {
    symbol: SYMBOL,
    side: 'Ask',
    orderType: 'Market',
    triggerQuantity: '1.00',
    triggerPrice: price,
    triggerBy: 'MarkPrice',
    reduceOnly: true,
  };
  http('POST', '/api/v1/order', placed(id, exit), exit);
}
command({ op: 'cancelSymbolOrders', symbol: SYMBOL });
http('DELETE', '/api/v1/orders', [], { symbol: SYMBOL }, commands);
// Once the cancel-all is acknowledged, the venue lists no open orders.
http('GET', '/api/v1/orders', [], undefined, commands);
ws('account.orderUpdate', order('100', { e: 'orderCancelled', X: 'Cancelled', p: '130', q: '5', c: 42, y: true }));
ws(
  'account.orderUpdate',
  order('300', {
    e: 'triggerFailed',
    X: 'TriggerFailed',
    o: 'Market',
    S: 'Ask',
    p: undefined,
    P: '130',
    Y: '3',
    q: '0',
    r: true,
  }),
);
command({ op: 'setLeverage', symbol: SYMBOL, leverage: 20 });
http('PATCH', '/api/v1/account', { leverageLimit: '20' }, { leverageLimit: '20' });
ws('account.positionUpdate', position(BTC, '-0.02', '89800', '1800', { b: '89900', B: '89800' }));
mkdirSync(dirname(values.out), { recursive: true });
writeFileSync(values.out, records.map((r) => JSON.stringify(r)).join('\n') + '\n');
console.log(JSON.stringify({ out: values.out, records: records.length, seconds: clock / 1000 }));
