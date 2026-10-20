import assert from 'node:assert/strict';
import test from 'node:test';
import { fixOrderly, ORDERLY_TS_FIXES } from './known-ts-fixes-orderly.mjs';

const BTC = 'PERP_BTC_USDC';
const meta = { type: 'meta', private: true };
const http = (method, route, rows, body) => ({ type: 'http', method, url: `https://example.test${route}`, status: 200,
  body: JSON.stringify({ success: true, data: rows }), ...(body === undefined ? {} : { requestBody: JSON.stringify(body) }) });
const command = (op, args = {}) => ({ type: 'command', op, ...args });
const ws = (topic, data) => ({ type: 'ws', text: JSON.stringify({ topic, data, ts: 1000 }) });
const account = () => ({ fills: [], orders: [], settings: [], positions: [{ untouched: 'positions' }], balance: { untouched: 'balance' } });
const fill = { id: 8, order_id: 2, symbol: BTC, side: 'BUY', executed_price: 50000,
  executed_quantity: 0.1, executed_timestamp: 2000, fee: 0.5, fee_asset: 'USDC', is_maker: 1 };

test('query stub correction is input-derived and leaves protocol failures visible', () => {
  const ts = { commands: [{ ok: [] }], invalidSignatures: 7, missingWrites: 3 };
  fixOrderly(ts, [meta, command('fetchFills', { symbol: 'BTCUSDC', from: 1000, to: 3000, limit: 2 }),
    http('GET', `/v1/trades?symbol=${BTC}&start_t=1000&end_t=3000&page=1&size=500`, { rows: [fill], meta: { total: 1 } })]);
  assert.deepEqual(ts.commands[0], { ok: [{ id: '8', timestamp: 2, symbol: 'BTCUSDC', orderId: '2', price: 50000,
    qty: 0.1, side: 'buy', commission: 0.5, commissionAsset: 'USDC', maker: true }] });
  assert.equal(ts.invalidSignatures, 7);
  assert.equal(ts.missingWrites, 3);
  assert.notEqual(ts.commands[0].ok[0].price, 1, 'an incorrect native price remains distinguishable');
  assert.equal(fixOrderly.length, 2, 'there is no native-output argument');
});

test('query oracle refuses unexpected TS errors or nonempty data', () => {
  const records = [meta, command('fetchFills', { limit: 2 }),
    http('GET', '/v1/trades?page=1&size=500', { rows: [fill], meta: { total: 1 } })];
  assert.throws(() => fixOrderly({ commands: [{ error: 'bad native request' }] }, records), /unexpected TS/);
  assert.throws(() => fixOrderly({ commands: [{ ok: [{ price: 1 }] }] }, records), /unexpected TS/);
});

test('paged fill oracle deduplicates overlap and applies exact time bounds', () => {
  const ts = { commands: [{ ok: [] }] };
  fixOrderly(ts, [meta, command('fetchFills', { from: 2000, to: 3000, limit: 10 }),
    http('GET', '/v1/trades?start_t=2000&end_t=3000&page=1&size=500', { rows: [fill], meta: { total: 3 } }),
    http('GET', '/v1/trades?start_t=2000&end_t=3000&page=2&size=500', { rows: [fill, { ...fill, id: 9, executed_timestamp: 1999 },
      { ...fill, id: 10, executed_timestamp: 3000 }], meta: { total: 3 } })]);
  assert.deepEqual(ts.commands[0].ok.map((r) => r.id), ['10', '8']);
});

test('funding direction and provisional settlement are independently checked', () => {
  const payment = { symbol: BTC, funding_fee: 1.5, funding_rate: 0.0001, mark_price: 50000, status: 'Settled', created_time: 2000, payment_type: 'Pay' };
  const ts = { commands: [{ ok: [] }] };
  fixOrderly(ts, [meta, command('fetchIncome', { from: 1000, to: 3000, limit: 10 }),
    http('GET', '/v1/funding_fee/history?start_t=1000&end_t=3000&page=1&size=500', { rows: [payment,
      { ...payment, payment_type: 'Receive', created_time: 2100 }, { ...payment, status: 'Accrued' }], meta: { total: 3 } })]);
  assert.deepEqual(ts.commands[0].ok.map((r) => r.income), [1.5, -1.5]);
  assert.ok(ts.commands[0].ok.every((r) => r.category === 'funding' && r.timestamp < 3));
});

test('public trades retain identical executions using distinct stable occurrence IDs', () => {
  const row = { symbol: BTC, side: 'SELL', executed_price: 50000, executed_quantity: 0.1, executed_timestamp: 2000 };
  const ts = { commands: [{ error: 'Not implemented' }] };
  fixOrderly(ts, [meta, command('fetchPublicTrades', { symbol: 'BTCUSDC', limit: 2 }),
    http('GET', `/v1/public/market_trades?symbol=${BTC}&limit=2`, { rows: [row, row] })]);
  assert.equal(ts.commands[0].ok.length, 2);
  assert.notEqual(ts.commands[0].ok[0].id, ts.commands[0].ok[1].id);
  assert.equal(ts.commands[0].ok[0].quoteQty, 5000);
});

test('order history merges terminal ordinary and nested protective rows', () => {
  const ts = { commands: [{ ok: [] }] };
  fixOrderly(ts, [meta, command('fetchOrderHistory', { limit: 10 }),
    http('GET', '/v1/orders?page=1&size=500', { rows: [{ order_id: 1, symbol: BTC, type: 'LIMIT', side: 'BUY', status: 'FILLED',
      quantity: 1, total_executed_quantity: 1, price: 50000, created_time: 2000 }], meta: { total: 1 } }),
    http('GET', '/v1/algo/orders?page=1&size=500', { rows: [{ algo_order_id: 2, symbol: BTC, algo_type: 'POSITIONAL_TP_SL', algo_status: 'FILLED',
      created_time: 2100, child_orders: [{ algo_order_id: 3, algo_type: 'STOP_LOSS', algo_status: 'CANCELLED', side: 'SELL', quantity: 0, trigger_price: 49000 }] }], meta: { total: 1 } })]);
  assert.deepEqual(ts.commands[0].ok.map((r) => [r.id, r.status, r.createdTime]), [['3', 'canceled', 2100], ['1', 'closed', 2000]]);
});

test('fill correction requires the demonstrated duplicate stream and uses cumulative increments', () => {
  const row = { orderId: 2, symbol: BTC, type: 'LIMIT', side: 'BUY', status: 'PARTIAL_FILLED',
    quantity: 1, totalExecutedQuantity: 0.25, executedQuantity: 0.25, executedPrice: 50000, tradeId: 7 };
  const old = { id: 2, symbol: 'BTCUSDC', side: 'buy', amount: 0.25, price: 50000 };
  const originalAccount = account();
  originalAccount.fills = [old, old];
  const ts = { commands: [], account: originalAccount };
  fixOrderly(ts, [meta, ws('executionreport', row), ws('executionreport', row)]);
  assert.deepEqual(ts.account.fills, [{ ...old, id: '2' }]);
  assert.deepEqual(ts.account.positions, [{ untouched: 'positions' }]);
  assert.deepEqual(ts.account.balance, { untouched: 'balance' });
  assert.throws(() => fixOrderly({ commands: [], account: { ...account(), fills: [{ ...old, price: 1 }] } },
    [meta, ws('executionreport', row), ws('executionreport', row)]), /unexpected TS/);
});

test('root placement correction requires explicit root and legacy child IDs', () => {
  const ts = { commands: [{ ok: ['3'] }] };
  fixOrderly(ts, [meta, command('placeOrders', { orders: [{ symbol: 'BTCUSDC', type: 'stop_market', side: 'sell', amount: 1, price: 49000 }] }),
    http('POST', '/v1/algo/order', { order_id: 2, rows: [{ orderId: 3 }] }, { symbol: BTC,
      child_orders: [{ algo_type: 'STOP_LOSS', trigger_price: '49000' }] })]);
  assert.deepEqual(ts.commands, [{ ok: ['2'] }]);
});

test('order corrections leave unrelated fields and unknown rows untouched', () => {
  const ts = { commands: [], account: account() };
  ts.account.orders = [{ id: '2', type: 'limit', clientOrderId: null, timeInForce: null, price: 49900, status: 'open', filled: 0.25 },
    { id: 'unrelated', price: 123, status: 'open' }];
  fixOrderly(ts, [meta, ws('executionreport', { orderId: 2, symbol: BTC, type: 'LIMIT', side: 'BUY', status: 'NEW',
    quantity: 1, price: 49900, clientOrderId: 'caller-exact' })]);
  assert.equal(ts.account.orders[0].clientOrderId, 'caller-exact');
  assert.equal(ts.account.orders[0].timeInForce, 'GoodTillCancel');
  assert.equal(ts.account.orders[0].filled, 0.25);
  assert.deepEqual(ts.account.orders[1], { id: 'unrelated', price: 123, status: 'open' });
  assert.ok(ORDERLY_TS_FIXES.length >= 5);
});

test('root terminals prune only older confirmed rows and preserve later canceled children', () => {
  const child = (childId, root, kind, price) => ({ algo_order_id: childId, root_algo_order_id: root,
    symbol: BTC, algo_type: kind, trigger_price: price, side: 'SELL', quantity: 0 });
  const old = child(11, 10, 'TAKE_PROFIT', 55000);
  const fresh = [child(21, 20, 'STOP_LOSS', 49000), child(22, 20, 'TAKE_PROFIT', 55000)];
  const ts = { commands: [{ ok: ['21', '22'] }], account: { ...account(), orders: [
    { id: '11', type: 'take_profit_market', status: 'open', trigger: null },
    { id: '21', type: 'stop_market', status: 'open', trigger: null },
    { id: '22', type: 'take_profit_market', status: 'open', trigger: null },
    { id: 'unrelated', status: 'open', price: 1 },
  ] } };
  const staged = http('GET', '/v1/algo/orders?status=INCOMPLETE&page=1&size=500', { rows: [{ algo_order_id: 20,
    root_algo_order_id: 20, symbol: BTC, algo_type: 'POSITIONAL_TP_SL', child_orders: fresh }] });
  staged.afterCommand = 1;
  fixOrderly(ts, [meta,
    http('GET', '/v1/orders?status=INCOMPLETE&page=1&size=500', { rows: [] }),
    http('GET', '/v1/algo/orders?status=INCOMPLETE&page=1&size=500', { rows: [{ algo_order_id: 10,
      root_algo_order_id: 10, symbol: BTC, algo_type: 'POSITIONAL_TP_SL', child_orders: [old] }] }),
    command('placeOrders', { orders: [{ symbol: 'BTCUSDC', type: 'stop_market', side: 'sell', amount: 1, price: 49000 }] }),
    http('POST', '/v1/algo/order', { order_id: 20, rows: [{ orderId: 21 }, { orderId: 22 }] },
      { symbol: BTC, child_orders: [{ algo_type: 'STOP_LOSS', trigger_price: '49000' }] }), staged,
    ws('algoexecutionreportv2', { root_algo_order_id: 10, algo_type: 'POSITIONAL_TP_SL', algo_status: 'CANCELLED' }),
    ws('algoexecutionreportv2', { root_algo_order_id: 20, algo_type: 'POSITIONAL_TP_SL', algo_status: 'CANCELLED' }),
  ]);
  assert.deepEqual(ts.account.orders.map((r) => r.id), ['21', '22', 'unrelated']);
  assert.deepEqual(ts.account.orders.slice(0, 2).map((r) => [r.status, r.trigger]), [['canceled', 'mark_price'], ['canceled', 'mark_price']]);
  assert.deepEqual(ts.account.orders[2], { id: 'unrelated', status: 'open', price: 1 });
});

test('settings use explicit default selectors and successful native writes', () => {
  const ts = { commands: [], account: { ...account(), settings: [
    { symbol: 'BTCUSDC', marginType: 'isolated', leverage: 5 },
    { symbol: 'ETHUSDC', marginType: 'isolated', leverage: 20 },
  ] } };
  fixOrderly(ts, [meta,
    http('GET', '/v1/public/info', { rows: [{ symbol: BTC }, { symbol: 'PERP_ETH_USDC' }] }),
    http('GET', '/v1/client/info', { max_leverage: 20 }),
    http('GET', '/v1/client/margin_modes', { rows: [{ symbol: BTC, default_margin_mode: 'CROSS' },
      { symbol: 'PERP_ETH_USDC', default_margin_mode: 'ISOLATED' }] }),
    http('GET', '/v1/positions', { rows: [{ symbol: BTC, margin_mode: 'CROSS', leverage: 10 },
      { symbol: BTC, margin_mode: 'ISOLATED', leverage: 5 }] }),
    http('POST', '/v1/client/leverages', {}, { symbol: BTC, margin_mode: 'CROSS', leverage: 12 }),
    http('POST', '/v1/client/margin_mode', {}, { symbol: 'PERP_ETH_USDC', default_margin_mode: 'CROSS' }),
  ]);
  assert.deepEqual(ts.account.settings, [{ symbol: 'BTCUSDC', marginType: 'cross', leverage: 12 },
    { symbol: 'ETHUSDC', marginType: 'cross', leverage: 20 }]);
});
