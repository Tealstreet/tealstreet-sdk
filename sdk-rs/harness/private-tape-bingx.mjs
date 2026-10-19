// Deterministic swap payloads from BingX-API/api-ai-skills:
// swap-market, swap-account, swap-trade and swap-ws-account/api-reference.md.
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname } from 'node:path';
import { parseArgs } from 'node:util';

const { values } = parseArgs({ options: { out: { type: 'string', default: 'sdk-rs/harness/tapes/bingx-private.jsonl' } } });
const REST = 'https://open-api.bingx.com';
const PUBLIC = 'wss://ws-market-swap.qq-os.com/ws';
const PRIVATE = 'wss://open-api-swap.bingx.com/swap-market?listenKey=documented-replay-key';
const records = [];
const ok = (data) => ({ code: 0, msg: '', data });
const http = (method, path, params, body) => records.push({
  t: 0, type: 'http', method,
  url: `${REST}${path}${method !== 'POST' && Object.keys(params).length ? `?${new URLSearchParams(params)}` : ''}`,
  ...(method === 'POST' && Object.keys(params).length ? { requestBody: JSON.stringify(params) } : {}),
  status: 200, body: JSON.stringify(body),
});
let clock = 0;
const ws = (url, event) => records.push({ t: clock += 400, type: 'ws', url, text: typeof event === 'string' ? event : JSON.stringify(event) });
const command = (body) => records.push({ t: clock += 400, type: 'command', ...body });
const native = (orderId, symbol, side, type, quantity, price, extra = {}) => ({
  orderId, symbol, side, type, origQty: String(quantity), executedQty: '0', price: String(price),
  status: 'NEW', positionSide: 'BOTH', timeInForce: 'GTC', reduceOnly: false, ...extra,
});
http('GET', '/openApi/swap/v2/quote/contracts', {}, ok([
  { symbol: 'BTC-USDT', asset: 'BTC', currency: 'USDT', status: 1, quantityPrecision: 4, pricePrecision: 1, tradeMinLimit: '0.0001', maxLongLeverage: 125 },
  { symbol: 'ETH-USDT', asset: 'ETH', currency: 'USDT', status: 1, quantityPrecision: 3, pricePrecision: 2, tradeMinLimit: '0.001', maxLongLeverage: 100 },
]));
http('GET', '/openApi/swap/v2/quote/ticker', {}, ok([
  { symbol: 'BTC-USDT', lastPrice: '96000', priceChangePercent: '1.2', volume: '1000', quoteVolume: '96000000' },
  { symbol: 'ETH-USDT', lastPrice: '3000', priceChangePercent: '-0.5', volume: '50000', quoteVolume: '150000000' },
]));
http('GET', '/openApi/swap/v2/quote/premiumIndex', {}, ok([
  { symbol: 'BTC-USDT', markPrice: '96000', indexPrice: '95990', lastFundingRate: '0.01', nextFundingTime: 1760889600000 },
  { symbol: 'ETH-USDT', markPrice: '3000', indexPrice: '2999.5', lastFundingRate: '0.01', nextFundingTime: 1760889600000 },
]));
http('POST', '/openApi/user/auth/userDataStream', {}, { listenKey: 'documented-replay-key' });
http('GET', '/openApi/swap/v1/positionSide/dual', {}, ok({ dualSidePosition: 'false' }));
http('GET', '/openApi/swap/v3/user/balance', {}, ok([
  { asset: 'USDC', balance: '1000', availableMargin: '1000', usedMargin: '0', unrealizedProfit: '0' },
  { asset: 'USDT', balance: '4000', availableMargin: '3952', usedMargin: '48', unrealizedProfit: '10' },
]));
http('GET', '/openApi/swap/v2/user/positions', {}, ok([
  { symbol: 'BTC-USDT', positionId: '1785561029271568384', positionSide: 'BOTH', positionAmt: '0.01', avgPrice: '95000', leverage: 20, isolated: false, unrealizedProfit: '10', initialMargin: '48', liquidationPrice: '80000.5' },
]));
http('GET', '/openApi/swap/v2/trade/openOrders', {}, ok({ orders: [native('@8389765519993909012', 'BTC-USDT', 'BUY', 'LIMIT', 0.004, 90000, { clientOrderId: 'resting' })] }));
ws(PUBLIC, { dataType: 'market.contracts', data: { contracts: [{ symbol: 'ETH-USDT', tradePrice: '3001', fairPrice: '3000', indexPrice: '2999.5', fundingRate: '0.01', changePercentage: '-0.5', volumeF: '50001', valueF: '150000001' }] } });
const trade = (o, e = 'ORDER_TRADE_UPDATE') => ({ e, E: 1760880001000, o: { s: 'ETH-USDT', i: 1001, S: 'BUY', o: 'LIMIT', q: '1', p: '3000', z: '0', l: '0', L: '0', ap: '0', f: 'GTC', ps: 'BOTH', R: false, wt: 'CONTRACT_PRICE', ...o } });
ws(PRIVATE, trade({ X: 'NEW' }));
ws(PRIVATE, trade({ X: 'PARTIALLY_FILLED', z: '0.4', l: '0.4', L: '2999.5', ap: '2999.5' }));
ws(PRIVATE, trade({ X: 'FILLED', z: '1', l: '0.6', L: '2999', ap: '2999.2' }));
// Duplicate terminal event across the two supported event names: Rust emits no second fill.
ws(PRIVATE, trade({ X: 'FILLED', z: '1', l: '0.6', L: '2999', ap: '2999.2' }, 'TRADE_UPDATE'));
// Native numeric execution IDs exceed JS's safe-integer range.
ws(PRIVATE, trade({ s: 'BTC-USDT', i: '@8389765519993909999', o: 'MARKET', X: 'FILLED', q: '0.002', z: '0.002', l: '0.002', p: '95000', L: '95000', ap: '95000' }));
command({ op: 'placeOrders', orders: [{ symbol: 'BTCUSDT', type: 'limit', side: 'buy', amount: 0.0021, price: 90500.04, clientOrderId: 'caller-lowercase' }] });
http('POST', '/openApi/swap/v2/trade/order', { symbol: 'BTC-USDT', positionSide: 'BOTH', side: 'BUY', type: 'LIMIT', quantity: 0.0021, price: 90500, timeInForce: 'GTC', clientOrderId: 'caller-lowercase' }, ok({ order: native('1003', 'BTC-USDT', 'BUY', 'LIMIT', 0.0021, 90500, { clientOrderId: 'caller-lowercase' }) }));
ws(PRIVATE, trade({ s: 'BTC-USDT', i: 1003, c: 'caller-lowercase', X: 'NEW', q: '0.0021', p: '90500' }));
command({ op: 'cancelOrders', ids: ['1003'] });
http('DELETE', '/openApi/swap/v2/trade/order', { symbol: 'BTC-USDT', orderId: '1003' }, ok({ orderId: '1003', status: 'CANCELED' }));
command({ op: 'cancelSymbolOrders', symbol: 'ETHUSDT' });
http('DELETE', '/openApi/swap/v2/trade/allOpenOrders', { symbol: 'ETH-USDT' }, ok({ success: true }));
ws(PRIVATE, 'Ping');
// Let both polling loops finish a second authoritative order snapshot.
while (clock < 14_000) ws(PRIVATE, 'Pong');
// A final resting trigger after periodic reconciliation, preserving full-position semantics.
ws(PRIVATE, trade({ i: '2001', S: 'SELL', o: 'STOP_MARKET', X: 'NEW', q: '0', p: '0', sp: '2900', cp: true, wt: 'MARK_PRICE' }));
ws(PRIVATE, { e: 'ACCOUNT_UPDATE', E: 1760880015000, a: { B: [{ a: 'USDT', wb: '4000', cw: '4000' }], P: [{ s: 'BTC-USDT', pa: '0.02', ep: '95000', up: '20', ps: 'BOTH', mt: 'cross', liqp: '80000.5' }] } });
http('GET', '/openApi/swap/v2/user/positions', {}, ok([
  { symbol: 'BTC-USDT', positionId: '1785561029271568384', positionSide: 'BOTH', positionAmt: '0.02', avgPrice: '95000', leverage: 20, isolated: false, unrealizedProfit: '20', initialMargin: '48', liquidationPrice: '80000.5' },
]));
records.at(-1).availableAfter = clock;
ws(PRIVATE, 'Pong');

mkdirSync(dirname(values.out), { recursive: true });
records.sort((a, b) => a.t - b.t);
const rawIds = (text) => text.replace(/"@(\d+)"/g, '$1').replace(/\\"@(\d+)\\"/g, '$1');
writeFileSync(values.out, `${records.map((r) => rawIds(JSON.stringify(r))).join('\n')}\n`);
console.log(JSON.stringify({ out: values.out, records: records.length }));
