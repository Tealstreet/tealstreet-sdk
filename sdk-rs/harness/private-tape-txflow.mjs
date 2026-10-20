// Documented TxFlow testnet payloads. The signing key is the PUBLIC disposable
// signing-vector key; this generator never contacts or funds an account.
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname } from 'node:path';
import { parseArgs } from 'node:util';

const { values } = parseArgs({ options: { out: { type: 'string', default: 'sdk-rs/harness/tapes/txflow-private.jsonl' } } });
const REST = 'https://rpc.txflow-testnet.xyz';
const NODE = 'wss://rpc.txflow-testnet.xyz/ws';
const API = 'wss://rpc.txflow-testnet.xyz/api-ws';
const EPOCH = 1_700_000_000_000;
const USER = '0x0000000000000000000000000000000000000001';
const records = [
  { type: 'meta', venue: 'txflow', private: true, t: 0, epoch: EPOCH },
  { type: 'config', private: true, t: 0, options: { testnet: true, key: '', applicationId: USER, secret: '0x' + 'ab'.repeat(32) } },
];
let commands = 0;
const stage = () => ({ afterCommand: commands });
const success = (data) => ({ success: true, code: 0, data });
const http = (path, requestBody, body, extra = {}) => records.push({
  type: 'http', t: 0, method: 'POST', url: REST + path,
  requestBody: JSON.stringify(requestBody), status: 200, body: JSON.stringify(body), ...extra,
});
const info = (type, fields, data, extra = {}) => http('/info', { type, ...fields }, success(data), extra);
const command = (t, op, args = {}) => { records.push({ type: 'command', t, op, ...args }); commands += 1; };
const ws = (t, channel, data, url = NODE, socket = 0) => records.push({ type: 'ws', t, url, socket, text: JSON.stringify({ channel, data }) });
const write = (action, responseType) => http('/exchange', {
  action, nonce: EPOCH, signature: { r: '0x00', s: '0x00', v: 27 }, confirm: 'executed',
}, { status: 'ok', code: 0, confirm: 'executed', data: { type: responseType, data: { statuses: ['success'] } } }, stage());

const instrument = (index, base, priceTick, basePrecision) => ({
  index, name: `${base}-USDC`, baseCurrency: base, quoteCurrency: 'USDC', currencyType: 'usdc',
  priceTick, basePrecision, maxLeverage: 50, maxLimitOrderSize: 100, maxMarketOrderSize: 50,
  delisted: false, haltTrading: false, onlyIsolated: false, useNotionalMargin: false,
});
info('perpMeta', { includeDelisted: true }, { universe: [instrument(1, 'BTC', '0.1', '0.001'), instrument(2, 'ETH', '0.01', '0.001'), { ...instrument(9, 'OLD', '0.1', '0.001'), delisted: true, haltTrading: true }] });
const context = (price) => ({
  markPx: String(price), oraclePx: String(price), lastPrice: String(price), midPx: String(price),
  prevDayPx: String(price), funding: '0.01', openInterest: '2', dayNtlVlm: '1000', dayBaseVlm: '10', nextFundingTime: EPOCH + 3_600_000,
});
const tickerSeeds = [[1, 100], [2, 200]];
for (const [index, price] of tickerSeeds) {
  info('activeAssetCtx', { coin: String(index) }, {
    instrumentId: index, lastPrice: String(price), prevDayPx: String(price), markPx: String(price), oraclePx: String(price),
    openInterest: '2', dayNtlVlm: '1000', coinVlm: '10', nodeCtx: context(price),
  });
}

const setting = (coin, leverage, type = 'cross') => ({ user: USER, coin: String(coin), leverage: { type, mode: 'oneWay', value: leverage } });
let btcSetting = setting(1, 10);
let ethSetting = setting(2, 10);
let positions = [];
let orders = [];
let executions = [];
let stateTime = EPOCH;
const state = () => ({
  user: USER, time: stateTime, assetPositions: positions,
  marginSummary: { accountValue: '1000', totalMarginUsed: positions.length ? '20.5' : '0', totalRawUsd: positions.length ? '1002.5' : '1000' },
});
// Polling always sees the latest command-owned native state. The startup-window
// limit500 reconciliation read is distinct from indexed limit2000 query reads.
// Both production consumers pace each initial activeAssetCtx seed by 25 ms,
// including the last seed, before recording the account's execution window.
// This is an input-declared startup schedule, never a replay-output rewrite.
const accountStartupAt = EPOCH + tickerSeeds.length * 25;
const snapshots = (extra = stage()) => {
  info('clearinghouseState', { user: USER, accountType: 'perp' }, state(), extra);
  info('frontendOpenOrders', { user: USER, offset: 0, limit: 500 }, orders, extra);
  info('userFills', { user: USER, accountType: ['perp'], offset: 0, limit: 500, startTimeMs: accountStartupAt }, { data: executions, hasMore: false }, extra);
  info('activeAssetData', { user: USER, coin: '1' }, btcSetting, extra);
  info('activeAssetData', { user: USER, coin: '2' }, ethSetting, extra);
};
snapshots();
const order = (asset, oid, side, price, amount, cloid, time, extra = {}) => ({
  asset, oid, coin: asset === 1 ? 'BTC-USDC' : 'ETH-USDC', side, orderType: 'Limit',
  limitPx: String(price), origSz: String(amount), sz: String(amount), timestamp: EPOCH + time,
  state: 'Open', tif: 'GTC', reduceOnly: false, isTrigger: false,
  triggerPx: '', children: null, cloid, ...extra,
});
const update = (t, row, status = 'Open', socket = 0) => ws(t, 'orderUpdates', [{ order: row, status, statusTimestamp: EPOCH + t }], NODE, socket);
const limit = (asset, side, price, size, cloid, tif = 'Gtc') => ({
  a: asset, b: side === 'buy', p: String(price), s: String(size), r: false, t: { limit: { tif } }, c: cloid,
});
ws(100, 'allAssetCtxs', { ctxs: [{ coin: '1', ctx: context(100) }, { coin: '2', ctx: context(200) }] });
ws(180, 'pong', null, API);

command(400, 'setPositionMode', { hedged: false });
write({ type: 'updatePositionMode', isOneway: true }, 'UpdatePositionMode'); snapshots();
// Account channels follow the paced allAssetCtxs/order/state subscriptions.
ws(600, 'userAssetData', { user: USER, coins: [btcSetting, ethSetting] });
command(800, 'setLeverage', { symbol: 'BTC-USDC', leverage: 7 });
write({ type: 'updateLeverage', asset: 1, leverage: 7 }, 'UpdateLeverage'); btcSetting = setting(1, 7); snapshots();
ws(900, 'activeAssetData', btcSetting);
command(1200, 'setMarginType', { symbol: 'ETH-USDC', isolated: true });
write({ type: 'updateMarginMode', asset: 2, isCross: false }, 'UpdateMarginMode'); ethSetting = setting(2, 5, 'isolated'); snapshots();
ws(1300, 'activeAssetData', ethSetting);

command(1600, 'placeOrders', { orders: [{ symbol: 'BTC-USDC', type: 'limit', side: 'buy', amount: 0.25, price: 90, clientOrderId: 'caller-limit' }] });
write({ type: 'order', grouping: 'na', orders: [limit(1, 'buy', 90, 0.25, 'caller-limit')] }, 'PlaceOrder');
orders = [order(1, 1001, 'B', 90, 0.25, 'caller-limit', 1600)]; snapshots(); update(1750, orders[0]);
command(2100, 'amendOrder', { id: '1001', price: 91 });
write({ type: 'modify2', order: { o: 1001, p: '91', s: '0.25' } }, 'Modify2');
orders = [{ ...orders[0], limitPx: '91' }]; snapshots(); update(2250, orders[0]);

command(2500, 'placeOrders', { orders: [{ symbol: 'ETH-USDC', type: 'market', side: 'sell', amount: 0.5, clientOrderId: 'caller-market' }] });
write({ type: 'order', grouping: 'na', orders: [limit(2, 'sell', 'bbo', 0.5, 'caller-market', 'FrontendMarket')] }, 'PlaceOrder');
const marketOrder = order(2, 2001, 'A', 200, 0.5, 'caller-market', 2500, { orderType: 'Market', tif: '', sz: '0' });
const execution = { instrumentId: 2, tid: 9001, oid: 2001, time: EPOCH + 2500, px: '200', sz: '0.5', side: 'A', fee: '0.02', feeToken: 'USDC', closedPnl: '0', crossed: true };
executions = [execution]; snapshots(); update(2650, marketOrder, 'Filled'); update(2700, marketOrder, 'Filled');
// The API-service feed is historical and must not manufacture another fill.
ws(2750, 'userFills', { user: USER, isSnapshot: true, fills: [execution] }, API);

command(3000, 'cancelOrders', { ids: ['1001'] });
write({ type: 'cancel', cancels: [{ a: 1, o: 1001 }] }, 'CancelOrder');
const canceled = orders[0]; orders = []; snapshots(); update(3100, canceled, 'Canceled');

// Unmeasured protective/reduce-only behavior and malformed caller IDs fail in
// preflight; no corresponding exchange acknowledgement exists in this tape.
command(3400, 'placeOrders', { orders: [{ symbol: 'BTC-USDC', type: 'limit', side: 'buy', amount: 0.1, price: 90, stopLoss: 80, takeProfit: 120, clientOrderId: 'rejected-bracket' }] }); snapshots();
command(3800, 'placeOrders', { orders: [{ symbol: 'BTC-USDC', type: 'limit', side: 'buy', amount: 0.1, price: 90, clientOrderId: '0xreserved' }] }); snapshots();
command(4200, 'placeOrders', { orders: [
  { symbol: 'BTC-USDC', type: 'limit', side: 'buy', amount: 0.1, price: 90, clientOrderId: 'never-written' },
  { symbol: 'ETH-USDC', type: 'market', side: 'buy', amount: 0.1, reduceOnly: true, clientOrderId: 'rejected-reduce' },
] }); snapshots();

command(4600, 'placeOrders', { orders: [{ symbol: 'BTC-USDC', type: 'limit', side: 'buy', amount: 0.2, price: 89, timeInForce: 'PostOnly', clientOrderId: 'retained-caller' }] });
write({ type: 'order', grouping: 'na', orders: [limit(1, 'buy', 89, 0.2, 'retained-caller', 'Alo')] }, 'PlaceOrder');
orders = [order(1, 3001, 'B', 89, 0.2, 'retained-caller', 4600, { tif: 'ALO' })]; snapshots(); update(4700, orders[0]);
positions = [{ type: 'oneWay', position: {
  asset: '2', coin: 'ETH-USDC', szi: '-0.5', entryPx: '200', positionValue: '102.5',
  unrealizedPnl: '-2.5', liquidationPx: '300', marginUsed: '20.5', leverage: ethSetting.leverage, updateTime: EPOCH + 2500,
} }];
stateTime = EPOCH + 5000; snapshots({ ...stage(), effectiveAt: 5000 }); ws(5000, 'clearinghouseState', state());

const queryRange = { from: EPOCH, to: EPOCH + 10_000, limit: 100 };
const queryRead = (type, data, fields = {}) => info(type, { user: USER, ...(type === 'userNonFundingLedgerUpdates' ? {} : { accountType: ['perp'] }), ...fields, offset: 0, limit: 2000 }, { data, hasMore: false }, stage());
const filters = { coin: '2', startTimeMs: EPOCH, endTimeMs: EPOCH + 10_000 };
command(5400, 'fetchFills', { symbol: 'ETH-USDC', ...queryRange }); queryRead('userFills', [execution, execution], filters); snapshots();
command(5800, 'fetchOrderHistory', { symbol: 'BTC-USDC', ...queryRange });
queryRead('historicalOrders', [{ order: { instrumentId: 1, oid: 1001, timestamp: EPOCH + 1600, side: 'B', orderType: 'Limit', limitPx: '91', origSz: '0.25', filledAmount: '0', reduceOnly: false }, status: 'Canceled', statusTimestamp: EPOCH + 3100 }], { ...filters, coin: '1' }); snapshots();
const funding = { time: EPOCH + 5100, coin: 'ETH', amount: '-0.12', szi: '-0.5', fundingRate: '0.01', usdc: '-0.12' };
command(6200, 'fetchIncome', { symbol: 'ETH-USDC', incomeType: 'FUNDING_FEE', ...queryRange }); queryRead('userFundings', [funding], filters); snapshots();
command(6600, 'fetchIncome', { symbol: 'ETH-USDC', incomeType: 'COMMISSION', ...queryRange }); queryRead('userFills', [execution], filters); snapshots();

records.push({ type: 'close', t: 7500, url: NODE, socket: 0, code: 1006 });
update(8800, orders[0], 'Open', 1); ws(8900, 'clearinghouseState', state(), NODE, 1);
ws(9000, 'userAssetData', { user: USER, coins: [btcSetting, ethSetting] }, NODE, 1);
update(9100, marketOrder, 'Filled', 1);
records.push({ type: 'ws', t: 9200, url: NODE, socket: 1, text: '{"method":"PONG"}' });
ws(9300, 'pong', null, API);
mkdirSync(dirname(values.out), { recursive: true });
writeFileSync(values.out, records.map((record) => JSON.stringify(record)).join('\n') + '\n');
console.log(JSON.stringify({ out: values.out, events: records.filter((r) => r.type === 'ws').length, commands, writes: records.filter((r) => r.type === 'http' && r.url.endsWith('/exchange')).length, methods: ['order', 'cancel', 'modify2', 'updateLeverage', 'updateMarginMode', 'updatePositionMode'], expectedExecutions: 1 }));
