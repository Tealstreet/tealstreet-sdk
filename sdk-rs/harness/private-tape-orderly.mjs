// Replay-only Orderly omnichain examples, shared by its six broker frontends.
// Schemas: https://github.com/OrderlyNetwork/documentation-public/blob/main/orderly.openapi.yaml
// Algo lifecycle: https://orderly.network/docs/build-on-omnichain/user-flows/algo-order-samples
// No network requests or production exchange imports are used by this generator.
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname } from 'node:path';
import { parseArgs } from 'node:util';
import { ORDERLY_REPLAY } from './venues.mjs';

const { values } = parseArgs({ options: {
  out: { type: 'string' },
  venue: { type: 'string', default: 'orderly' },
  testnet: { type: 'boolean', default: false },
} });
const tags = {
  orderly: 'TEALSTREET',
  'tealstreet-dex': 'TEALSTREET',
  kodiakfi: 'TEALKODIAK',
  woofi: 'TEALSTREET',
  mode: 'TEALMODE',
  salsadex: 'TEALSALSA',
  what: 'TEALWHAT',
};
if (!Object.hasOwn(tags, values.venue)) throw new Error('Expected an Orderly broker venue');
const out = values.out ?? `sdk-rs/harness/tapes/${values.venue}${values.testnet ? '-testnet' : ''}-private.jsonl`;
const REST = values.testnet ? 'https://testnet-api-evm.orderly.org' : 'https://api-evm.orderly.org';
const PUBLIC = `wss://${values.testnet ? 'testnet-' : ''}ws-evm.orderly.org/ws/stream/${ORDERLY_REPLAY.applicationId}`;
const PRIVATE = `wss://${values.testnet ? 'testnet-' : ''}ws-private-evm.orderly.org/v2/ws/private/stream/${ORDERLY_REPLAY.applicationId}`;
const BROKER = values.testnet ? 'e708a644-9ce5-46c6-b50c-74b15f62d8ca' : 'ab82cb09-cfec-4473-80a3-b740779d0644';
const BTC = 'PERP_BTC_USDC';
const ETH = 'PERP_ETH_USDC';
const EPOCH = 1765805280000;
const records = [{ t: 0, type: 'meta', private: true, epoch: EPOCH, venue: values.venue, testnet: values.testnet }];
let clock = 0;
let commandCount = 0;
const ok = (data) => ({ success: true, data, timestamp: EPOCH });
const http = (method, path, data, requestBody, metadata = {}) => records.push({
  t: 0, type: 'http', method, url: REST + path, status: 200,
  body: JSON.stringify(ok(data)),
  ...(requestBody === undefined ? {} : { requestBody: JSON.stringify(requestBody) }),
  ...(method === 'GET' ? {} : { afterCommand: commandCount }),
  ...metadata,
});
const rows = (data) => ({ rows: data, meta: { total: data.length, records_per_page: 500, current_page: 1 } });
const stage = (path, data, afterCommand = 0) => http('GET', path, rows(data), undefined, { afterCommand });
// TS requests no explicit page; Rust requests the documented maximum page size.
const openOrders = (data, afterCommand = 0, algo = false) => {
  const path = `/v1/${algo ? 'algo/' : ''}orders`;
  for (const query of ['status=INCOMPLETE', 'status=INCOMPLETE&page=1&size=500'])
    stage(`${path}?${query}`, data, afterCommand);
};
const ws = (url, data) => records.push({ t: clock += 100, type: 'ws', url, text: JSON.stringify(data) });
const push = (topic, data) => ws(PRIVATE, { topic, ts: EPOCH + clock, data });
const command = (data) => {
  records.push({ t: clock += 100, type: 'command', ...data });
  return ++commandCount;
};
const market = (symbol, baseMax, quoteTick, baseImr) => ({
  symbol, base_tick: 0.001, quote_tick: quoteTick, base_min: 0.001, base_max: baseMax,
  base_imr: baseImr, base_mmr: baseImr / 2, imr_factor: 0.0000002512,
  quote_min: 0, quote_max: 1000000, min_notional: 1, price_range: 0.1,
  created_time: EPOCH - 86400000, updated_time: EPOCH,
});
http('GET', '/v1/public/info', { rows: [market(BTC, 20, 0.1, 0.01), market(ETH, 100, 0.01, 0.02)] });
http('GET', '/v1/public/futures', { rows: [
  { symbol: BTC, '24h_open': 50000, '24h_close': 51000, '24h_volume': 10, '24h_amount': 505000,
    mark_price: 51000, index_price: 50999, open_interest: 500, last_funding_rate: 0.0001, next_funding_time: EPOCH + 3600000 },
  { symbol: ETH, '24h_open': 2000, '24h_close': 2100, '24h_volume': 100, '24h_amount': 205000,
    mark_price: 2100, index_price: 2099, open_interest: 1000, last_funding_rate: -0.0001, next_funding_time: EPOCH + 3600000 },
] });
http('GET', '/v1/client/info', { account_id: ORDERLY_REPLAY.applicationId, account_mode: 'FUTURES',
  max_leverage: 20, futures_taker_fee_rate: 0.0005, futures_maker_fee_rate: 0.0002 });
http('GET', '/v1/client/margin_modes', { rows: [
  { symbol: BTC, default_margin_mode: 'CROSS' }, { symbol: ETH, default_margin_mode: 'ISOLATED' },
] });
const position = (symbol, quantity, entry, mode, leverage, extra = {}) => ({
  symbol, position_qty: quantity, average_open_price: entry, mark_price: symbol === BTC ? 51000 : 2100,
  est_liq_price: symbol === BTC ? 40000 : 1500, margin_mode: mode, leverage,
  margin: Math.abs(quantity * entry / leverage), imr: 1 / leverage,
  last_sum_unitary_funding: 0, timestamp: EPOCH, ...extra,
});
const positions = [position(BTC, 1, 50000, 'CROSS', 10), position(BTC, -0.5, 52000, 'ISOLATED', 5)];
http('GET', '/v1/positions', { rows: positions, total_collateral_value: 10000, free_collateral: 7000 });
const order = (id, symbol, side, quantity, price, extra = {}) => ({
  order_id: id, symbol, type: 'LIMIT', side, status: 'NEW', price, quantity,
  total_executed_quantity: 0, executed: 0, executed_quantity: 0, visible_quantity: quantity,
  reduce_only: false, margin_mode: 'CROSS', client_order_id: `resting-${id}`,
  created_time: EPOCH, updated_time: EPOCH, ...extra,
});
const resting = order(11, BTC, 'BUY', 1, 50000);
const filling = order(21, ETH, 'BUY', 2, 2000);
openOrders([resting, filling]);
const child = (id, parent, kind, price, symbol = BTC, extra = {}) => ({
  algo_order_id: id, root_algo_order_id: parent, symbol, algo_type: kind, algo_status: 'NEW',
  side: 'SELL', type: 'CLOSE_POSITION', trigger_price: price, trigger_price_type: 'MARK_PRICE',
  quantity: 0, total_executed_quantity: 0, reduce_only: true, margin_mode: 'CROSS', ...extra,
});
const pair = (id, children, symbol = BTC, extra = {}) => ({
  algo_order_id: id, root_algo_order_id: id, symbol, algo_type: 'POSITIONAL_TP_SL',
  algo_status: 'NEW', margin_mode: 'CROSS', child_orders: children, ...extra,
});
const originalPair = pair(100, [child(102, 100, 'TAKE_PROFIT', 55000)]);
openOrders([originalPair], 0, true);

ws(PRIVATE, { event: 'auth', success: true });
ws(PUBLIC, { event: 'ping' });
const execution = (id, symbol, side, quantity, price, extra = {}) => ({
  orderId: id, symbol, type: 'LIMIT', side, status: 'NEW', price, quantity,
  totalExecutedQuantity: 0, executedQuantity: 0, reduceOnly: false, marginMode: 'CROSS',
  clientOrderId: `resting-${id}`, timestamp: EPOCH + clock, seq: 1, ...extra,
});
const partial = execution(21, ETH, 'BUY', 2, 2000, { status: 'PARTIAL_FILLED',
  totalExecutedQuantity: 0.5, executedQuantity: 0.5, executedPrice: 2001, tradeId: 7001 });
push('executionreport', partial);
push('executionreport', partial); // One execution, delivered twice.
push('executionreport', execution(21, ETH, 'BUY', 2, 2000, { status: 'FILLED',
  totalExecutedQuantity: 2, executedQuantity: 1.5, executedPrice: 2002, tradeId: 7002, seq: 2 }));
push('position', { positions: [{ symbol: BTC, positionQty: 1.2, averageOpenPrice: 50100,
  markPrice: 51000, marginMode: 'CROSS', leverage: 10, imr: 0.1, timestamp: EPOCH + clock }] });

const entryBody = (symbol, side, type, quantity, price, extra = {}) => ({
  symbol, order_type: type, ...(price === undefined ? {} : { order_price: price }), side,
  margin_mode: symbol === ETH ? 'ISOLATED' : 'CROSS', order_tag: tags[values.venue],
  order_quantity: quantity, broker_id: BROKER, ...extra,
});
const tifCommand = command({ op: 'placeOrders', orders: [{ symbol: 'BTCUSDC', type: 'limit',
  side: 'buy', amount: 0.25, price: 49900, clientOrderId: 'OrderlyCaller-30', timeInForce: 'PostOnly' }] });
const tifBody = entryBody(BTC, 'BUY', 'POST_ONLY', 0.25, 49900, { client_order_id: 'OrderlyCaller-30' });
http('POST', '/v1/order', { order_id: 30, client_order_id: 'OrderlyCaller-30' }, tifBody);
records.at(-1).tsRequestBody = JSON.stringify({ ...tifBody, order_type: 'LIMIT' });
push('executionreport', execution(30, BTC, 'BUY', 0.25, 49900, { type: 'POST_ONLY', clientOrderId: 'OrderlyCaller-30' }));
const resting30 = order(30, BTC, 'BUY', 0.25, 49900, { type: 'POST_ONLY', client_order_id: 'OrderlyCaller-30' });
openOrders([resting, resting30], tifCommand);

const amendCommand = command({ op: 'amendOrder', id: '11', price: 50100 });
http('PUT', '/v1/order', { status: 'EDIT_SENT' }, { order_id: '11', symbol: BTC,
  order_type: 'LIMIT', side: 'BUY', order_price: 50100 });
const amended = order(11, BTC, 'BUY', 1, 50100);
openOrders([amended, resting30], amendCommand);
push('executionreport', execution(11, BTC, 'BUY', 1, 50100, { status: 'REPLACED', seq: 2 }));

const positionalBody = (symbol, stop, take, marginMode = 'CROSS') => ({
  reduce_only: true, symbol, algo_type: 'POSITIONAL_TP_SL', margin_mode: marginMode,
  child_orders: [
    ...(stop === undefined ? [] : [{ reduce_only: true, algo_type: 'STOP_LOSS', type: 'CLOSE_POSITION', side: 'SELL', trigger_price: String(stop) }]),
    ...(take === undefined ? [] : [{ reduce_only: true, algo_type: 'TAKE_PROFIT', type: 'CLOSE_POSITION', side: 'SELL', trigger_price: String(take) }]),
  ], brokerId: BROKER,
});
const replacementCommand = command({ op: 'placeOrders', orders: [{ symbol: 'BTCUSDC', type: 'stop_market',
  side: 'sell', amount: 1, price: 48000, reduceOnly: true }] });
http('DELETE', `/v1/algo/order?order_id=100&symbol=${BTC}`, { status: 'CANCELLED' });
http('POST', '/v1/algo/order', { order_id: 200, rows: [{ orderId: 201 }, { orderId: 202 }] },
  positionalBody(BTC, 48000, 55000));
const replacement = pair(200, [child(201, 200, 'STOP_LOSS', 48000), child(202, 200, 'TAKE_PROFIT', 55000)]);
openOrders([replacement], replacementCommand, true);
push('algoexecutionreportv2', pair(100, [], BTC, { algo_status: 'CANCELLED' }));
push('algoexecutionreportv2', replacement);

const childCancel = command({ op: 'cancelOrders', ids: ['201'] });
http('DELETE', `/v1/algo/order?order_id=200&symbol=${BTC}`, { status: 'CANCELLED' });
http('POST', '/v1/algo/order', { order_id: 300, rows: [{ orderId: 302 }] }, positionalBody(BTC, undefined, 55000));
const surviving = pair(300, [child(302, 300, 'TAKE_PROFIT', 55000)]);
openOrders([surviving], childCancel, true);
push('algoexecutionreportv2', pair(200, [], BTC, { algo_status: 'CANCELLED' }));
push('algoexecutionreportv2', surviving);

const bracketCommand = command({ op: 'placeOrders', orders: [{ symbol: 'ETHUSDC', type: 'market',
  side: 'buy', amount: 2, stopLoss: 1900, takeProfit: 2300 }] });
http('POST', '/v1/order', { order_id: 40 }, entryBody(ETH, 'BUY', 'MARKET', 2));
http('POST', '/v1/algo/order', { order_id: 400, rows: [{ orderId: 401 }, { orderId: 402 }] },
  positionalBody(ETH, 1900, 2300, 'ISOLATED'));
const bracket = pair(400, [child(401, 400, 'STOP_LOSS', 1900, ETH, { margin_mode: 'ISOLATED' }),
  child(402, 400, 'TAKE_PROFIT', 2300, ETH, { margin_mode: 'ISOLATED' })], ETH, { margin_mode: 'ISOLATED' });
const entry40 = order(40, ETH, 'BUY', 2, 0, { type: 'MARKET', margin_mode: 'ISOLATED' });
openOrders([amended, resting30, entry40], bracketCommand);
openOrders([surviving, bracket], bracketCommand, true);
push('executionreport', execution(40, ETH, 'BUY', 2, 0, { type: 'MARKET', marginMode: 'ISOLATED' }));
push('algoexecutionreportv2', bracket);

const bulkCommand = command({ op: 'cancelSymbolOrders', symbol: 'BTCUSDC' });
http('DELETE', `/v1/orders?symbol=${BTC}`, { status: 'CANCEL_ALL_SENT' });
http('DELETE', `/v1/algo/orders?symbol=${BTC}`, { status: 'CANCEL_ALL_SENT' });
openOrders([entry40], bulkCommand);
openOrders([bracket], bulkCommand, true);
push('executionreport', execution(11, BTC, 'BUY', 1, 50100, { status: 'CANCELLED', seq: 3 }));
push('executionreport', execution(30, BTC, 'BUY', 0.25, 49900, { type: 'POST_ONLY', status: 'CANCELLED', clientOrderId: 'OrderlyCaller-30', seq: 2 }));
push('algoexecutionreportv2', pair(300, [], BTC, { algo_status: 'CANCELLED' }));
command({ op: 'setLeverage', symbol: 'BTCUSDC', leverage: 12 });
http('POST', '/v1/client/leverages', { max_leverage: 12 }, { leverage: 12, symbol: BTC, margin_mode: 'CROSS' });
command({ op: 'setMarginType', symbol: 'ETHUSDC', isolated: false });
http('POST', '/v1/client/margin_mode', {}, { symbol: ETH, default_margin_mode: 'CROSS' });
command({ op: 'fetchLeverage', symbol: 'BTCUSDC' });
http('GET', `/v1/client/leverage?symbol=${BTC}&margin_mode=CROSS`, { max_leverage: 12, margin_mode: 'CROSS' });

// Query fixtures are documented venue data, independent of either implementation.
const from = EPOCH - 1000;
const to = EPOCH + 60000;
command({ op: 'fetchFills', symbol: 'BTCUSDC', from, to, limit: 2 });
const fillRows = [
  { id: 8001, symbol: BTC, order_id: 11, side: 'BUY', executed_price: 50000, executed_quantity: 0.1,
    executed_timestamp: EPOCH + 1, fee: 0.5, fee_asset: 'USDC', is_maker: 1, realized_pnl: 0 },
  { id: 8002, symbol: BTC, order_id: 11, side: 'BUY', executed_price: 50001, executed_quantity: 0.2,
    executed_timestamp: EPOCH + 2, fee: 1.00002, fee_asset: 'USDC', is_maker: 0, realized_pnl: 0 },
];
for (const size of [2, 500]) stage(`/v1/trades?symbol=${BTC}&start_t=${from}&end_t=${to}&page=1&size=${size}`, fillRows);
command({ op: 'fetchOrderHistory', symbol: 'BTCUSDC', from, to, limit: 2 });
const historyRows = [order(80, BTC, 'BUY', 1, 50000, { status: 'FILLED', total_executed_quantity: 1,
  executed: 1, average_executed_price: 50000 }), order(81, BTC, 'SELL', 0.5, 55000, { status: 'CANCELLED' })];
for (const size of [2, 500]) stage(`/v1/orders?symbol=${BTC}&start_t=${from}&end_t=${to}&page=1&size=${size}`, historyRows);
stage(`/v1/algo/orders?symbol=${BTC}&start_t=${from}&end_t=${to}&page=1&size=500`, []);
command({ op: 'fetchIncome', symbol: 'BTCUSDC', incomeType: 'FUNDING_FEE', from, to, limit: 2 });
const fundingRows = [
  { symbol: BTC, funding_rate: 0.0001, mark_price: 51000, funding_fee: 0.51, payment_type: 'Pay', status: 'Settled', created_time: EPOCH + 3 },
  { symbol: BTC, funding_rate: -0.0001, mark_price: 51000, funding_fee: 0.255, payment_type: 'Receive', status: 'Settled', created_time: EPOCH + 4 },
];
for (const size of [2, 60, 500]) stage(`/v1/funding_fee/history?symbol=${BTC}&start_t=${from}&end_t=${to}&page=1&size=${size}`, fundingRows);
command({ op: 'fetchPublicTrades', symbol: 'BTCUSDC', limit: 2 });
http('GET', `/v1/public/market_trades?symbol=${BTC}&limit=2`, { rows: [
  { symbol: BTC, side: 'BUY', executed_price: 51000, executed_quantity: 0.1, executed_timestamp: EPOCH + 5 },
  { symbol: BTC, side: 'SELL', executed_price: 50999.9, executed_quantity: 0.2, executed_timestamp: EPOCH + 6 },
] });
command({ op: 'fetchCandles', symbol: 'BTCUSDC', interval: '1m', limit: 2 });
http('GET', `/v1/kline?symbol=${BTC}&type=1m&limit=2`, { rows: [
  { start_timestamp: EPOCH, end_timestamp: EPOCH + 59999, open: 50000, high: 51000, low: 49900, close: 50500, volume: 10 },
  { start_timestamp: EPOCH - 60000, end_timestamp: EPOCH - 1, open: 49900, high: 50100, low: 49800, close: 50000, volume: 5 },
] });
push('position', { positions: [
  { symbol: BTC, positionQty: 1.2, averageOpenPrice: 50100, markPrice: 51000, marginMode: 'CROSS', leverage: 12, imr: 1 / 12, timestamp: EPOCH + 10000 },
  { symbol: BTC, positionQty: -0.5, averageOpenPrice: 52000, markPrice: 51000, marginMode: 'ISOLATED', leverage: 5, margin: 5200, timestamp: EPOCH + 10000 },
] });
ws(PRIVATE, { event: 'ping' });
mkdirSync(dirname(out), { recursive: true });
records.sort((a, b) => a.t - b.t);
writeFileSync(out, records.map((record) => JSON.stringify(record)).join('\n') + '\n');
console.log(JSON.stringify({ out, venue: values.venue, testnet: values.testnet, records: records.length,
  commands: commandCount, seconds: clock / 1000 }));
