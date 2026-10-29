// Classic USDT-M documented native payloads, fake credentials only.
// Run with the root rust:private-tape-bitget script.
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname } from 'node:path';
import { parseArgs } from 'node:util';
const { values } = parseArgs({
  options: { out: { type: 'string', default: 'sdk-rs/harness/tapes/bitget-private.jsonl' } },
});
const REST = 'https://api.bitget.com';
const PUBLIC = 'wss://ws.bitget.com/v2/ws/public';
const PRIVATE = 'wss://ws.bitget.com/v2/ws/private';
const records = [];
const http = (method, path, query, data, requestBody, code = '00000') =>
  records.push({
    t: 0,
    type: 'http',
    method,
    url: `${REST}${path}${query ? `?${query}` : ''}`,
    requestBody,
    status: 200,
    body: JSON.stringify({ code, msg: code === '00000' ? 'success' : 'Classic Account mode', data }),
  });
const ws = (t, url, message) => records.push({ t, type: 'ws', url, text: JSON.stringify(message) });
const command = (t, op, args) => records.push({ t, type: 'command', op, ...args });
http('GET', '/api/v2/mix/market/contracts', 'productType=usdt-futures', [
  {
    symbol: 'BTCUSDT',
    baseCoin: 'BTC',
    quoteCoin: 'USDT',
    symbolStatus: 'normal',
    pricePlace: '1',
    priceEndStep: '1',
    sizeMultiplier: '0.001',
    minTradeNum: '0.001',
    maxOrderQty: '120',
    maxLever: '125',
    minLever: '1',
  },
]);
http('GET', '/api/v2/mix/market/tickers', 'productType=usdt-futures', [
  {
    symbol: 'BTCUSDT',
    bidPr: '99.9',
    askPr: '100.1',
    lastPr: '100',
    indexPrice: '99',
    changeUtc24h: '0.01',
    baseVolume: '1000',
    quoteVolume: '100000',
    fundingRate: '0.0001',
  },
]);
http('GET', '/api/v3/account/settings', '', null, undefined, '40084');
const position = {
  symbol: 'BTCUSDT',
  instId: 'BTCUSDT',
  marginCoin: 'USDT',
  holdSide: 'long',
  total: '1',
  openPriceAvg: '90',
  markPrice: '100',
  leverage: '20',
  unrealizedPL: '10',
  liquidationPrice: '50',
  marginSize: '50',
  marginMode: 'crossed',
  posMode: 'one_way_mode',
  uTime: '1760880000000',
};
http('GET', '/api/v2/mix/position/all-position', 'productType=usdt-futures&marginCoin=USDT', [position]);
http('GET', '/api/v2/mix/account/accounts', 'productType=usdt-futures', [
  {
    marginCoin: 'USDT',
    unionTotalMargin: '1010',
    unionAvailable: '900',
    unrealizedPL: '10',
    accountEquity: '1010',
    available: '900',
  },
]);
const normal = {
  orderId: '1001',
  clientOid: 'resting',
  symbol: 'BTCUSDT',
  size: '0.002',
  baseVolume: '0',
  price: '90',
  side: 'buy',
  orderType: 'limit',
  status: 'live',
  force: 'gtc',
  reduceOnly: 'NO',
  posMode: 'one_way_mode',
  cTime: '1760880000000',
  uTime: '1760880000000',
};
// Rust reads pending orders unfiltered at limit=200; the TS reference reads
// each documented status at the Classic maximum of 100. Same venue rows.
const tagged = (impl) => Object.assign(records.at(-1), { impl });
const pending = (rows) => {
  http('GET', '/api/v2/mix/order/orders-pending', 'productType=usdt-futures&limit=200', { entrustedList: rows, endId: '' });
  tagged('rust');
  http('GET', '/api/v2/mix/order/orders-pending', 'productType=usdt-futures&status=live&limit=100', {
    entrustedList: rows.filter((row) => row.status === 'live'), endId: '',
  });
  tagged('ts');
  http('GET', '/api/v2/mix/order/orders-pending', 'productType=usdt-futures&status=partially_filled&limit=100', {
    entrustedList: rows.filter((row) => row.status === 'partially_filled'), endId: '',
  });
  tagged('ts');
};
pending([normal]);
for (const planType of ['profit_loss', 'track_plan']) {
  for (const [impl, limit] of [['rust', 200], ['ts', 100]]) {
    http('GET', '/api/v2/mix/order/orders-plan-pending', `productType=usdt-futures&planType=${planType}&limit=${limit}`, {
      entrustedList: [],
      endId: '',
    });
    tagged(impl);
  }
}
http('GET', '/api/v2/mix/account/account', 'symbol=BTCUSDT&productType=usdt-futures&marginCoin=USDT', {
  symbol: 'BTCUSDT',
  posMode: 'one_way_mode',
  marginMode: 'crossed',
  leverage: '20',
});
const orderEvent = (t, row) =>
  ws(t, PRIVATE, {
    arg: { instType: 'USDT-FUTURES', channel: 'orders', instId: 'default' },
    action: 'snapshot',
    data: [row],
  });
ws(0, PRIVATE, { event: 'login', code: '0', connId: 'documented-extra-field' });
ws(100, PUBLIC, {
  arg: { instType: 'USDT-FUTURES', channel: 'ticker', instId: 'BTCUSDT' },
  action: 'snapshot',
  data: [{ instId: 'BTCUSDT', lastPr: '100', bidPr: '99.9', askPr: '100.1' }],
});
ws(200, PRIVATE, {
  arg: { instType: 'USDT-FUTURES', channel: 'positions', instId: 'default' },
  action: 'snapshot',
  data: [position],
});
command(600, 'placeOrders', {
  orders: [
    { symbol: 'BTCUSDT', type: 'limit', side: 'buy', amount: 0.003, price: 91, clientOrderId: 'caller:bitget-1' },
  ],
});
http(
  'POST',
  '/api/v2/mix/order/batch-place-order',
  '',
  { successList: [{ orderId: '1002', clientOid: 'caller:bitget-1' }], failureList: [] },
  {
    symbol: 'BTCUSDT',
    productType: 'usdt-futures',
    marginCoin: 'USDT',
    marginMode: 'crossed',
    orderList: [
      {
        size: '0.003',
        price: '91',
        side: 'buy',
        orderType: 'limit',
        force: 'gtc',
        reduceOnly: 'NO',
        clientOid: 'caller:bitget-1',
      },
    ],
  },
);
// TS spells Classic batch marginMode as cross; the native request stays crossed.
const classicBatch = records.find((record) => record.type === 'http' && record.url.includes('/batch-place-order'));
classicBatch.tsRequestBody = { ...classicBatch.requestBody, marginMode: 'cross' };
orderEvent(1000, { ...normal, orderId: '1002', clientOid: 'caller:bitget-1', size: '0.003', price: '91' });
command(1400, 'amendOrder', { id: '1002', price: 92 });
http(
  'POST',
  '/api/v2/mix/order/modify-order',
  '',
  { orderId: '1003' },
  {
    orderId: '1002',
    symbol: 'BTCUSDT',
    productType: 'usdt-futures',
    newPrice: '92',
    newSize: '0.003',
    newClientOid: 'generated',
  },
);
orderEvent(1800, { ...normal, orderId: '1002', status: 'canceled' });
orderEvent(2000, { ...normal, orderId: '1003', size: '0.003', price: '92', clientOid: 'generated' });
command(2400, 'cancelOrders', { ids: ['1003'] });
http(
  'POST',
  '/api/v2/mix/order/batch-cancel-orders',
  '',
  { successList: [{ orderId: '1003' }], failureList: [] },
  { symbol: 'BTCUSDT', orderIdList: [{ orderId: '1003' }], marginCoin: 'USDT', productType: 'usdt-futures' },
);
orderEvent(2800, { ...normal, orderId: '1003', size: '0.003', price: '92', status: 'canceled' });
orderEvent(3200, {
  ...normal,
  tradeId: 'execution-1',
  status: 'filled',
  baseVolume: '0.002',
  accBaseVolume: '0.002',
  fillPrice: '90',
  priceAvg: '90',
});
// Allow the 3-RPS TS client's queued reconciliation reads to complete.
// Terminal rows are ephemeral; finish with consistent live REST/WS state.
const finalOrder = { ...normal, orderId: '1004', clientOid: 'external-live', size: '0.004', price: '93' };
const final = records.length;
pending([finalOrder]);
for (const record of records.slice(final)) record.availableAfter = 10_000;
orderEvent(10_000, finalOrder);
mkdirSync(dirname(values.out), { recursive: true });
writeFileSync(values.out, `${records.map((r) => JSON.stringify(r)).join('\n')}\n`);
console.log(JSON.stringify({ out: values.out, records: records.length, seconds: 10 }));
