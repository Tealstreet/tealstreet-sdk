// Active WOO V1/V3 documented payloads; synthetic account, replay credentials only.
// Exercises the V1 realized-PnL ledger separately from V3 execution history.
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname } from 'node:path';
import { parseArgs } from 'node:util';
const { values } = parseArgs({ options: { out: { type: 'string', default: 'sdk-rs/harness/tapes/woo-private.jsonl' } } });
const REST = 'https://api.woox.io';
const PUBLIC = 'wss://wss.woox.io/ws/stream/undefined';
const PRIVATE = 'wss://wss.woox.io/v2/ws/private/stream/undefined';
const EPOCH = 1792454400000;
const BROKER = 'ab82cb09-cfec-4473-80a3-b740779d0644';
const records = [{ t: 0, type: 'meta', private: true, epoch: EPOCH }];
const ok = (data) => ({ success: true, data });
const meta = (total, perPage = 100) => ({ total, records_per_page: perPage, current_page: 1 });
const page = (rows, total = rows.length, perPage = 100) => ({ success: true, rows, meta: meta(total, perPage) });
const algoPage = (rows, total = rows.length, perPage = 100) => ok({ rows, meta: meta(total, perPage) });
const form = (params) => new URLSearchParams(Object.entries(params).sort(([a], [b]) => a.localeCompare(b))).toString();
const stage = () => ({ afterCommand: records.filter((r) => r.type === 'command').length });
function http(method, path, params, body, requestBody, extra = {}) {
  records.push({ t: 0, type: 'http', method, url: `${REST}${path}${params && Object.keys(params).length ? `?${new URLSearchParams(params)}` : ''}`, status: 200, body: JSON.stringify(body), ...(requestBody === undefined ? {} : { requestBody: typeof requestBody === 'string' ? requestBody : JSON.stringify(requestBody) }), ...extra });
}
const ws = (t, url, body) => records.push({ t, type: 'ws', url, text: JSON.stringify(body) });
const command = (t, op, args = {}) => records.push({ t, type: 'command', op, ...args });
const info = (positionMode = 'ONE_WAY') => ok({ applicationId: 'undefined', accountMode: 'FUTURES', positionMode, freeCollateral: '900', totalCollateral: '1000', totalAccountValue: '1000' });
const pos = (holding = 0.02, leverage = 10) => ok({ positions: holding ? [{ symbol: 'PERP_BTC_USDT', holding, averageOpenPrice: 60000, markPrice: 60000, leverage, marginMode: 'CROSS', estLiqPrice: 50000 }] : [] });
const leverage = (value = 10) => ok({ symbol: 'PERP_BTC_USDT', margin_mode: 'CROSS', position_mode: 'ONE_WAY', details: [{ position_side: 'BOTH', leverage: String(value) }] });
const regular = (orderId, quantity = 0.003, price = 59000, executed = 0, status = 'NEW', extra = {}) => ({ symbol: 'PERP_BTC_USDT', order_id: orderId, orderId, quantity, order_quantity: quantity, price, order_price: price, executed, status, side: 'BUY', type: 'LIMIT', order_type: 'LIMIT', reduce_only: false, ...extra });
const leaf = (algoOrderId, rootAlgoOrderId, algoType, price, quantity = 0, extra = {}) => ({ symbol: 'PERP_BTC_USDT', algoOrderId, rootAlgoOrderId, algoType, algoStatus: 'NEW', side: 'SELL', quantity, triggerPrice: price, triggerPriceType: 'MARK_PRICE', totalExecutedQuantity: 0, reduceOnly: true, ...extra });
const pair = (root = 200, sl = 201, tp = 202, stop = 58000) => ({ symbol: 'PERP_BTC_USDT', rootAlgoOrderId: root, algoOrderId: root, algoType: 'POSITIONAL_TP_SL', algoStatus: 'NEW', childOrders: [leaf(sl, root, 'STOP_LOSS', stop), leaf(tp, root, 'TAKE_PROFIT', 65000, 0, { triggerPriceType: 'MARKET_PRICE' })] });
const regularWs = (t, row) => ws(t, PRIVATE, { topic: 'executionreport', ts: EPOCH + t, data: row });
const algoWs = (t, row) => ws(t, PRIVATE, { topic: 'algoexecutionreportv2', ts: EPOCH + t, data: row });
const touch = { success: true, timestamp: EPOCH, asks: [{ price: 60001, quantity: 1 }], bids: [{ price: 59999, quantity: 1 }] };
const algoPlaced = (ids) => ok({ rows: ids.map((orderId) => ({ orderId })) });
const algoRequest = (body) => ({ ...body, brokerId: BROKER });
const positional = (stop = 58500) => algoRequest({ reduceOnly: true, symbol: 'PERP_BTC_USDT', algoType: 'POSITIONAL_TP_SL', childOrders: [
  { reduceOnly: true, algoType: 'STOP_LOSS', type: 'CLOSE_POSITION', triggerPriceType: 'MARK_PRICE', side: 'SELL', triggerPrice: String(stop) },
  { reduceOnly: true, algoType: 'TAKE_PROFIT', type: 'CLOSE_POSITION', triggerPriceType: 'MARKET_PRICE', side: 'SELL', triggerPrice: '65000' },
] });
const trailing = (callback) => algoRequest({ symbol: 'PERP_BTC_USDT', type: 'MARKET', algoType: 'TRAILING_STOP', callbackValue: String(callback), side: 'SELL', reduceOnly: true, quantity: '0.023', triggerPriceType: 'MARKET_PRICE' });
let pendingRegular = [regular(99, 0.001, 57000)];
let pendingAlgo = [pair()];
const pendingSnapshot = () => {
  http('GET', '/v1/orders', { status: 'INCOMPLETE', page: 1 }, page(pendingRegular), undefined, stage());
  http('GET', '/v1/orders', { status: 'INCOMPLETE', page: 2 }, { ...page([]), meta: { ...meta(0), current_page: 2 } }, undefined, stage());
  http('GET', '/v3/algo/orders', { status: 'INCOMPLETE', page: 1 }, algoPage(pendingAlgo), undefined, stage());
};
const trailingRow = (id, callback) => leaf(id, id, 'TRAILING_STOP', 0, 0.023, { callbackValue: callback, extremePrice: 60000, triggerPriceType: 'MARKET_PRICE' });


http('GET', '/v1/public/info', null, page([
  { symbol: 'PERP_BTC_USDT', base_tick: 0.001, quote_tick: 0.1, base_min: 0.001, base_max: 100, base_imr: 0.02, price_range: 0.01 },
  { symbol: 'PERP_ETH_USDT', base_tick: 0.001, quote_tick: 0.01, base_min: 0.001, base_max: 1000, base_imr: 0.05, price_range: 0.01 },
]));
http('GET', '/v1/public/futures', null, page(['BTC', 'ETH'].map((base) => ({ symbol: `PERP_${base}_USDT`, '24h_open': base === 'BTC' ? 50000 : 2500, '24h_close': base === 'BTC' ? 60000 : 3000, '24h_volume': 10, '24h_amount': 600000, index_price: base === 'BTC' ? 60000 : 3000, mark_price: base === 'BTC' ? 60000 : 3000, last_funding_rate: 0.0001, next_funding_time: EPOCH + 28800000, open_interest: 1 }))));
http('GET', '/v1/public/funding_rates', null, page(['BTC', 'ETH'].map((base) => ({ symbol: `PERP_${base}_USDT`, est_funding_rate: 0.0001, next_funding_time: EPOCH + 28800000 }))));
http('GET', '/v1/public/orderbook/PERP_BTC_USDT', null, touch);
http('GET', '/v3/accountinfo', null, info());
http('GET', '/v3/positions', null, pos(), undefined, { effectiveAt: 0 });
http('GET', '/v3/positions', null, pos(0.023), undefined, { effectiveAt: 500 });
// A real two-page ordinary snapshot; algos use their own nested envelope.
http('GET', '/v1/orders', { status: 'INCOMPLETE', page: 1 }, page([regular(100)], 2, 1));
http('GET', '/v1/orders', { status: 'INCOMPLETE', page: 2 }, { ...page([regular(99, 0.001, 57000)], 2, 1), meta: { ...meta(2, 1), current_page: 2 } });
http('GET', '/v3/algo/orders', { status: 'INCOMPLETE', page: 1 }, algoPage([pair()]));
http('GET', '/v1/client/futures_leverage', { symbol: 'PERP_BTC_USDT', position_mode: 'ONE_WAY', margin_mode: 'CROSS' }, leverage());

ws(100, PRIVATE, { event: 'auth', success: true });
ws(150, PUBLIC, { event: 'pong' });
for (const [t, executed, executionId] of [[250, 0.001, 7001], [300, 0.002, 7002], [350, 0.002, 7002], [400, 0.003, 7003]]) {
  regularWs(t, regular(100, 0.003, 59000, executed, executed === 0.003 ? 'FILLED' : 'PARTIAL_FILLED', { tradeId: executionId, executedQuantity: 0.001, executedPrice: 59000, executedTimestamp: EPOCH + t }));
}
// A new camel-only terminal order cannot borrow a previously decoded type.
regularWs(525, { symbol: 'PERP_BTC_USDT', orderId: 98, orderType: 'LIMIT', orderQuantity: 0.001, orderPrice: 57000, totalExecutedQuantity: 0, status: 'CANCELLED', side: 'BUY' });
command(550, 'placeOrders', { orders: [
  { symbol: 'BTCUSDT', type: 'limit', side: 'buy', amount: 0.01, price: 59000 },
  { symbol: 'BTCUSDT', type: 'limit', side: 'buy', amount: 0.01, price: 59000, clientOrderId: '01' },
] });
command(800, 'placeOrders', { orders: [{ symbol: 'BTCUSDT', type: 'limit', side: 'buy', amount: 0.01, price: 59000, clientOrderId: '123456' }] });
pendingRegular.push(regular(101, 0.01, 59000, 0, 'NEW', { client_order_id: 123456 })); pendingSnapshot();
http('POST', '/v1/order', null, { success: true, order_id: 101 }, form({ symbol: 'PERP_BTC_USDT', order_type: 'LIMIT', order_price: 59000, side: 'BUY', order_quantity: 0.01, client_order_id: 123456, broker_id: BROKER }));
regularWs(1000, regular(101, 0.01, 59000, 0, 'NEW', { client_order_id: 123456 }));
command(1200, 'amendOrder', { id: '101', price: 59100 });
pendingRegular = pendingRegular.map((r) => r.order_id === 101 ? regular(101, 0.01, 59100, 0, 'NEW', { client_order_id: 123456 }) : r); pendingSnapshot();
http('PUT', '/v3/order/101', null, ok({ success: true, status: 'EDIT_SENT' }), { orderId: '101', price: '59100', quantity: '0.01' });
regularWs(1400, regular(101, 0.01, 59100, 0, 'REPLACED', { client_order_id: 123456 }));
command(1600, 'placeOrders', { orders: [{ symbol: 'BTCUSDT', type: 'market', side: 'buy', amount: 0.001 }] });
pendingRegular.push(regular(102, 0.001, 60540)); pendingSnapshot();
http('POST', '/v1/order', null, { success: true, order_id: 102 }, form({ symbol: 'PERP_BTC_USDT', order_type: 'IOC', order_price: 60540, side: 'BUY', order_quantity: 0.001, broker_id: BROKER }));
regularWs(1900, regular(102, 0.001, 60540));
command(2400, 'placeOrders', { orders: [{ symbol: 'BTCUSDT', type: 'stop_market', side: 'sell', amount: 0.005, price: 58000, slTriggerType: 'last_price' }] });
pendingAlgo.push(leaf(300, 300, 'STOP', 58000, 0.005, { triggerPriceType: 'MARKET_PRICE' })); pendingSnapshot();
http('POST', '/v3/algo/order', null, algoPlaced([300]), algoRequest({ symbol: 'PERP_BTC_USDT', type: 'MARKET', algoType: 'STOP', side: 'SELL', triggerPrice: '58000', triggerPriceType: 'MARKET_PRICE', reduceOnly: true, quantity: '0.005' }));
algoWs(2800, leaf(300, 300, 'STOP', 58000, 0.005, { triggerPriceType: 'MARKET_PRICE' }));
command(3200, 'placeOrders', { orders: [{ symbol: 'BTCUSDT', type: 'stop_market', side: 'sell', amount: 0, price: 58500 }] });
pendingAlgo = pendingAlgo.filter((r) => r.rootAlgoOrderId !== 200); pendingAlgo.push(pair(400, 401, 402, 58500)); pendingSnapshot();
http('DELETE', '/v3/algo/order/200', null, { success: true, status: 'CANCEL_SENT' });
http('POST', '/v3/algo/order', null, algoPlaced([401, 402, 400]), positional());
algoWs(3600, { algoOrderId: 200, algoStatus: 'CANCELLED' });
algoWs(3750, pair(400, 401, 402, 58500));
command(4200, 'placeOrders', { orders: [{ symbol: 'BTCUSDT', type: 'trailing_stop_market', side: 'sell', amount: 0, price: 59000, slTriggerType: 'last_price' }] });
pendingAlgo.push(trailingRow(500, 1000)); pendingSnapshot();
http('POST', '/v3/algo/order', null, algoPlaced([500]), trailing(1000));
algoWs(4600, leaf(500, 500, 'TRAILING_STOP', 0, 0.023, { callbackValue: 1000, extremePrice: 60000, triggerPriceType: 'MARKET_PRICE' }));
command(5000, 'amendOrder', { id: '500', price: 59200 });
pendingAlgo = pendingAlgo.filter((r) => r.algoOrderId !== 500); pendingAlgo.push(trailingRow(501, 800)); pendingSnapshot();
http('DELETE', '/v3/algo/order/500', null, { success: true, status: 'CANCEL_SENT' });
http('POST', '/v3/algo/order', null, algoPlaced([501]), trailing(800));
algoWs(5400, { algoOrderId: 500, algoStatus: 'CANCELLED' });
algoWs(5500, leaf(501, 501, 'TRAILING_STOP', 0, 0.023, { callbackValue: 800, extremePrice: 60000, triggerPriceType: 'MARKET_PRICE' }));
command(5800, 'cancelOrders', { ids: ['300', '501'] });
pendingAlgo = pendingAlgo.filter((r) => ![300, 501].includes(r.algoOrderId)); pendingSnapshot();
for (const id of [300, 501]) http('DELETE', `/v3/algo/order/${id}`, null, { success: true, status: 'CANCEL_SENT' });
for (const id of [300, 501]) algoWs(6200, { algoOrderId: id, algoStatus: 'CANCELLED' });
command(6600, 'cancelSymbolOrders', { symbol: 'BTCUSDT' });
pendingRegular = []; pendingAlgo = []; pendingSnapshot();
http('DELETE', '/v3/merge/orders/pending/PERP_BTC_USDT', null, { success: true, status: 'CANCEL_ALL_SENT' });
for (const id of [99, 101, 102]) regularWs(6900, regular(id, id === 101 ? 0.01 : 0.001, id === 101 ? 59100 : 59000, 0, 'CANCELLED'));
for (const id of [401, 402]) algoWs(6900, { algoOrderId: id, algoStatus: 'CANCELLED' });
command(7200, 'cancelSymbolOrders', { symbol: '' });
http('DELETE', '/v3/orders/pending', null, { success: true, status: 'CANCEL_SENT' });
http('DELETE', '/v3/algo/orders/pending', null, { success: true, status: 'CANCEL_SENT' });
for (const currentPage of [1, 2]) http('GET', '/v1/orders', { status: 'INCOMPLETE', page: currentPage }, { ...page([]), meta: { ...meta(0), current_page: currentPage } }, undefined, stage());
http('GET', '/v3/algo/orders', { status: 'INCOMPLETE', page: 1 }, algoPage([]), undefined, stage());
command(8000, 'setLeverage', { symbol: 'BTCUSDT', leverage: 20 });
http('POST', '/v1/client/futures_leverage', null, { success: true }, form({ symbol: 'PERP_BTC_USDT', margin_mode: 'CROSS', position_side: 'BOTH', leverage: 20 }));
http('GET', '/v1/client/futures_leverage', { symbol: 'PERP_BTC_USDT', position_mode: 'ONE_WAY', margin_mode: 'CROSS' }, leverage(20), undefined, stage());
http('GET', '/v3/positions', null, pos(0.023, 20), undefined, { ...stage(), effectiveAt: 0 });
// An external closing execution leaves mode-switch preconditions flat.
const closingPair = pair(910, 911, 912, 58000);
closingPair.childOrders[0] = { ...closingPair.childOrders[0], quantity: 0.023, algoStatus: 'FILLED', totalExecutedQuantity: 0.023, executedQuantity: 0.023, averageExecutedPrice: 58000, tradeId: 7900, executedTimestamp: EPOCH + 8600 };
closingPair.childOrders[1].algoStatus = 'CANCELLED';
// Parent remains NEW: each child's execution fields are authoritative.
algoWs(8600, closingPair);
algoWs(8700, closingPair); // Exact repeated executions emit no second fill.
http('GET', '/v3/positions', null, pos(0), undefined, { ...stage(), effectiveAt: 8600 });
command(9500, 'setPositionMode', { hedged: true });
http('POST', '/v1/client/position_mode', null, { success: true }, form({ position_mode: 'HEDGE_MODE' }));
http('GET', '/v3/accountinfo', null, info('HEDGE_MODE'), undefined, stage());
http('GET', '/v3/positions', null, pos(0), undefined, { ...stage(), effectiveAt: 0 });
http('GET', '/v3/positions', null, pos(0.01, 20), undefined, { ...stage(), effectiveAt: 11000 });

command(11500, 'fetchFills', { symbol: 'woo:BTCUSDT.P', limit: 2 });
http('GET', '/v3/trade/transactionHistory', { symbol: 'PERP_BTC_USDT', limit: 2 }, ok({ rows: [
  { id: 10001, orderId: 100, symbol: 'BTC_USDT_PERP', side: 'OPEN LONG', executedPrice: 59000, executedQuantity: 0.001, executedTimestamp: EPOCH + 250, fee: 0.01, feeAsset: 'USDT', realizedPnl: null, isMaker: 0 },
  { id: 10002, orderId: 911, symbol: 'BTC-USDT-PERP', side: 'CLOSE LONG', executedPrice: 60000, executedQuantity: 0.023, executedTimestamp: EPOCH + 8600, fee: 0.02, feeAsset: 'USDT', realizedPnl: null, isMaker: 0 },
] }));
command(12500, 'fetchOrderHistory', { symbol: 'BTCUSDT', limit: 100 });
http('GET', '/v1/orders', { symbol: 'PERP_BTC_USDT', size: 100, page: 1, realized_pnl: true }, page([
  { order_id: 100, symbol: 'PERP_BTC_USDT', side: 'BUY', type: 'LIMIT', status: 'FILLED', price: 59000, quantity: 0.003, executed: 0.003, average_executed_price: 59000, reduce_only: false, position_side: 'LONG', created_time: String(EPOCH / 1000), updated_time: String((EPOCH + 400) / 1000), realized_pnl: 0, total_fee: 0.01, fee_asset: 'USDT', total_rebate: 0 },
  { order_id: 900, symbol: 'PERP_BTC_USDT', side: 'SELL', type: 'IOC', status: 'FILLED', price: 60000, quantity: 0.023, executed: 0.023, average_executed_price: 60000, reduce_only: true, position_side: 'LONG', created_time: String((EPOCH + 8500) / 1000), updated_time: String((EPOCH + 8600) / 1000), realized_pnl: 3.2, total_fee: 0.02, fee_asset: 'USDT', total_rebate: 0 },
]));
command(13500, 'fetchIncome', { limit: 100 });
http('GET', '/v1/client/transaction_history', { size: 100, page: 1 }, ok({ rows: [
  { id: 'ledger-account-pnl', type: 'REALIZED_PNL', token: 'USDT', amount: '3.2', timestamp: EPOCH + 8600 },
  { id: 'ledger-funding', type: 'FUNDING_FEE', token: 'USDT', amount: '-0.001', timestamp: EPOCH + 1000, symbol: 'PERP_BTC_USDT' },
], meta: meta(2) }));
// Empty canonical symbols recover through the active TS alias sequence.
command(14500, 'fetchFills', { symbol: 'ETHUSDT', limit: 2 });
http('GET', '/v3/trade/transactionHistory', { symbol: 'PERP_ETH_USDT', limit: 2 }, { success: true, rows: [] });
http('GET', '/v3/trade/transactionHistory', { symbol: 'ETH_USDT_PERP', limit: 2 }, { success: true, rows: [
  { orderId: 920, symbol: 'ETH_USDT_PERP', side: 'BUY', executedPrice: '3000', executedQuantity: '0.01', executedTimestamp: String((EPOCH + 14000) / 1000) },
] });
// The legacy aggregate schema has an order ID but no per-execution ID.
command(15000, 'fetchFills', { symbol: 'BTCUSDT', limit: 3 });
for (const symbol of ['PERP_BTC_USDT', 'BTC_USDT_PERP', 'BTC-USDT-PERP', 'BTCUSDT', undefined])
  http('GET', '/v3/trade/transactionHistory', { ...(symbol ? { symbol } : {}), limit: 3 }, { success: true, rows: [] });
http('GET', '/v1/client/trades', { symbol: 'PERP_BTC_USDT', size: 3 }, { success: true, rows: [
  { order_id: 930, symbol: 'PERP_BTC_USDT', side: 'BUY', average_executed_price: '62000', executed: '0.002', created_time: String((EPOCH + 14500) / 1000), total_fee: '0.01', fee_asset: 'USDT' },
], meta: meta(1, 3) });
// Entry and its native paired protections retain the closing hedge rail.
command(17000, 'placeOrders', { orders: [{ symbol: 'BTCUSDT', type: 'limit', side: 'buy', amount: 0.01, price: 59000, stopLoss: 58000, takeProfit: 65000, tpTriggerType: 'last_price' }] });
pendingRegular = [regular(800, 0.01, 59000, 0, 'NEW', { position_side: 'LONG' })];
pendingAlgo = [pair(801, 802, 803, 58000)]; pendingSnapshot();
http('POST', '/v1/order', null, { success: true, order_id: 800 }, form({ symbol: 'PERP_BTC_USDT', order_type: 'LIMIT', order_price: 59000, side: 'BUY', position_side: 'LONG', order_quantity: 0.01, broker_id: BROKER }));
const hedgedProtection = positional(58000);
hedgedProtection.positionSide = 'LONG';
for (const child of hedgedProtection.childOrders) child.positionSide = 'LONG';
http('POST', '/v3/algo/order', null, algoPlaced([802, 803, 801]), hedgedProtection);
regularWs(17400, regular(800, 0.01, 59000, 0, 'NEW', { position_side: 'LONG' }));
algoWs(17500, pair(801, 802, 803, 58000));
command(18500, 'cancelSymbolOrders', { symbol: 'BTCUSDT' });
pendingRegular = []; pendingAlgo = []; pendingSnapshot();
http('DELETE', '/v3/merge/orders/pending/PERP_BTC_USDT', null, { success: true, status: 'CANCEL_ALL_SENT' });
regularWs(18900, regular(800, 0.01, 59000, 0, 'CANCELLED'));
// Full terminal leaves remain self-contained after the server's empty read.
for (const child of pair(801, 802, 803, 58000).childOrders)
  algoWs(18900, { ...child, positionSide: 'LONG', algoStatus: 'CANCELLED' });
for (const t of [10000, 16000, 22000]) ws(t, PUBLIC, { event: 'ping' });
for (const t of [10000, 12000, 14000, 16000, 18000, 20000, 22000]) ws(t, PRIVATE, { event: 'pong' });
records.sort((a, b) => a.t - b.t);
mkdirSync(dirname(values.out), { recursive: true });
writeFileSync(values.out, `${records.map((r) => JSON.stringify(r)).join('\n')}\n`);
console.log(JSON.stringify({ out: values.out, records: records.length, commands: records.filter((r) => r.type === 'command').length }));
