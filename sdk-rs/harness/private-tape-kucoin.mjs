// Classic Futures documented payloads, fake credentials, no live writes.
// https://www.kucoin.com/docs-new/rest/futures-trading/orders/add-order
// https://www.kucoin.com/docs-new/websocket-api/futures-trading/private-channels/orders
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname } from 'node:path';
import { parseArgs } from 'node:util';
const { values } = parseArgs({ options: { out: { type: 'string', default: 'sdk-rs/harness/tapes/kucoin-private.jsonl' } } });
const REST = 'https://api-futures.kucoin.com';
const endpoint = 'wss://ws-api-futures.kucoin.test/endpoint';
const publicUrl = endpoint + '?token=public-detail&connectId=fixture';
const snapshotUrl = endpoint + '?token=public-snapshot&connectId=fixture';
const privateUrl = endpoint + '?token=private-token&connectId=fixture';
const records = [];
const ok = (data) => ({ code: '200000', data });
const http = (method, path, data, body, extra = {}) => records.push({ t: 0, type: 'http', method, url: REST + path, status: 200, body: JSON.stringify(ok(data)), ...(body === undefined ? {} : { requestBody: JSON.stringify(body) }), ...extra });
const get = (path, data, extra = {}) => {
  // TS requests its implicit first page (50); this tape has at most one page.
  const parsed = new URL(path, REST);
  const sameReadAs = ['/api/v1/orders', '/api/v1/stopOrders'].includes(parsed.pathname)
    && parsed.search === '?status=active'
    ? REST + path + '&currentPage=1&pageSize=100' : undefined;
  return http('GET', path, data, undefined, { ...(sameReadAs ? { sameReadAs } : {}), ...extra });
};
const page = (items) => ({ currentPage: 1, pageSize: 100, totalNum: items.length, totalPage: 1, items });
const market = (symbol, baseCurrency, multiplier, tickSize) => ({ symbol, baseCurrency, quoteCurrency: 'USDT', settleCurrency: 'USDT', status: 'Open', lotSize: 1, tickSize, multiplier, maxOrderQty: 1000000, maxLeverage: 100, expireDate: null, isInverse: false });
get('/api/v1/contracts/active', [market('XBTUSDTM', 'XBT', 0.001, 0.1), market('ETHUSDTM', 'ETH', 0.01, 0.01)]);
get('/api/v1/allTickers', [{ symbol: 'XBTUSDTM', price: '100', bestBidPrice: '99', bestAskPrice: '101', size: '5' }, { symbol: 'ETHUSDTM', price: '10', bestBidPrice: '9', bestAskPrice: '11', size: '3' }]);
for (const token of ['public-detail', 'public-snapshot']) http('POST', '/api/v1/bullet-public', { token, instanceServers: [{ endpoint, pingInterval: 18000, pingTimeout: 10000 }] });
http('POST', '/api/v1/bullet-private', { token: 'private-token', instanceServers: [{ endpoint, pingInterval: 18000, pingTimeout: 10000 }] });
const position = { symbol: 'XBTUSDTM', currentQty: 10, avgEntryPrice: '90', markPrice: '100', leverage: '10', unrealisedPnl: '0.1', liquidationPrice: '70', marginMode: 'CROSS', positionSide: 'BOTH', positionMode: 0, currentTimestamp: 1610000000000 };
get('/api/v1/positions', [position]);
get('/api/v1/account-overview?currency=USDT', { availableBalance: '950', orderMargin: '50', accountEquity: '1010', unrealisedPNL: '10' });
get('/api/v2/position/getPositionMode', { positionMode: 0 }, { nativeRead: { reason: 'position-mode' } });
get('/api/v2/position/getMarginMode?symbol=XBTUSDTM', { marginMode: 'CROSS' });
get('/api/v2/getCrossUserLeverage?symbol=XBTUSDTM', { leverage: '10' });
const resting = { id: 'old-order', symbol: 'XBTUSDTM', type: 'limit', orderType: 'limit', side: 'buy', price: '90', size: 10, filledSize: 2, dealSize: 2, status: 'open', createdAt: 1610000000000, updatedAt: 1610000000000, reduceOnly: false };
get('/api/v1/orders?status=active&currentPage=1&pageSize=100', page([resting]));
get('/api/v1/stopOrders?status=active&currentPage=1&pageSize=100', page([]));
get('/api/v1/orders?status=active', page([resting]));
get('/api/v1/stopOrders?status=active', page([]));
let clock = 100;
let commandIndex = 0;
const ws = (url, data) => records.push({ t: clock += 100, type: 'ws', url, text: JSON.stringify(data) });
const message = (url, topic, data) => ws(url, { type: 'message', topic, data });
const command = (data) => { commandIndex++; records.push({ t: clock += 100, type: 'command', ...data }); };
ws(publicUrl, { type: 'welcome', id: 'fixture' });
ws(snapshotUrl, { type: 'welcome', id: 'fixture' });
ws(privateUrl, { type: 'welcome', id: 'fixture' });
message(snapshotUrl, '/contractMarket/snapshot:XBTUSDTM', { symbol: 'XBTUSDTM', lastPrice: '102', priceChgPct: '0.02', fundingRate: '0.0001', volume: '7', turnover: '700' });
const match = { orderId: 'old-order', symbol: 'XBTUSDTM', type: 'match', orderType: 'limit', status: 'match', side: 'buy', price: '90', size: 10, filledSize: 3, remainSize: 7, matchSize: 1, matchPrice: '89.5', tradeId: '1794175373644', ts: 1610000000001000000 };
message(privateUrl, '/contractMarket/tradeOrders', match);
command({ op: 'placeOrders', orders: [{ symbol: 'BTCUSDT', type: 'limit', side: 'buy', amount: 2, price: 95, reduceOnly: false, clientOrderId: 'caller-123' }] });
http('POST', '/api/v1/orders', { orderId: 'placed-order' }, { symbol: 'XBTUSDTM', type: 'limit', side: 'buy', price: '95', size: '2', leverage: 10, reduceOnly: false, marginMode: 'CROSS', timeInForce: 'GTC', clientOid: 'caller-123' });
const placed = { orderId: 'placed-order', symbol: 'XBTUSDTM', type: 'open', orderType: 'limit', status: 'open', side: 'buy', price: '95', size: 2, filledSize: 0, remainSize: 2, clientOid: 'caller-123', ts: 1610000000002000000 };
message(privateUrl, '/contractMarket/tradeOrders', placed);
command({ op: 'amendOrder', id: 'placed-order', price: 96 });
http('DELETE', '/api/v1/orders/placed-order?symbol=XBTUSDTM', { cancelledOrderIds: ['placed-order'] });
get('/api/v1/orders/placed-order?symbol=XBTUSDTM', { ...resting, id: 'placed-order', size: 2, filledSize: 0, dealSize: 0, isActive: false, cancelExist: true, status: 'done' }, { afterCommand: 2, nativeRead: { reason: 'cancel-proof', command: commandIndex } });
http('POST', '/api/v1/orders', { orderId: 'replacement' }, { symbol: 'XBTUSDTM', type: 'limit', side: 'buy', price: '96', size: '2', leverage: 10, reduceOnly: false, marginMode: 'CROSS', timeInForce: 'GTC', clientOid: 'generated-replacement' });
message(privateUrl, '/contractMarket/tradeOrders', { ...placed, type: 'canceled', status: 'canceled' });
message(privateUrl, '/contractMarket/tradeOrders', { ...placed, orderId: 'replacement', clientOid: 'generated-replacement', price: '96' });
command({ op: 'cancelOrders', ids: ['replacement'] });
http('DELETE', '/api/v1/orders/replacement?symbol=XBTUSDTM', { cancelledOrderIds: ['replacement'] });
message(privateUrl, '/contractMarket/tradeOrders', { ...placed, orderId: 'replacement', type: 'canceled', status: 'canceled' });
command({ op: 'cancelSymbolOrders', symbol: 'BTCUSDT' });
http('DELETE', '/api/v3/orders?symbol=XBTUSDTM', { cancelledOrderIds: ['old-order'] });
message(privateUrl, '/contractMarket/tradeOrders', { ...resting, orderId: 'old-order', type: 'canceled', status: 'canceled' });
// Read snapshots after the cancel command cannot resurrect the closed order.
get('/api/v1/orders?status=active&currentPage=1&pageSize=100', page([]), { afterCommand: 4 });
get('/api/v1/orders?status=active', page([]), { afterCommand: 4 });
command({ op: 'fetchCandles', symbol: 'BTCUSDT', interval: '1m', limit: 1, from: 1610000000000, to: 1610000060000 });
get('/api/v1/kline/query?symbol=XBTUSDTM&granularity=1&from=1610000000000&to=1610000060000', [[1610000000000, 90, 100, 80, 95, 10]]);
command({ op: 'fetchFills', symbol: 'BTCUSDT', from: 1610000000000, to: 1610000060000, limit: 10, expected: [{ id: 'fill-1', timestamp: 1610000001, symbol: 'BTCUSDT', orderId: 'old-order', price: 89.5, qty: 0.001, side: 'buy', commission: 0.0001, commissionAsset: 'USDT', maker: true }] });
get('/api/v1/fills?symbol=XBTUSDTM&startAt=1610000000000&endAt=1610000060000&currentPage=1&pageSize=50', { currentPage: 1, pageSize: 50, totalNum: 1, totalPage: 1, items: [{ symbol: 'XBTUSDTM', tradeId: 'fill-1', orderId: 'old-order', side: 'buy', liquidity: 'maker', price: '89.5', size: 1, fee: '0.0001', feeCurrency: 'USDT', tradeTime: '1610000001000000000', createdAt: 1610000001000 }] }, { afterCommand: commandIndex, nativeRead: { reason: 'query', command: commandIndex } });
command({ op: 'fetchIncome', from: 1610000000000, to: 1610000060000, expected: [{ symbol: 'BTCUSDT', incomeType: 'REALIZED_PNL', income: 2, asset: 'USDT', timestamp: 1610000001000, tranId: '9007199254740993', category: 'realized_pnl' }] });
get('/api/v1/transaction-history?startAt=1610000000000&endAt=1610000060000&currency=USDT&forward=false&maxCount=50', { dataList: [{ time: 1610000001000, type: 'RealisedPNL', amount: 2, offset: '9007199254740993', currency: 'USDT', status: 'Completed', remark: 'XBTUSDTM' }], hasMore: false }, { afterCommand: commandIndex, nativeRead: { reason: 'query', command: commandIndex } });
command({ op: 'fetchPublicTrades', symbol: 'BTCUSDT', limit: 1, expected: [{ id: 'trade-1', symbol: 'BTCUSDT', price: 100, qty: 0.001, quoteQty: 0.1, timestamp: 1610000001000, side: 'sell' }] });
get('/api/v1/trade/history?symbol=XBTUSDTM', [{ tradeId: 'trade-1', ts: '1610000001000000000', size: 1, price: '100', side: 'sell' }], { afterCommand: commandIndex, nativeRead: { reason: 'query', command: commandIndex } });
// Append commands so the original four-command terminal-snapshot fence stays fixed.
const orderBook = (items, afterCommand, stop = false) => {
  const path = stop ? '/api/v1/stopOrders' : '/api/v1/orders';
  get(path + '?status=active&currentPage=1&pageSize=100', page(items), { afterCommand });
  get(path + '?status=active', page(items), { afterCommand });
};
command({ op: 'placeOrders', orders: [{ symbol: 'BTCUSDT', type: 'limit', side: 'buy', amount: 2, price: 95, reduceOnly: false, stopLoss: 80, takeProfit: 110 }] });
http('POST', '/api/v1/st-orders', { orderId: 'bracket-parent' }, { symbol: 'XBTUSDTM', type: 'limit', side: 'buy', price: '95', size: '2', leverage: 10, reduceOnly: false, marginMode: 'CROSS', timeInForce: 'GTC', clientOid: 'generated-bracket', triggerStopUpPrice: '110', triggerStopDownPrice: '80', stopPriceType: 'TP' });
const bracket = { ...placed, orderId: 'bracket-parent', id: 'bracket-parent', clientOid: 'generated-bracket', triggerStopUpPrice: '110', triggerStopDownPrice: '80', stopPriceType: 'TP', ts: 1610000000003000000 };
orderBook([bracket], 9);
message(privateUrl, '/contractMarket/tradeOrders', bracket);
command({ op: 'cancelOrders', ids: ['bracket-parent__stop_loss', 'bracket-parent__take_profit'] });
http('DELETE', '/api/v1/orders/bracket-parent?symbol=XBTUSDTM', { cancelledOrderIds: ['bracket-parent'] });
orderBook([], 10);
message(privateUrl, '/contractMarket/tradeOrders', { ...bracket, type: 'canceled', status: 'canceled', ts: 1610000000004000000 });
command({ op: 'placeOrders', orders: [{ symbol: 'BTCUSDT', type: 'stop_market', side: 'sell', amount: 0, price: 80, reduceOnly: true }] });
http('POST', '/api/v1/orders', { orderId: 'full-stop' }, { symbol: 'XBTUSDTM', clientOid: 'generated-full-stop', side: 'sell', type: 'market', reduceOnly: true, closeOrder: true, stop: 'down', stopPrice: '80', stopPriceType: 'TP', marginMode: 'CROSS' });
const fullStop = { id: 'full-stop', orderId: 'full-stop', symbol: 'XBTUSDTM', clientOid: 'generated-full-stop', side: 'sell', type: 'open', orderType: 'market', status: 'open', size: 0, remainSize: 0, filledSize: 0, reduceOnly: true, closeOrder: true, stop: 'down', stopPrice: '80', stopPriceType: 'TP', ts: 1610000000005000000 };
orderBook([fullStop], 11, true);
message(privateUrl, '/contractMarket/advancedOrders', fullStop);
command({ op: 'cancelOrders', ids: ['full-stop'] });
http('DELETE', '/api/v1/orders/full-stop?symbol=XBTUSDTM', { cancelledOrderIds: ['full-stop'] });
orderBook([], 12, true);
message(privateUrl, '/contractMarket/advancedOrders', { ...fullStop, type: 'cancel', status: 'canceled', ts: 1610000000006000000 });
command({ op: 'setLeverage', symbol: 'BTCUSDT', leverage: 5 });
http('POST', '/api/v2/changeCrossUserLeverage', { leverage: 5 }, { symbol: 'XBTUSDTM', leverage: '5' });
get('/api/v1/positions', [{ ...position, leverage: '5' }], { afterCommand: 13 });
get('/api/v2/getCrossUserLeverage?symbol=XBTUSDTM', { leverage: '5' }, { afterCommand: 13 });
// Keep CROSS while the position is open: KuCoin prohibits changing its margin family.
command({ op: 'setMarginType', symbol: 'BTCUSDT', isolated: false });
http('POST', '/api/v2/position/changeMarginMode', { symbol: 'XBTUSDTM', marginMode: 'CROSS' }, { symbol: 'XBTUSDTM', marginMode: 'CROSS' });
message(privateUrl, '/contract/positionAll', { ...position, leverage: '5', currentTimestamp: 1610000000007, changeReason: 'marginChange' });
// Both implementations must reject this before switching mode with live exposure.
command({ op: 'setPositionMode', hedged: true });
command({ op: 'fetchOrderHistory', symbol: 'BTCUSDT', from: 1610000000000, to: 1610000060000, limit: 10, expected: [{ id: 'history-order', symbol: 'BTCUSDT', side: 'buy', type: 'limit', status: 'canceled', price: 90, averagePrice: 89.5, amount: 0.01, filled: 0.003, reduceOnly: false, createdTime: 1610000000000, updatedTime: 1610000001000 }] });
get('/api/v1/orders?symbol=XBTUSDTM&status=done&startAt=1610000000000&endAt=1610000060000&currentPage=1&pageSize=50', { currentPage: 1, pageSize: 50, totalNum: 1, totalPage: 1, items: [{ id: 'history-order', symbol: 'XBTUSDTM', side: 'buy', type: 'limit', price: '90', size: 10, dealSize: 3, dealValue: '0.2685', cancelExist: true, reduceOnly: false, createdAt: 1610000000000, updatedAt: 1610000001000 }] }, { afterCommand: commandIndex, nativeRead: { reason: 'query', command: commandIndex } });
command({ op: 'fetchIncome', symbol: 'BTCUSDT', incomeType: 'FUNDING_FEE', from: 1610000000000, to: 1610000060000, expected: [{ symbol: 'BTCUSDT', incomeType: 'FUNDING_FEE', income: -0.05585669, asset: 'USDT', timestamp: 1610000001000, tranId: '1472387374042586', category: 'funding' }] });
get('/api/v1/funding-history?symbol=XBTUSDTM&startAt=1610000000000&endAt=1610000060000&reverse=true&forward=false&maxCount=50', { dataList: [{ id: '1472387374042586', symbol: 'XBTUSDTM', timePoint: 1610000001000, funding: -0.05585669, settleCurrency: 'USDT' }], hasMore: false }, { afterCommand: commandIndex, nativeRead: { reason: 'query', command: commandIndex } });
mkdirSync(dirname(values.out), { recursive: true });
records.sort((a, b) => a.t - b.t);
writeFileSync(values.out, records.map((r) => JSON.stringify(r)).join('\n') + '\n');
console.log(JSON.stringify({ out: values.out, records: records.length }));
