// Documented V5 payloads for the category/account variants. No keys or live orders.
// corepack yarn rust:private-tape-bybitinverse (or -bybitspot/-bybitdemo/-hyrotrader)
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname } from 'node:path';
import { parseArgs } from 'node:util';

const { values } = parseArgs({
  options: { venue: { type: 'string' }, public: { type: 'boolean', default: false }, out: { type: 'string' } },
});
const name = values.venue;
if (!['bybitinverse', 'bybitspot', 'bybitdemo', 'hyrotrader'].includes(name)) throw new Error('unknown Bybit variant');
const inverse = name === 'bybitinverse';
const spot = name === 'bybitspot';
const hyro = name === 'hyrotrader';
const category = inverse ? 'inverse' : spot ? 'spot' : 'linear';
const rest = hyro
  ? 'https://hyro.example.invalid'
  : name === 'bybitdemo'
    ? 'https://api-demo.bybit.com'
    : 'https://api.bybit.com';
const publicWs = hyro ? 'wss://hyro.example.invalid/v5/public/linear' : `wss://stream.bybit.com/v5/public/${category}`;
const privateWs = hyro
  ? 'wss://hyro.example.invalid/v5/private'
  : name === 'bybitdemo'
    ? 'wss://stream-demo.bybit.com/v5/private'
    : 'wss://stream.bybit.com/v5/private';
const symbols = inverse ? ['BTCUSD', 'ETHUSD'] : ['BTCUSDT', 'ETHUSDT'];
const records = [];
if (hyro) records.push({ t: 0, type: 'config', options: { endpoints: { rest, publicWs, privateWs } } });
const ok = (result, extra = {}) => ({ retCode: 0, retMsg: 'OK', result, retExtInfo: {}, ...extra });
const list = (rows) => ok({ category, list: rows, nextPageCursor: '' });
const http = (path, params, response, method = 'GET', body, impl) =>
  records.push({
    t: 0,
    type: 'http',
    method,
    url: `${rest}${path}${params ? `?${new URLSearchParams(params)}` : ''}`,
    status: 200,
    body: JSON.stringify(response),
    ...(body && { requestBody: JSON.stringify(body) }),
    ...(impl && { impl }),
  });
let clock = 0;
const ws = (url, text) => records.push({ t: (clock += 250), type: 'ws', url, text: JSON.stringify(text) });
const stream = (topic, rows) =>
  ws(privateWs, { id: `${topic}-${clock}`, topic, creationTime: 1760880000000 + clock, data: rows });
const command = (op, args) => records.push({ t: (clock += 250), type: 'command', op, ...args });
const instruments = symbols.map((symbol, i) => ({
  symbol,
  status: 'Trading',
  contractType: inverse ? 'InversePerpetual' : 'LinearPerpetual',
  baseCoin: i ? 'ETH' : 'BTC',
  quoteCoin: inverse ? 'USD' : 'USDT',
  settleCoin: inverse ? (i ? 'ETH' : 'BTC') : 'USDT',
  deliveryTime: '0',
  priceFilter: { tickSize: i ? '0.01' : '0.10' },
  leverageFilter: { minLeverage: '1', maxLeverage: '100' },
  lotSizeFilter: spot
    ? {
        basePrecision: i ? '0.00001' : '0.000001',
        quotePrecision: '0.00000001',
        minOrderQty: '0.000001',
        minOrderAmt: '5',
        maxOrderQty: '100',
        maxLimitOrderQty: '100',
        maxMarketOrderQty: '80',
      }
    : {
        qtyStep: inverse ? '1' : i ? '0.01' : '0.001',
        minOrderQty: inverse ? '1' : i ? '0.01' : '0.001',
        maxOrderQty: inverse ? '1000000' : '1000',
      },
}));
http('/v5/market/instruments-info', { category, limit: '1000' }, list(instruments));
if (spot) http('/v5/market/instruments-info', { category }, list(instruments));
else http('/v5/market/instruments-info', { category, limit: '1000', status: 'PreLaunch' }, list([]));
const tickers = symbols.map((symbol, i) => ({
  symbol,
  lastPrice: i ? '3000' : '96000',
  markPrice: i ? '3000' : '96000',
  indexPrice: i ? '3000' : '96000',
  bid1Price: i ? '2999.99' : '95999.9',
  ask1Price: i ? '3000' : '96000',
  price24hPcnt: '0.01',
  volume24h: '1000',
  turnover24h: i ? '3000000' : '96000000',
  openInterest: '1000',
  fundingRate: '0.0001',
  nextFundingTime: '1760889600000',
}));
http('/v5/market/tickers', { category }, list(tickers));
http('/v5/account/info', null, ok({ marginMode: 'REGULAR_MARGIN', unifiedMarginStatus: 6 }));
http(
  '/v5/user/query-api',
  null,
  ok({ id: '13770661', apiKey: 'replay-key', readOnly: 0, vipLevel: 'No VIP', unified: 0, uta: 1, isMaster: true }),
);
if (!spot && !hyro)
  http(
    '/v5/account/fee-rate',
    { category },
    list(symbols.map((symbol) => ({ symbol, takerFeeRate: '0.00055', makerFeeRate: '0.0002' }))),
  );
const order = (row) => ({
  category,
  orderId: 'resting',
  orderLinkId: '',
  symbol: symbols[1],
  side: 'Buy',
  orderType: 'Limit',
  orderStatus: 'New',
  price: '2800',
  qty: inverse ? '280' : '0.10',
  cumExecQty: '0',
  cumExecValue: '0',
  leavesQty: inverse ? '280' : '0.10',
  avgPrice: '0',
  timeInForce: 'GTC',
  stopOrderType: '',
  rejectReason: 'EC_NoError',
  triggerPrice: '',
  takeProfit: '',
  stopLoss: '',
  tpTriggerBy: '',
  slTriggerBy: '',
  tpslMode: '',
  reduceOnly: false,
  closeOnTrigger: false,
  positionIdx: 0,
  createdTime: '1760880000000',
  updatedTime: '1760880000000',
  ...row,
});
const resting = order({});
const ordersQuery = { category, ...(!spot && !inverse && { settleCoin: 'USDT' }), limit: '50', cursor: '' };
http('/v5/order/realtime', ordersQuery, list([resting]));
if (!spot && !inverse && !hyro) http('/v5/order/realtime', { ...ordersQuery, settleCoin: 'USDC' }, list([]));
const btc = {
  category,
  symbol: symbols[0],
  side: 'Buy',
  size: inverse ? '960' : '0.01',
  avgPrice: '95000',
  entryPrice: '95000',
  markPrice: '96000',
  positionValue: inverse ? '0.01' : '950',
  leverage: '10',
  positionIM: inverse ? '0.001' : '95',
  unrealisedPnl: inverse ? '0.0001' : '10',
  curRealisedPnl: '0',
  liqPrice: '80000',
  positionIdx: 0,
  seq: 100,
  updatedTime: '1760880000000',
};
const eth = {
  ...btc,
  symbol: symbols[1],
  side: '',
  size: '0',
  avgPrice: '0',
  entryPrice: '0',
  markPrice: '3000',
  positionValue: '0',
  leverage: '20',
  positionIM: '0',
  unrealisedPnl: '0',
  liqPrice: '',
};
if (!spot) {
  const params = { category, ...(!inverse && { settleCoin: 'USDT' }), limit: hyro ? '50' : '200', cursor: '' };
  http('/v5/position/list', params, list([btc, eth]));
  if (hyro) http('/v5/position/list', { ...params, limit: '200' }, list([btc, eth]));
  if (!inverse && !hyro) http('/v5/position/list', { ...params, settleCoin: 'USDC' }, list([]));
}
const wallet = {
  accountType: 'UNIFIED',
  totalEquity: '5010',
  totalWalletBalance: '5000',
  totalMarginBalance: '5010',
  totalAvailableBalance: '4915',
  totalInitialMargin: '95',
  totalMaintenanceMargin: '3',
  coin: [
    {
      coin: 'USDT',
      equity: '5010',
      usdValue: '5010',
      walletBalance: '5000',
      totalPositionIM: '95',
      totalOrderIM: '0',
      locked: '0',
      bonus: '0',
    },
  ],
};
http('/v5/account/wallet-balance', { accountType: 'UNIFIED' }, ok({ list: [wallet] }));
ws(publicWs, {
  topic: `tickers.${symbols[1]}`,
  type: 'delta',
  data: { symbol: symbols[1], lastPrice: '3001', volume24h: '1001' },
});
ws(privateWs, { op: 'auth', success: true, ret_msg: '', ...(hyro && { req_id: '@latest' }) });
if (hyro) ws(privateWs, { op: 'subscribe', success: true, ret_msg: '', req_id: '@latest' });
if (!spot) stream('position', [btc]);
const fillQty = inverse ? '300' : '1';
const partial = inverse ? '120' : '0.4';
const remainder = inverse ? '180' : '0.6';
const fill = order({ orderId: 'filled', qty: fillQty, price: '3000' });
stream('order', [{ ...fill, leavesQty: fillQty }]);
stream('order', [
  {
    ...fill,
    orderStatus: 'PartiallyFilled',
    cumExecQty: partial,
    cumExecValue: inverse ? '0.04' : '1200',
    avgPrice: '3000',
    leavesQty: remainder,
  },
]);
if (!hyro)
  stream('execution', [
    {
      category,
      symbol: symbols[1],
      orderId: 'filled',
      execId: 'exec-1',
      side: 'Buy',
      execType: 'Trade',
      execQty: partial,
      execPrice: '3000',
    },
  ]);
stream('order', [
  {
    ...fill,
    orderStatus: 'Filled',
    cumExecQty: fillQty,
    cumExecValue: inverse ? '0.1' : '3000',
    avgPrice: '3000',
    leavesQty: '0',
  },
]);
if (!hyro)
  stream('execution', [
    {
      category,
      symbol: symbols[1],
      orderId: 'filled',
      execId: 'exec-2',
      side: 'Buy',
      execType: 'Trade',
      execQty: remainder,
      execPrice: '3000',
    },
  ]);
// Inverse/native and demo/Hyro writes are compared too; spot precision and
// execution-control TS defects are covered by native tests rather than bad TS writes.
if (!spot) {
  const qty = inverse ? '200' : '0.002';
  const placed = 'placed-variant';
  command('placeOrders', {
    orders: [{ symbol: symbols[0], side: 'buy', type: 'limit', amount: inverse ? 200 : 0.002, price: 90500 }],
  });
  const payload = {
    symbol: symbols[0],
    side: 'Buy',
    orderType: 'Limit',
    qty,
    price: '90500',
    reduceOnly: false,
    timeInForce: 'GTC',
    closeOnTrigger: false,
    positionIdx: 0,
  };
  if (hyro)
    http('/v5/order/create', null, ok({ orderId: placed }), 'POST', {
      category,
      ...payload,
      orderLinkId: '@generated',
    });
  else
    http(
      '/v5/order/create-batch',
      null,
      ok({ list: [{ orderId: placed }] }, { retExtInfo: { list: [{ code: 0, msg: 'OK' }] } }),
      'POST',
      { category, request: [payload] },
    );
  const placedRow = order({ orderId: placed, symbol: symbols[0], price: '90500', qty, leavesQty: qty });
  stream('order', [placedRow]);
}
command('amendOrder', { id: 'resting', price: 2810 });
http('/v5/order/amend', null, ok({ orderId: 'resting' }), 'POST', {
  category,
  symbol: symbols[1],
  orderId: 'resting',
  price: '2810',
});
stream('order', [{ ...resting, price: '2810' }]);
command(hyro ? 'cancelOrders' : 'cancelSymbolOrders', hyro ? { ids: ['resting'] } : { symbol: symbols[1] });
if (hyro)
  http('/v5/order/cancel', null, ok({ orderId: 'resting' }), 'POST', {
    category,
    symbol: symbols[1],
    orderId: 'resting',
  });
else {
  const response = ok({ list: [{ orderId: 'resting' }], success: '1' });
  const body = { category, symbol: symbols[1] };
  http('/v5/order/cancel-all', null, response, 'POST', body, spot ? 'ts' : undefined);
  if (spot) {
    // KNOWN_TS_FIXES: unfiltered spot cancel-all leaves trigger families live.
    // Only an ordinary order is present here; both replays share its events.
    for (const orderFilter of ['Order', 'StopOrder', 'tpslOrder']) {
      const result = orderFilter === 'Order' ? response : ok({ list: [], success: '1' });
      http('/v5/order/cancel-all', null, result, 'POST', { ...body, orderFilter }, 'rust');
    }
  }
}
stream('order', [
  { ...resting, price: '2810', orderStatus: 'Cancelled', leavesQty: '0', rejectReason: 'EC_PerCancelRequest' },
]);
if (values.public) {
  if (!hyro) throw new Error('documented public tape is for Hyro; record live Bybit variants');
  const kept = records.filter(
    (r) => r.type === 'config' || (r.type === 'http' && new URL(r.url).pathname.startsWith('/v5/market/')),
  );
  // Hyro's documented simulator contract uses the V5 snapshot/delta shapes.
  // This is a deterministic contract fixture, not evidence of live connectivity.
  for (let i = 0; i < 40; i++) {
    for (const [index, symbol] of symbols.entries()) {
      const price = index ? 3000 : 96000;
      kept.push({
        t: 100 + i * 100,
        type: 'ws',
        url: publicWs,
        text: JSON.stringify({
          topic: `tickers.${symbol}`,
          type: 'delta',
          data: {
            symbol,
            lastPrice: String(price + i / 10),
            volume24h: String(1000 + i),
            turnover24h: String(price * (1000 + i)),
          },
        }),
      });
      kept.push({
        t: 100 + i * 100,
        type: 'ws',
        url: publicWs,
        text: JSON.stringify({
          topic: `orderbook.1000.${symbol}`,
          type: i === 0 ? 'snapshot' : 'delta',
          data: { s: symbol, b: [[String(price - 1), String(i + 1)]], a: [[String(price + 1), '2']], u: i + 1 },
        }),
      });
    }
  }
  records.splice(0, records.length, ...kept);
}
const out = values.out ?? `sdk-rs/harness/tapes/${name}-${values.public ? 'public-documented' : 'private'}.jsonl`;
mkdirSync(dirname(out), { recursive: true });
writeFileSync(
  out,
  `${records
    .sort((a, b) => a.t - b.t)
    .map((r) => JSON.stringify(r))
    .join('\n')}\n`,
);
console.log(JSON.stringify({ out, records: records.length, venue: name, documented: true }));
