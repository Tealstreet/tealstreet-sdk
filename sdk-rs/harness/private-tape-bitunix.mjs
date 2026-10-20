// Deterministic documented-payload Bitunix futures account tape. No live keys.
// Sources: bitunix.com/api-docs/futures/{account,position,trade,tp_sl,websocket}/
// and exchanges/bitunix/CLAUDE.md's verified legacy payload variants.
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname } from 'node:path';
import { parseArgs } from 'node:util';
const { values } = parseArgs({ options: { out: { type: 'string', default: 'sdk-rs/harness/tapes/bitunix-private.jsonl' } } });
const REST = 'https://fapi.bitunix.com';
const PUBLIC = 'wss://fapi.bitunix.com/public/';
const PRIVATE = 'wss://fapi.bitunix.com/private/';
const FUTURES = 'wss://api.bitunix.com/ws-futures/?from=contract&compress=2';
const records = [
  { t: 0, type: 'connect', url: PRIVATE, connection: 0, initial: true },
  { t: 0, type: 'open', url: PRIVATE, connection: 0 },
];
const http = (method, path, query, data, requestBody, after) => records.push({ t: 0, type: 'http', method, url: `${REST}/api/v1/futures/${path}${query ? `?${query}` : ''}`, requestBody, availableAfter: after, status: 200, body: JSON.stringify({ code: 0, data, msg: 'Success' }) });
const read = (path, query, data, after = 0) => http('GET', path, query, data, undefined, after);
let t = 0;
let privateConnection = 0;
const ws = (url, data) => records.push({ t: (t += 400), type: 'ws', url, ...(url === PRIVATE ? { connection: privateConnection } : {}), text: JSON.stringify(data) });
const command = (body) => records.push({ t: (t += 400), type: 'command', ...body });
const pair = (symbol, base, quotePrecision) => ({ symbol, base, quote: 'USDT', symbolStatus: 'OPEN', basePrecision: 3, quotePrecision, maxLimitOrderVolume: '100', isApiSupported: true });
read('market/trading_pairs', '', [pair('BTCUSDT', 'BTC', 1), pair('ETHUSDT', 'ETH', 2), { ...pair('NOUSDT', 'NO', 2), isApiSupported: false }]);
read('market/tickers', '', [
  { symbol: 'BTCUSDT', last: '96000', open: '95000', markPrice: '96001', baseVol: '1000', quoteVol: '96000000' },
  { symbol: 'ETHUSDT', last: '3000', open: '3000', markPrice: '3000', baseVol: '50000', quoteVol: '150000000' },
]);
const account = (available = '4952', margin = '48', pnl = '10') => [{ marginCoin: 'USDT', available, margin, frozen: '1', crossUnrealizedPNL: pnl, isolationUnrealizedPNL: '0', positionMode: 'HEDGE' }];
read('account', 'marginCoin=USDT', account());
const position = (leverage = 20, marginMode = 'CROSS', realizedPNL = '-3.5') => ({ symbol: 'BTCUSDT', positionId: 'position-1', qty: '0.01', maxQty: '0.02', avgOpenPrice: '95000', entryValue: '950', side: 'BUY', marginMode, positionMode: 'HEDGE', leverage, unrealizedPNL: '10', realizedPNL, fee: '2', funding: realizedPNL === '-3.5' ? '-1.5' : '-2.25', liqPrice: '80000', ctime: 1700000000000, mtime: 1700000000000 });
read('position/get_pending_positions', '', { positionList: [position()] });
const regular = (orderId, symbol, side, qty, price, clientId, tradeQty = '0') => ({ orderId, symbol, side, qty, price, clientId, tradeQty, orderType: 'LIMIT', effect: 'GTC', reduceOnly: false, mtime: 1700000000000 });
const resting = regular('111', 'BTCUSDT', 'BUY', '0.004', '90000', 'resting', '0.001');
read('trade/get_pending_orders', '', { orderList: [resting] });
const protection = { id: 'protect', positionId: 'position-1', symbol: 'BTCUSDT', side: 'SELL', tpPrice: '100000', slPrice: '85000', tpQty: '0.002', slQty: '0.002', tpStopType: 'LAST_PRICE', slStopType: 'MARK_PRICE', tpOrderType: null, slOrderType: 'MARKET', tpOrderPrice: null, slOrderPrice: null, reduceOnly: true };
read('tpsl/get_pending_orders', '', [protection]);
const orderEvent = (event, data) => ({ ch: 'order', ts: 1700000001000 + t, data: { event, positionMode: 'HEDGE', mtime: '2023-11-14T22:13:21.000Z', ...data } });
ws(PUBLIC, { ch: 'ticker', symbol: 'BTCUSDT', data: { la: '96000', o: '95000', b: '1000', q: '96000000' } });
ws(PRIVATE, { op: 'login', data: { result: true } });
ws(PRIVATE, { ch: 'position', data: { event: 'UPDATE', ...position() } });
const execution = { symbol: 'ETHUSDT', orderId: '1001', side: 'Buy', type: 'Limit', qty: '1', price: '3000', clientId: 'web', effect: 'POST_ONLY' };
ws(PRIVATE, orderEvent('CREATE', { ...execution, dealAmount: '0', averagePrice: '0', orderStatus: 'NEW' }));
ws(PRIVATE, orderEvent('UPDATE', { ...execution, dealAmount: '0.4', averagePrice: '2999.5', orderStatus: 'PART_FILLED' }));
ws(PRIVATE, orderEvent('CLOSE', { ...execution, dealAmount: '1', averagePrice: '2999.2', orderStatus: 'FILLED' }));
const transient = { orderId: 'second', positionId: 'position-1', symbol: 'BTCUSDT', side: 'Sell', type: 'TPSL', tpPrice: '100200', tpQty: '0.001', tpStopType: 'Last' };
ws(PRIVATE, { ch: 'tpsl', data: { event: 'CREATE', ...transient } });
ws(PRIVATE, { ch: 'tpsl', data: { event: 'UPDATE', ...transient, tpPrice: '100500' } });
// The terminal snapshot carries the last amended trigger price.
ws(PRIVATE, { ch: 'tpsl', data: { event: 'CLOSE', ...transient, tpPrice: '100500', status: 'CANCELED' } });
command({ op: 'placeOrders', orders: [{ symbol: 'BTCUSDT', type: 'limit', side: 'buy', amount: 0.0021, price: 90500.04, clientOrderId: 'caller-limit' }] });
http('POST', 'trade/place_order', '', { orderId: '1003', clientId: 'caller-limit' }, { symbol: 'BTCUSDT', qty: '0.002', price: '90500', side: 'BUY', tradeSide: 'OPEN', orderType: 'LIMIT', effect: 'GTC', clientId: 'caller-limit', reduceOnly: false });
const placed = { symbol: 'BTCUSDT', orderId: '1003', side: 'Buy', type: 'Limit', qty: '0.002', price: '90500', clientId: 'caller-limit', effect: 'GTC' };
ws(PRIVATE, orderEvent('CREATE', { ...placed, dealAmount: '0', orderStatus: 'NEW' }));
read('trade/get_pending_orders', '', { orderList: [resting, regular('1003', 'BTCUSDT', 'BUY', '0.002', '90500', 'caller-limit')] }, t);
command({ op: 'amendOrder', id: '1003', price: 90600.04, amount: 0.0031 });
http('POST', 'trade/modify_order', '', { orderId: '1003' }, { orderId: '1003', symbol: 'BTCUSDT', qty: '0.003', price: '90600' });
ws(PRIVATE, orderEvent('UPDATE', { ...placed, qty: '0.003', price: '90600', dealAmount: '0', orderStatus: 'NEW' }));
read('trade/get_pending_orders', '', { orderList: [resting, regular('1003', 'BTCUSDT', 'BUY', '0.003', '90600', 'caller-limit')] }, t);
command({ op: 'cancelOrders', ids: ['1003'] });
http('POST', 'trade/cancel_orders', '', { successList: [{ orderId: '1003' }], failureList: [] }, { symbol: 'BTCUSDT', orderList: [{ orderId: '1003' }] });
read('trade/get_pending_orders', '', { orderList: [resting] }, t);
ws(PRIVATE, orderEvent('CLOSE', { ...placed, qty: '0.003', price: '90600', dealAmount: '0', orderStatus: 'CANCELED' }));
command({ op: 'placeOrders', orders: [
  { symbol: 'ETHUSDT', type: 'limit', side: 'buy', amount: 0.2, price: 2800, clientOrderId: 'caller-batch-a' },
  { symbol: 'ETHUSDT', type: 'limit', side: 'buy', amount: 0.3, price: 2810, clientOrderId: 'caller-batch-b' },
] });
const batchLeg = (qty, price, clientId) => ({ qty, price, side: 'BUY', tradeSide: 'OPEN', orderType: 'LIMIT', effect: 'GTC', clientId, reduceOnly: false });
http('POST', 'trade/batch_order', '', { successList: [{ orderId: '1004', clientId: 'caller-batch-a' }, { orderId: '1005', clientId: 'caller-batch-b' }], failureList: [] }, { symbol: 'ETHUSDT', orderList: [batchLeg('0.2', '2800', 'caller-batch-a'), batchLeg('0.3', '2810', 'caller-batch-b')] });
const batchA = { symbol: 'ETHUSDT', orderId: '1004', side: 'Buy', type: 'Limit', qty: '0.2', price: '2800', clientId: 'caller-batch-a', effect: 'GTC', dealAmount: '0' };
const batchB = { ...batchA, orderId: '1005', qty: '0.3', price: '2810', clientId: 'caller-batch-b' };
ws(PRIVATE, orderEvent('CREATE', batchA));
read('trade/get_pending_orders', '', { orderList: [resting, regular('1004', 'ETHUSDT', 'BUY', '0.2', '2800', 'caller-batch-a')] }, t);
ws(PRIVATE, orderEvent('CREATE', batchB));
read('trade/get_pending_orders', '', { orderList: [resting, regular('1004', 'ETHUSDT', 'BUY', '0.2', '2800', 'caller-batch-a'), regular('1005', 'ETHUSDT', 'BUY', '0.3', '2810', 'caller-batch-b')] }, t);
// Let the prior terminal-order reconciliation finish before cancel-all.
t += 1200;
command({ op: 'cancelSymbolOrders', symbol: 'ETHUSDT' });
http('POST', 'trade/cancel_orders', '', { successList: [{ orderId: '1004' }, { orderId: '1005' }], failureList: [] }, { symbol: 'ETHUSDT', orderList: [{ orderId: '1004' }, { orderId: '1005' }] });
read('trade/get_pending_orders', '', { orderList: [resting] }, t);
ws(PRIVATE, orderEvent('CLOSE', { ...batchA, orderStatus: 'CANCELED' }));
ws(PRIVATE, orderEvent('CLOSE', { ...batchB, orderStatus: 'CANCELED' }));
command({ op: 'setLeverage', symbol: 'BTCUSDT', leverage: 30 });
http('POST', 'account/change_leverage', '', {}, { marginCoin: 'USDT', symbol: 'BTCUSDT', leverage: 30 });
read('position/get_pending_positions', '', { positionList: [position(30)] }, t);
command({ op: 'setMarginType', symbol: 'BTCUSDT', isolated: true });
http('POST', 'account/change_margin_mode', '', {}, { symbol: 'BTCUSDT', marginMode: 'ISOLATION', marginCoin: 'USDT' });
read('position/get_pending_positions', '', { positionList: [position(30, 'ISOLATION')] }, t);
ws(PRIVATE, { ch: 'position', data: { event: 'UPDATE', ...position(30, 'ISOLATION') } });
ws(PRIVATE, { ch: 'balance', data: { coin: 'USDT', available: '4900' } });
read('account', 'marginCoin=USDT', account('4900', '100', '12'), t);
command({ op: 'fetchFills', symbol: 'BTCUSDT', orderId: '111', from: 1700000000, to: 1700000100, limit: 100 });
read('trade/get_history_trades', 'symbol=BTCUSDT&orderId=111&startTime=1700000000000&endTime=1700000100000&limit=100', { tradeList: [
  { tradeId: 'fill-1', orderId: '111', symbol: 'BTCUSDT', side: 'BUY', price: '90000', qty: '0.0004', fee: '0.01', marginCoin: 'USDT', ctime: 1700000001000 },
  { tradeId: 'fill-2', orderId: '111', symbol: 'BTCUSDT', side: 'BUY', price: '89999', qty: '0.0006', fee: '0.02', marginCoin: 'USDT', ctime: 1700000002000 },
  { tradeId: 'fill-3', orderId: '111', symbol: 'BTCUSDT', side: 'BUY', price: '89999', qty: '0.0006', fee: '0.02', marginCoin: 'USDT', ctime: 1700000002000 },
] });
command({ op: 'fetchOrderHistory', symbol: 'BTCUSDT', from: 1700000000000, to: 1700000100000, limit: 100 });
read('trade/get_history_orders', 'symbol=BTCUSDT&startTime=1700000000000&endTime=1700000100000&limit=100', { orderList: [
  { orderId: 'closed-1', symbol: 'BTCUSDT', side: 'BUY', orderType: 'LIMIT', status: 'FILLED', qty: '0.001', tradeQty: '0.001', price: '90000', avgPrice: '89999', ctime: 1700000001000, mtime: 1700000002000 },
  { orderId: 'closed-2', symbol: 'BTCUSDT', side: 'SELL', orderType: 'MARKET', status: 'CANCELED', qty: '0.001', tradeQty: '0', price: 'MARKET', avgPrice: '0', ctime: 1700000003000, mtime: 1700000004000 },
] });
command({ op: 'fetchIncome', symbol: 'BTCUSDT', from: 1700000000000, to: 1700000100000 });
read('trade/get_history_trades', 'symbol=BTCUSDT&startTime=1700000000000&endTime=1700000100000&limit=100', { tradeList: [
  { tradeId: 'ledger-1', symbol: 'BTCUSDT', ctime: 1700000001000, realizedPNL: '-3.5', fee: '2', marginCoin: 'USDT' },
  { tradeId: 'ledger-2', symbol: 'BTCUSDT', ctime: 1700000002000, realizedPNL: '0', fee: '-0.1', marginCoin: 'USDT' },
] });
command({ op: 'fetchPublicTrades', symbol: 'BTCUSDT' });
command({ op: 'fetchCandles', symbol: 'BTCUSDT', interval: '1m', from: 1700000000000, to: 1700000100000, limit: 2 });
read('market/kline', 'symbol=BTCUSDT&interval=1m&startTime=1699999980000&endTime=1700000100000&limit=2', [
  { time: 1700000040000, open: '96000', high: '96010', low: '95990', close: '96005', quoteVol: '100' },
  { time: 1700000100000, open: '96005', high: '96020', low: '96000', close: '96010', quoteVol: '200' },
]);
command({ op: 'subscribeCandles', symbol: 'BTCUSDT', interval: '1m' });
read('account/get_leverage_margin_mode', 'symbol=BTCUSDT&marginCoin=USDT', { leverage: 30, marginMode: 'ISOLATION' });
const candle = (time, close, volume) => ({ channel: 'market_btcusdt_klineop_1min', data: { time, open: '96000', high: '97100', low: '95990', close, volume } });
ws(FUTURES, candle(1700000040, '97000', '10'));
ws(FUTURES, candle(1700000040, '97010', '11'));
ws(FUTURES, candle(1700000100, '97020', '20'));
ws(PUBLIC, { ch: 'ticker', symbol: 'BTCUSDT', data: { la: '97020', o: '95000', b: '1000', q: '96000000' } });
ws(PUBLIC, { ch: 'price', symbol: 'BTCUSDT', data: { mp: '97001', ip: '97002', fr: '0.0001', ft: '1700000200000' } });
ws(PRIVATE, { ch: 'position', data: { event: 'UPDATE', ...position(30, 'ISOLATION', '-4.25') } });
read('position/get_pending_positions', '', { positionList: [position(30, 'ISOLATION', '-4.25')] }, t);
records.push({ t: (t += 400), type: 'close', url: PRIVATE, connection: 0, code: 1006 });
records.push({ t, type: 'connect', url: PRIVATE, connection: 1, initial: false });
records.push({ t: (t += 1600), type: 'open', url: PRIVATE, connection: 1 });
privateConnection = 1;
ws(PRIVATE, { op: 'login', data: { result: true } });
t = Math.max(t, 19600);
ws(PUBLIC, { ch: 'ticker', symbol: 'ETHUSDT', data: { la: '3001', o: '3000', b: '50000', q: '150000000' } });
mkdirSync(dirname(values.out), { recursive: true });
records.sort((a, b) => a.t - b.t);
writeFileSync(values.out, `${records.map((r) => JSON.stringify(r)).join('\n')}\n`);
console.log(JSON.stringify({ out: values.out, records: records.length, seconds: t / 1000 }));
