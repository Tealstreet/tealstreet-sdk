// Synthetic account; Omni v3 documented envelopes and real zkLink request checks.
// No live account or API credentials are contacted by this generator.
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname } from 'node:path';
import { parseArgs } from 'node:util';
import { APEX_ACCOUNT } from './apexomni-protocol.mjs';
const { values } = parseArgs({ options: { out: { type: 'string', default: 'sdk-rs/harness/tapes/apexomni-private.jsonl' } } });
const REST = 'https://omni.apex.exchange/api';
const PUBLIC = 'wss://quote.omni.apex.exchange/realtime_public?v=2';
const PRIVATE = 'wss://quote.omni.apex.exchange/realtime_private?v=2';
const EPOCH = 1792454400000;
const records = [{ t: 0, type: 'meta', epoch: EPOCH, private: true }];
const ok = (data) => ({ code: 0, data });
const stage = () => ({ afterCommand: records.filter((r) => r.type === 'command').length });
function http(method, path, params, data, requestBody, extra = {}) {
  records.push({ t: 0, type: 'http', method, url: `${REST}${path}${params ? `?${new URLSearchParams(params)}` : ''}`, status: 200,
    body: JSON.stringify(data), ...(requestBody === undefined ? {} : { requestBody }), ...extra });
}
const ws = (t, url, body) => records.push({ t, type: 'ws', url, text: JSON.stringify(body) });
const accountWs = (t, contents, type = 'delta') => ws(t, PRIVATE, { topic: 'ws_zk_accounts_v3', type, contents, timestamp: EPOCH + t });
const command = (t, op, args = {}) => records.push({ t, type: 'command', op, ...args });
const position = (side, leverage = 10) => ({ symbol: 'BTC-USDT', side, size: side === 'LONG' ? '0.01' : '0.005', entryPrice: '60000', customImr: String(1 / leverage), unrealisedPnl: '0', curRealisedPnl: '0', liqPrice: side === 'LONG' ? '50000' : '70000' });
const account = (leverage = 10) => ok({ id: APEX_ACCOUNT, l2key: 'replay-l2-key', ethereumAddress: '0x' + '01'.repeat(20),
  positions: [position('LONG', leverage), position('SHORT', leverage)], contractAccount: { makerFeeRate: '0.0002', takerFeeRate: '0.0005' } });
const balance = ok({ totalEquityValue: '1000', initialMargin: '100', maintenanceMargin: '20', availableBalance: '900' });
const order = (id, size = '0.001', price = '60000', extra = {}) => ({ id, symbol: 'BTC-USDT', status: 'OPEN', type: 'LIMIT', side: 'BUY',
  size, price, triggerPrice: '0', triggerPriceType: 'MARKET', cumSuccessFillSize: '0', cumSuccessFillFee: '0', timeInForce: 'GOOD_TIL_CANCEL', reduceOnly: false, ...extra });
const brackets = (id, stop = '58000') => order(id, '0.003', '59000', {
  openSlParam: { triggerSize: '0.003', triggerPrice: stop, triggerPriceType: 'INDEX' },
  openTpParams: { triggerSize: '0.003', triggerPrice: '67000', triggerPriceType: 'MARKET' },
});
const fill = (id, t = 900) => ({ id, matchFillId: `match-${id}`, orderId: 'order-caller', symbol: 'BTC-USDT', side: 'BUY', size: '0.001', price: '60000', fee: '0.03', createdAt: EPOCH + t, updatedAt: EPOCH + t, status: 'SUCCESS', direction: 'TAKER' });
let openOrders = [];
let leverage = 10;
const snapshot = () => {
  http('GET', '/v3/account', null, account(leverage), undefined, stage());
  http('GET', '/v3/open-orders', null, ok(openOrders), undefined, stage());
  http('GET', '/v3/account-balance', null, balance, undefined, stage());
};
const generated = (size, price, extra = {}) => ({ brokerId: '6587', clientId: 'recorded-parent', clientOrderId: 'recorded-parent',
  expiration: 'recorded-expiration', limitFee: '0.000000', price, side: 'BUY', signature: 'recorded-signature', size,
  symbol: 'BTC-USDT', timeInForce: 'GOOD_TIL_CANCEL', type: 'LIMIT', ...extra });
const bracketBody = (stop = '58000', slPrice = '57710', slFee = '0.086565') => generated('0.003', '59000', {
  limitFee: '0.088500', isOpenTpslOrder: 'true', isSetOpenTp: 'true', isSetOpenSl: 'true',
  tpClientOrderId: 'recorded-tp', tpExpiration: 'recorded-expiration', tpLimitFee: '0.099998', tpPrice: '66665', tpSide: 'SELL',
  tpSignature: 'recorded-signature', tpSize: '0.003', tpTriggerPrice: '67000', tpTriggerPriceType: 'MARKET',
  slClientOrderId: 'recorded-sl', slExpiration: 'recorded-expiration', slLimitFee: slFee, slPrice, slSide: 'SELL',
  slSignature: 'recorded-signature', slSize: '0.003', slTriggerPrice: stop, slTriggerPriceType: 'INDEX',
});

http('GET', '/v3/symbols', null, ok({ contractConfig: { perpetualContract: [{
  symbol: 'BTC-USDT', crossSymbolName: 'BTCUSDT', baseTokenId: 'BTC', settleAssetId: 'USDT', enableTrade: true,
  stepSize: '0.001', tickSize: '0.1', minOrderSize: '0.001', maxOrderSize: '1000', displayMinLeverage: '1',
  displayMaxLeverage: '100', l2PairId: '50000', initialMarginRate: '0.1', defaultInitialMarginRate: '0.1', maxMarketPriceRange: '0.045',
}] } }));
http('GET', '/v3/user', null, ok({ id: 'replay-user' }));
snapshot();
ws(100, PUBLIC, { op: 'pong' });
ws(120, PUBLIC, { topic: 'instrumentInfo.all', data: [{ s: 'BTCUSDT', p: '60000', mp: '60000', xp: '60000', pr: '0.01', o: '100', v: '5', to: '300000', fr: '-0.0001' }] });
accountWs(150, { fills: [fill('snapshot-old', -60000)], orders: [], positions: [position('LONG'), position('SHORT')] }, 'snapshot');
command(500, 'placeOrders', { orders: [{ symbol: 'BTCUSDT', type: 'limit', side: 'buy', amount: 1000.001, price: 60000, stopLoss: 58000 }] });

command(800, 'placeOrders', { orders: [{ symbol: 'BTCUSDT', type: 'limit', side: 'buy', amount: 0.002, price: 60000, clientOrderId: 'caller-exact-01' }] });
const caller = order('order-caller', '0.002', '60000', { clientId: 'caller-exact-01' });
openOrders.push(caller); snapshot();
const callerBody = generated('0.002', '60000', { clientId: 'caller-exact-01', clientOrderId: 'caller-exact-01', limitFee: '0.060000' });
http('POST', '/v3/order', null, { code: 20016, msg: 'sign check failed' }, callerBody);
http('POST', '/v3/order', null, ok({ id: 'order-caller' }), callerBody);
accountWs(1000, { orders: [caller] });
const executed = { ...caller, status: 'FILLED', cumSuccessFillSize: '0.002', cumSuccessFillFee: '0.06', updatedAt: EPOCH + 1100 };
accountWs(1100, { fills: [fill('exec-a', 1100), fill('exec-b', 1100)], orders: [executed] });
accountWs(1150, { fills: [fill('exec-a', 1100), fill('exec-b', 1100)], orders: [executed] });
openOrders = [];

command(1400, 'placeOrders', { orders: [{ symbol: 'BTCUSDT', type: 'limit', side: 'buy', amount: 0.003, price: 59000, stopLoss: 58000, takeProfit: 67000 }] });
openOrders.push(brackets('order-bracket')); snapshot();
http('POST', '/v3/order', null, ok({ id: 'order-bracket' }), bracketBody());
accountWs(1700, { orders: [brackets('order-bracket')] });
ws(2000, PRIVATE, { op: 'ping', args: ['private-token', 7] });

command(2400, 'amendOrder', { id: 'order-bracket__stop_loss', price: 58100 });
openOrders = [brackets('order-replaced', '58100')]; snapshot();
http('POST', '/v3/delete-order', null, ok({ id: 'order-bracket' }), { id: 'order-bracket' });
http('POST', '/v3/order', null, ok({ id: 'order-replaced' }), bracketBody('58100', '57809.5', '0.086715'));
accountWs(2750, { orders: [brackets('order-replaced', '58100')] });

command(3300, 'placeOrders', { orders: [{ symbol: 'BTCUSDT', type: 'market', side: 'buy', amount: 0.002 }] });
const market = order('order-market', '0.002', '61200', { type: 'MARKET' });
openOrders.push(market); snapshot();
http('POST', '/v3/order', null, ok({ id: 'order-market' }), generated('0.002', '61200', { limitFee: '0.061200', type: 'MARKET' }));
accountWs(3600, { orders: [market] });

command(4200, 'placeOrders', { orders: [{ symbol: 'BTCUSDT', type: 'stop_market', side: 'sell', amount: 0.001, price: 58000, reduceOnly: true }] });
const stop = order('order-stop', '0.001', '57710', { type: 'STOP_LIMIT', status: 'UNTRIGGERED', side: 'SELL', reduceOnly: true, triggerPrice: '58000', triggerPriceType: 'INDEX' });
openOrders.push(stop); snapshot();
http('POST', '/v3/order', null, ok({ id: 'order-stop' }), generated('0.001', '57710', { side: 'SELL', limitFee: '0.028855', type: 'STOP_LIMIT', reduceOnly: 'true', triggerPrice: '58000', triggerPriceType: 'INDEX' }));
accountWs(4550, { orders: [stop] });

command(5100, 'placeOrders', { orders: [{ symbol: 'BTCUSDT', type: 'take_profit_market', side: 'sell', amount: 0.001, price: 65000, reduceOnly: true }] });
const take = order('order-take', '0.001', '64675', { type: 'TAKE_PROFIT_LIMIT', status: 'UNTRIGGERED', side: 'SELL', reduceOnly: true, triggerPrice: '65000', triggerPriceType: 'MARKET' });
openOrders.push(take); snapshot();
http('POST', '/v3/order', null, ok({ id: 'order-take' }), generated('0.001', '64675', { side: 'SELL', limitFee: '0.032338', type: 'TAKE_PROFIT_LIMIT', reduceOnly: 'true', triggerPrice: '65000', triggerPriceType: 'MARKET' }));
accountWs(5450, { orders: [take] });

command(6100, 'cancelOrders', { ids: ['order-market'] });
openOrders = openOrders.filter((row) => row.id !== 'order-market'); snapshot();
http('POST', '/v3/delete-order', null, ok({ id: 'order-market' }), { id: 'order-market' });
accountWs(6450, { orders: [{ ...market, status: 'CANCELED', cancelReason: 'USER_CANCELED' }] });

command(7000, 'setLeverage', { symbol: 'BTCUSDT', leverage: 20 });
leverage = 20; snapshot();
const leverageBody = { symbol: 'BTC-USDT', token: 'USDT', initialMarginRate: '0.05000' };
http('POST', '/v3/set-initial-margin-rate', null, { code: 20016, msg: 'sign check failed' }, leverageBody);
http('POST', '/v3/set-initial-margin-rate', null, ok({}), leverageBody);
accountWs(7450, { positions: [position('LONG', 20), position('SHORT', 20)] });

command(8100, 'fetchFills', { symbol: 'BTCUSDT', limit: 2 });
http('GET', '/v3/fills', { symbol: 'BTC-USDT', limit: 2 }, ok({ fills: ['rest-a', 'rest-b'].map((tradeId) => ({ tradeId, orderId: 'order-caller', symbol: 'BTC-USDT', side: 'BUY', fillPrice: '60000', fillSize: '0.001', createdAt: EPOCH + 1100, fee: '0.03', token: 'USDT' })) }));
command(9000, 'fetchFills', { symbol: 'BTCUSDT', limit: 3 });
for (const symbol of ['BTC-USDT', 'BTCUSDT']) http('GET', '/v3/fills', { symbol, limit: 3 }, ok({ fills: [] }));
http('GET', '/v3/history-orders', { symbol: 'BTC-USDT', limit: 3, orderType: 'HISTORY' }, ok({ orders: [{ id: 'aggregate-order', symbol: 'BTCUSDT', side: 'BUY', cumSuccessFillSize: '0.002', cumSuccessFillValue: '120', createdAt: EPOCH - 10000, cumSuccessFillFee: '0.06' }] }));
command(9450, 'fetchFills', { symbol: 'BTCUSDT', limit: 4 });
const anonymous = (offset) => ({ orderId: 'anonymous-order', symbol: 'BTC-USDT', side: 'SELL', price: '60010', size: '0.001', createdAt: EPOCH + offset, fee: '0.030005' });
http('GET', '/v3/fills', { symbol: 'BTC-USDT', limit: 4 }, ok({ fills: [anonymous(1200), anonymous(1600), anonymous(1200)] }));
command(9700, 'fetchFills', { symbol: 'BTCUSDT', limit: 5, from: 0, to: EPOCH });
const boundParams = { symbol: 'BTC-USDT', limit: 5, beginTimeInclusive: 0, endTimeExclusive: EPOCH + 1 };
http('GET', '/v3/fills', boundParams, { code: 20016, msg: 'sign check failed' });
http('GET', '/v3/fills', boundParams, ok({ fills: [{ tradeId: 'zero-bound', orderId: 'old-order', symbol: 'BTC-USDT', side: 'BUY', price: '59000', size: '0.001', createdAt: EPOCH - 1000, fee: '0.0295' }] }));
command(9800, 'fetchFills', { symbol: 'BTCUSDT', limit: 6, from: EPOCH + 1200, to: EPOCH + 1600 });
http('GET', '/v3/fills', { symbol: 'BTC-USDT', limit: 6, beginTimeInclusive: EPOCH + 1200, endTimeExclusive: EPOCH + 1601 },
  ok({ fills: [anonymous(1200), anonymous(1601), anonymous(1199), anonymous(1600)] }));
command(9900, 'fetchFills', { symbol: 'BTCUSDT', limit: 7, from: EPOCH + 1100, to: EPOCH + 1100 });
for (const symbol of ['BTC-USDT', 'BTCUSDT']) {
  const params = { symbol, limit: 7, beginTimeInclusive: EPOCH + 1100, endTimeExclusive: EPOCH + 1101 };
  http('GET', '/v3/fills', params, ok({ fills: [] }));
  http('GET', '/v3/history-orders', { ...params, orderType: 'HISTORY' }, ok({ orders: [] }));
}
command(10000, 'fetchIncome', {});
command(10500, 'fetchOrderHistory', {});
command(10700, 'getAccount', {});
command(11000, 'cancelSymbolOrders', { symbol: 'BTCUSDT' });
openOrders = []; snapshot();
http('POST', '/v3/delete-open-orders', null, ok({}), { symbol: 'BTC-USDT' });
accountWs(11350, { orders: [brackets('order-replaced', '58100'), stop, take].map((row) => ({ ...row, status: 'CANCELED', cancelReason: 'USER_CANCELED' })) });
command(12000, 'fetchCandles', { symbol: 'BTCUSDT', interval: '1m', limit: 2, to: EPOCH });
http('GET', '/v3/klines', { symbol: 'BTCUSDT', start: EPOCH / 1000 - 120, end: EPOCH / 1000, interval: '1' }, ok({ BTCUSDT: [{ t: EPOCH - 60000, o: '59990', h: '60010', l: '59980', c: '60000', v: '1' }] }));
command(12100, 'placeOrders', { orders: [{ symbol: 'BTCUSDT', type: 'limit', side: 'buy', amount: 0.01, price: 60000, clientOrderId: 'partial-caller' }] });
const partial = order('partial-entry', '0.01', '60000', { clientId: 'partial-caller', cumSuccessFillSize: '0.006' });
openOrders = [partial]; snapshot();
http('POST', '/v3/order', null, ok({ id: 'partial-entry' }), generated('0.01', '60000', { clientId: 'partial-caller', clientOrderId: 'partial-caller', limitFee: '0.300000' }));
accountWs(12200, { orders: [partial] });
command(12400, 'amendOrder', { id: 'partial-entry', price: 59900 });
const residual = order('partial-replaced', '0.004', '59900');
openOrders = [residual]; snapshot();
http('POST', '/v3/delete-order', null, ok({ id: 'partial-entry' }), { id: 'partial-entry' });
http('POST', '/v3/order', null, ok({ id: 'partial-replaced' }), generated('0.004', '59900', { limitFee: '0.119800' }));
accountWs(12600, { orders: [residual] });
command(12700, 'cancelOrders', { ids: ['partial-replaced'] });
openOrders = []; snapshot();
http('POST', '/v3/delete-order', null, ok({ id: 'partial-replaced' }), { id: 'partial-replaced' });
accountWs(12800, { orders: [{ ...residual, status: 'CANCELED', cancelReason: 'USER_CANCELED' }] });
records.push({ t: 13000, type: 'close', url: PRIVATE, code: 1006 });
accountWs(16800, { fills: [fill('snapshot-old', -60000)], orders: [], positions: [position('LONG', 20), position('SHORT', 20)] }, 'snapshot');
ws(17000, PRIVATE, { op: 'ping', args: ['after-reconnect'] });
for (const t of [5000, 10000, 17500]) ws(t, PUBLIC, { op: 'ping' });
for (const t of [5000, 10000, 17500]) ws(t, PRIVATE, { op: 'pong' });
records.sort((a, b) => a.t - b.t);
mkdirSync(dirname(values.out), { recursive: true });
writeFileSync(values.out, records.map((record) => JSON.stringify(record)).join('\n') + '\n');
console.log(JSON.stringify({ out: values.out, records: records.length, commands: records.filter((r) => r.type === 'command').length }));
