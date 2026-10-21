// Documented Contract V3 futures schemas: https://www.weex.com/api-doc/llms-full.txt
// Market_API, Account_API, Transaction_API and streams/private examples. Amounts
// are adapted to two markets; credentials are public replay constants.
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname } from 'node:path';
import { parseArgs } from 'node:util';
const { values } = parseArgs({ options: { out: { type: 'string', default: 'sdk-rs/harness/tapes/weex-private.jsonl' } } });
const REST = 'https://api-contract.weex.com';
const PUBLIC = 'wss://ws-contract.weex.com/v3/ws/public';
const PRIVATE = 'wss://ws-contract.weex.com/v3/ws/private';
const records = [];
const http = (method, path, body, requestBody, effectiveAt = 0, afterCommand) => records.push({ t: 0, effectiveAt, ...(afterCommand === undefined ? {} : { afterCommand }), type: 'http', method, url: REST + path, requestBody: requestBody && JSON.stringify(requestBody), status: 200, body: JSON.stringify(body) });
const ws = (t, url, payload, extra = {}) => records.push({ t, type: 'ws', url, text: JSON.stringify(payload), ...extra });
let commands = 0;
const command = (t, payload) => { commands++; records.push({ t, type: 'command', ...payload }); };
// A write belongs to the command just issued; its stage gates that command's snapshots.
const write = (method, path, body, requestBody) => http(method, path, body, requestBody, 0, commands);
const market = (symbol, baseAsset, pricePrecision) => ({ symbol, baseAsset, quoteAsset: 'USDT', marginAsset: 'USDT', pricePrecision, quantityPrecision: 3, minOrderSize: 0.001, maxOrderSize: 10000, minLeverage: 1, maxLeverage: 100, contractVal: 1, makerFeeRate: 0.0002, takerFeeRate: 0.0008 });
http('GET', '/capi/v3/market/exchangeInfo', { symbols: [market('BTCUSDT', 'BTC', 1), market('ETHUSDT', 'ETH', 2)] });
http('GET', '/capi/v3/market/ticker/24hr', ['BTCUSDT', 'ETHUSDT'].map((symbol, i) => ({ symbol, lastPrice: i ? '3573.40' : '69350.1', markPrice: i ? '3573.4' : '69350.1', indexPrice: i ? '3572' : '69349', priceChangePercent: '1.2', volume: '1000', quoteVolume: '100000' })));
http('GET', '/capi/v3/market/ticker/bookTicker', [{ symbol: 'BTCUSDT', bidPrice: '69350', askPrice: '69350.1' }, { symbol: 'ETHUSDT', bidPrice: '3573.39', askPrice: '3573.4' }]);
http('GET', '/capi/v3/account/accountConfig', { dualSidePosition: false, canTrade: true });
http('GET', '/capi/v3/account/symbolConfig', ['BTCUSDT', 'ETHUSDT'].map((symbol) => ({ symbol, marginType: 'CROSSED', separatedType: 'COMBINED', crossLeverage: '20', isolatedLongLeverage: '20', isolatedShortLeverage: '20' })));
http('GET', '/capi/v3/account/balance', [{ asset: 'USDT', balance: '5000', availableBalance: '4950', frozen: '50', unrealizePnl: '10' }]);
const position = { symbol: 'BTCUSDT', side: 'LONG', marginType: 'CROSSED', separatedMode: 'COMBINED', leverage: '20', size: '0.01', openValue: '690', unrealizePnl: '10', realizedPnl: '0', liquidatePrice: '0', marginSize: '34.5', updatedTime: '1773298805123' };
http('GET', '/capi/v3/account/position/allPosition', [position]);
const row = (orderId, symbol, side, quantity, price, extra = {}) => ({ orderId, symbol, side, type: 'LIMIT', origQty: quantity, executedQty: '0', price, status: 'NEW', timeInForce: 'GTC', updateTime: 1773298805123, ...extra });
const resting = row('702345678901234560', 'BTCUSDT', 'BUY', '0.005', '68000');
const placedBtc = row('702345678901234561', 'BTCUSDT', 'BUY', '0.002', '69300', { clientOrderId: 'caller-btc' });
const placedEth = row('702345678901234562', 'ETHUSDT', 'SELL', '0.5', '3600', { clientOrderId: 'caller-eth' });
// Later than write acknowledgements in either replay, so stream updates are
// authoritative. The final checkpoint precedes the next periodic REST cycle.
const streamTime = 4_000_000_000_000;
// Documented Order Channel push row (streams/private/Order-Channel). Generated
// algo client IDs are unknown to the tape, so those rows omit clientOrderId.
const orderEvent = ({ id, symbol = 'BTCUSDT', side, type, size, price = '0', triggerPrice = '0', status, reduceOnly = false, clientOrderId }) => ({
  id, coin: 'USDT', symbol, marginMode: 'CROSSED', separatedMode: 'COMBINED', positionSide: (side === 'BUY') !== reduceOnly ? 'LONG' : 'SHORT',
  orderSide: side, price, size, ...(clientOrderId ? { clientOrderId } : {}), type, timeInForce: 'GTC', reduceOnly, triggerPrice,
  triggerPriceType: 'CONTRACT_PRICE', status, cumFillSize: '0', cumFillValue: '0', createdTime: '1773298805123', updatedTime: String(streamTime),
});
const finalEth = { ...placedEth, price: '3610', executedQty: '0.1', status: 'PARTIALLY_FILLED', updateTime: streamTime };
const finalPosition = { ...position, size: '0.02', openValue: '1380', marginSize: '69', unrealizePnl: '12', updatedTime: String(streamTime) };
const algo = { algoId: '702345678901234563', symbol: 'BTCUSDT', side: 'BUY', orderType: 'STOP_MARKET', algoStatus: 'NEW', quantity: '0.001', triggerPrice: '70000', price: '0', timeInForce: 'GTC', createTime: 1773298805123 };
const protective = { algoId: '702345678901234564', symbol: 'BTCUSDT', side: 'SELL', orderType: 'TAKE_PROFIT_MARKET', planType: 'TAKE_PROFIT', reduceOnly: true, algoStatus: 'NEW', quantity: '0.01', triggerPrice: '76000', price: '0', timeInForce: 'GTC', createTime: 1773298805123 };
const finalAlgo = { ...algo, algoId: '702345678901234565', triggerPrice: '71000' };
// Each snapshot shows the outcome of one command: it is served only after
// that command's writes are acknowledged, however a poll is timed.
for (const [at, afterCommand, rows] of [[0, 0, [resting]], [2800, 1, [resting, placedBtc, placedEth]], [4100, 3, [resting, placedEth]], [5300, 5, [placedEth]], [7200, 6, [finalEth]]]) http('GET', '/capi/v3/openOrders?limit=100&page=0', rows, undefined, at, afterCommand);
http('GET', '/capi/v3/account/position/allPosition', [finalPosition], undefined, 7200);
for (const [at, afterCommand, rows] of [[0, 0, []], [3300, 2, [algo]], [4100, 3, []], [4500, 4, [protective]], [5300, 5, []], [6300, 6, [finalAlgo]]]) http('GET', '/capi/v3/openAlgoOrders?limit=100&page=1', rows, undefined, at, afterCommand);
ws(400, PUBLIC, { event: 'ping', time: '1773298805000' });
ws(800, PRIVATE, { type: 'ping', time: '1773298805000' });
ws(1200, PRIVATE, { e: 'positions', E: 1773298805123, d: [position] });
ws(1600, PRIVATE, { e: 'orders', E: 1773298805123, d: [resting] });
ws(2000, PRIVATE, { e: 'fill', E: 1773298805123, d: [{ id: 'execution-1', orderId: resting.orderId, symbol: 'BTCUSDT', orderSide: 'BUY', fillSize: '0.001', fillValue: '68', fillFee: '0.01', realizePnl: '0', createdTime: '1773298805123' }, { id: 'execution-2', orderId: resting.orderId, symbol: 'BTCUSDT', orderSide: 'BUY', fillSize: '0.002', fillValue: '136.2', fillFee: '0.02', realizePnl: '0', createdTime: '1773298805123' }] });
command(2500, { op: 'placeOrders', orders: [{ symbol: 'BTCUSDT', type: 'limit', side: 'buy', amount: 0.0021, price: 69300.04, clientOrderId: 'caller-btc' }, { symbol: 'ETHUSDT', type: 'limit', side: 'sell', amount: 0.5, price: 3600, clientOrderId: 'caller-eth' }] });
write('POST', '/capi/v3/batchOrders', [{ orderId: placedEth.orderId, clientOrderId: 'caller-eth', success: true }, { orderId: placedBtc.orderId, clientOrderId: 'caller-btc', success: true }], { batchOrders: [{ symbol: 'BTCUSDT', side: 'BUY', positionSide: 'LONG', type: 'LIMIT', timeInForce: 'GTC', quantity: '0.002', price: '69300', newClientOrderId: 'caller-btc' }, { symbol: 'ETHUSDT', side: 'SELL', positionSide: 'SHORT', type: 'LIMIT', timeInForce: 'GTC', quantity: '0.5', price: '3600', newClientOrderId: 'caller-eth' }] });
ws(2800, PRIVATE, { e: 'orders', E: 1773298805123, d: [placedBtc, placedEth] });
command(3300, { op: 'placeOrders', orders: [{ symbol: 'BTCUSDT', type: 'conditional', side: 'buy', amount: 0.001, triggerPrice: 70000 }] });
write('POST', '/capi/v3/algoOrder', { orderId: algo.algoId, clientOrderId: '$clientAlgoId', success: true }, { symbol: 'BTCUSDT', side: 'BUY', positionSide: 'LONG', type: 'STOP_MARKET', quantity: '0.001', triggerPrice: '70000' });
ws(3600, PUBLIC, { event: 'ping', time: '1773298805000' });
ws(3600, PRIVATE, { e: 'orders', E: streamTime, d: [{ id: algo.algoId, symbol: 'BTCUSDT', orderSide: 'BUY', type: 'STOP_MARKET', status: 'UNTRIGGERED', size: '0.001', price: '0', triggerPrice: '70000', updatedTime: String(streamTime) }] });
command(4100, { op: 'cancelOrders', ids: [placedBtc.orderId, algo.algoId] });
write('DELETE', '/capi/v3/batchOrders', [{ orderId: placedBtc.orderId, success: true }], { orderIdList: [placedBtc.orderId] });
write('DELETE', `/capi/v3/algoOrder?orderId=${algo.algoId}`, { orderId: algo.algoId, success: true });
command(4500, { op: 'placeOrders', orders: [{ symbol: 'BTCUSDT', type: 'take_profit_market', side: 'sell', amount: 0.01, price: 76000, reduceOnly: true }] });
write('POST', '/capi/v3/placeTpSlOrder', { orderId: protective.algoId, success: true }, { symbol: 'BTCUSDT', planType: 'TAKE_PROFIT', triggerPrice: '76000', executePrice: '0', quantity: '0.01', positionSide: 'LONG', triggerPriceType: 'CONTRACT_PRICE' });
ws(4200, PRIVATE, { e: 'orders', E: streamTime, msgEvent: 'OrderUpdate', d: [
  orderEvent({ id: placedBtc.orderId, side: 'BUY', type: 'LIMIT', size: '0.002', price: '69300', status: 'CANCELED', clientOrderId: 'caller-btc' }),
  orderEvent({ id: algo.algoId, side: 'BUY', type: 'STOP_MARKET', size: '0.001', triggerPrice: '70000', status: 'CANCELED' }),
] });
// Production often pushes the order event before the REST acknowledgement.
ws(4600, PRIVATE, { e: 'orders', E: streamTime, msgEvent: 'OrderUpdate', d: [
  orderEvent({ id: protective.algoId, side: 'SELL', type: 'TAKE_PROFIT_MARKET', size: '0.01', triggerPrice: '76000', status: 'UNTRIGGERED', reduceOnly: true }),
] }, { beforeAck: true, afterCommand: 4 });
ws(4800, PRIVATE, { type: 'ping', time: '1773298805000' });
command(5300, { op: 'cancelSymbolOrders', symbol: 'BTCUSDT' });
write('DELETE', '/capi/v3/allOpenOrders?symbol=BTCUSDT', [{ orderId: resting.orderId, success: true }]);
write('DELETE', '/capi/v3/algoOpenOrders?symbol=BTCUSDT', [{ orderId: protective.algoId, success: true }]);
ws(5400, PRIVATE, { e: 'orders', E: streamTime, msgEvent: 'OrderUpdate', d: [
  orderEvent({ id: resting.orderId, side: 'BUY', type: 'LIMIT', size: '0.005', price: '68000', status: 'CANCELED' }),
  orderEvent({ id: protective.algoId, side: 'SELL', type: 'TAKE_PROFIT_MARKET', size: '0.01', triggerPrice: '76000', status: 'CANCELED', reduceOnly: true }),
] });
ws(5600, PRIVATE, { e: 'account', E: 1773298805123, d: [{ coin: 'USDT', amount: '4950' }] });
command(6300, { op: 'placeOrders', orders: [{ symbol: 'BTCUSDT', type: 'conditional', side: 'buy', amount: 0.001, triggerPrice: 71000 }] });
write('POST', '/capi/v3/algoOrder', { orderId: finalAlgo.algoId, clientOrderId: '$clientAlgoId', success: true }, { symbol: 'BTCUSDT', side: 'BUY', positionSide: 'LONG', type: 'STOP_MARKET', quantity: '0.001', triggerPrice: '71000' });
ws(6400, PRIVATE, { e: 'orders', E: 1773298805123, d: [finalAlgo] });
// A post-only buy above the ask is accepted and canceled at once; its terminal
// event arrives before the acknowledgement, which must not reopen the order.
const rejectedPostOnly = '702345678901234566';
command(6700, { op: 'placeOrders', orders: [{ symbol: 'BTCUSDT', type: 'limit', side: 'buy', amount: 0.001, price: 69400, timeInForce: 'PostOnly', clientOrderId: 'caller-post' }] });
write('POST', '/capi/v3/batchOrders', [{ orderId: rejectedPostOnly, clientOrderId: 'caller-post', success: true }], { batchOrders: [{ symbol: 'BTCUSDT', side: 'BUY', positionSide: 'LONG', type: 'LIMIT', timeInForce: 'POST_ONLY', quantity: '0.001', price: '69400', newClientOrderId: 'caller-post' }] });
ws(6800, PRIVATE, { e: 'orders', E: streamTime, msgEvent: 'OrderUpdate', d: [
  orderEvent({ id: rejectedPostOnly, side: 'BUY', type: 'LIMIT', size: '0.001', price: '69400', status: 'CANCELED', clientOrderId: 'caller-post' }),
] }, { beforeAck: true, afterCommand: commands });
ws(7200, PRIVATE, { e: 'positions', E: streamTime, d: [finalPosition] });
ws(7200, PRIVATE, { e: 'orders', E: streamTime, d: [finalEth] });
ws(7200, PRIVATE, { type: 'ping', time: String(streamTime) });
mkdirSync(dirname(values.out), { recursive: true });
records.sort((a, b) => a.t - b.t);
writeFileSync(values.out, records.map((r) => JSON.stringify(r)).join('\n') + '\n');
console.log(JSON.stringify({ out: values.out, records: records.length }));
