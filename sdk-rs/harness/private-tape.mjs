// Write a deterministic Binance USD-M private tape from documented payloads:
// startup reads, user-stream events and user commands with their responses.
// Replay it with --paced so both implementations see the same timing.
//
//   corepack yarn rust:private-tape --out sdk-rs/harness/tapes/binance-private.jsonl
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname } from 'node:path';
import { parseArgs } from 'node:util';

const { values } = parseArgs({ options: { out: { type: 'string', default: 'sdk-rs/harness/tapes/binance-private.jsonl' } } });

const REST = 'https://fapi.binance.com';
const LISTEN_KEY = 'pqia91ma19a5s61cv6a81va65sdf19v8a65a1a5s61cv6a81va65sdf19v8a65a1';
const PRIVATE = `wss://fstream.binance.com/private/ws?listenKey=${LISTEN_KEY}`;
const MARKET = 'wss://fstream.binance.com/market/ws';

const filters = (tick, step, max) => [
  { filterType: 'PRICE_FILTER', tickSize: tick, minPrice: '0.10', maxPrice: '4529764' },
  { filterType: 'LOT_SIZE', stepSize: step, minQty: step, maxQty: '1000' },
  { filterType: 'MARKET_LOT_SIZE', stepSize: step, minQty: step, maxQty: max },
];
const symbol = (name, base, tick, step, max) => ({
  symbol: name, status: 'TRADING', baseAsset: base, quoteAsset: 'USDT', marginAsset: 'USDT',
  contractType: 'PERPETUAL', filters: filters(tick, step, max),
});
const SYMBOLS = [symbol('BTCUSDT', 'BTC', '0.10', '0.001', '120'), symbol('ETHUSDT', 'ETH', '0.01', '0.001', '2000')];

const records = [];
const http = (method, path, query, body, status = 200) =>
  records.push({ t: 0, type: 'http', method, url: `${REST}${path}${query ? `?${query}` : ''}`, status, body: JSON.stringify(body) });
let clock = 0;
/** Frames and commands 400 ms apart: past the TS client's 3 RPS limiter. */
const at = () => (clock += 400);
const ws = (url, event) => records.push({ t: at(), type: 'ws', url, text: JSON.stringify(event) });
const command = (body) => records.push({ t: at(), type: 'command', ...body });

http('GET', '/fapi/v1/exchangeInfo', '', { timezone: 'UTC', symbols: SYMBOLS });
http('GET', '/fapi/v1/leverageBracket', '', [
  { symbol: 'BTCUSDT', brackets: [{ bracket: 1, initialLeverage: 125, notionalCap: 50000, notionalFloor: 0, maintMarginRatio: 0.004, cum: 0 }] },
  { symbol: 'ETHUSDT', brackets: [{ bracket: 1, initialLeverage: 100, notionalCap: 10000, notionalFloor: 0, maintMarginRatio: 0.005, cum: 0 }] },
]);
http('GET', '/fapi/v1/ticker/24hr', '', [
  { symbol: 'BTCUSDT', lastPrice: '96000.0', priceChangePercent: '1.2', volume: '1000', quoteVolume: '96000000' },
  { symbol: 'ETHUSDT', lastPrice: '3000.00', priceChangePercent: '-0.5', volume: '50000', quoteVolume: '150000000' },
]);
http('GET', '/fapi/v1/ticker/bookTicker', '', [
  { symbol: 'BTCUSDT', bidPrice: '95999.9', askPrice: '96000.0' },
  { symbol: 'ETHUSDT', bidPrice: '2999.99', askPrice: '3000.00' },
]);
http('GET', '/fapi/v1/premiumIndex', '', [
  { symbol: 'BTCUSDT', markPrice: '96000.0', indexPrice: '95990.0', lastFundingRate: '0.0001', nextFundingTime: 1760889600000 },
  { symbol: 'ETHUSDT', markPrice: '3000.00', indexPrice: '2999.50', lastFundingRate: '0.0001', nextFundingTime: 1760889600000 },
]);
http('POST', '/fapi/v1/listenKey', '', { listenKey: LISTEN_KEY });
http('GET', '/fapi/v1/positionSide/dual', '', { dualSidePosition: false });
http('GET', '/fapi/v3/positionRisk', '', [
  {
    symbol: 'BTCUSDT', positionSide: 'BOTH', positionAmt: '0.010', entryPrice: '95000.0', breakEvenPrice: '95010.0',
    markPrice: '96000.0', unRealizedProfit: '10.00000000', liquidationPrice: '80000.5', isolatedMargin: '0', notional: '960.0',
    marginAsset: 'USDT', isolatedWallet: '0', initialMargin: '48', maintMargin: '3.84', positionInitialMargin: '48',
    openOrderInitialMargin: '0', adl: 1, bidNotional: '0', askNotional: '0', updateTime: 1760880000000,
  },
]);
http('GET', '/fapi/v1/symbolConfig', '', [
  { symbol: 'BTCUSDT', marginType: 'CROSSED', isAutoAddMargin: false, leverage: 20, maxNotionalValue: '1000000' },
  { symbol: 'ETHUSDT', marginType: 'ISOLATED', isAutoAddMargin: false, leverage: 10, maxNotionalValue: '1000000' },
]);
http('GET', '/fapi/v3/account', '', {
  totalInitialMargin: '48', totalMaintMargin: '3.84', totalWalletBalance: '5000', totalUnrealizedProfit: '10',
  totalMarginBalance: '5010', availableBalance: '4952', maxWithdrawAmount: '4952',
  assets: [
    { asset: 'USDT', walletBalance: '4000.0', unrealizedProfit: '10.0', marginBalance: '4010.0', maintMargin: '3.84', initialMargin: '48.0', positionInitialMargin: '48', openOrderInitialMargin: '0', crossWalletBalance: '4000', crossUnPnl: '10', availableBalance: '3952.0', maxWithdrawAmount: '3952', updateTime: 1760880000000 },
    { asset: 'USDC', walletBalance: '1000.0', unrealizedProfit: '0.0', marginBalance: '1000.0', maintMargin: '0', initialMargin: '0', availableBalance: '1000.0', updateTime: 1760880000000 },
    { asset: 'BTC', walletBalance: '0.01', unrealizedProfit: '0.0', marginBalance: '0.01', maintMargin: '0', initialMargin: '0', availableBalance: '0.01', updateTime: 1760880000000 },
  ],
  positions: [],
});
http('GET', '/fapi/v1/openOrders', '', [
  {
    avgPrice: '0.00000', clientOrderId: 'resting', cumQuote: '0', executedQty: '0.001', orderId: '@8389765519993909012',
    origQty: '0.004', origType: 'LIMIT', price: '90000.0', reduceOnly: false, side: 'BUY', positionSide: 'BOTH',
    status: 'PARTIALLY_FILLED', stopPrice: '0', closePosition: false, symbol: 'BTCUSDT', time: 1760880000000,
    timeInForce: 'GTC', type: 'LIMIT', updateTime: 1760880000000, workingType: 'CONTRACT_PRICE', priceProtect: false,
  },
]);
http('GET', '/fapi/v1/openAlgoOrders', 'algoType=CONDITIONAL', [
  {
    algoId: 2146760, clientAlgoId: 'website', algoType: 'CONDITIONAL', orderType: 'STOP_MARKET', symbol: 'BTCUSDT',
    side: 'SELL', positionSide: 'BOTH', timeInForce: 'GTC', quantity: '0', algoStatus: 'NEW', triggerPrice: '85000.0',
    price: '0', workingType: 'MARK_PRICE', reduceOnly: false, closePosition: true, createTime: 1760880000000,
  },
]);

// A market-data frame so the replay has a public socket to wait on.
ws(MARKET, [{ e: '24hrTicker', E: 1, s: 'ETHUSDT', p: '-15', P: '-0.5', c: '3001.00', v: '50001', q: '150000001' }]);

const order = (o) => ({
  e: 'ORDER_TRADE_UPDATE', E: 1760880001000, T: 1760880001000,
  o: { f: 'GTC', ap: '0', sp: '0', x: 'NEW', l: '0', z: '0', L: '0', n: '0', N: 'USDT', T: 1760880001000, t: 0, b: '0', a: '0', m: false, R: false, wt: 'CONTRACT_PRICE', ps: 'BOTH', cp: false, rp: '0', pP: false, si: 0, ss: 0, V: 'NONE', pm: 'NONE', gtd: 0, ...o },
});
const position = (s, pa, ep, up, mt = 'cross', iw = '0') => ({
  e: 'ACCOUNT_UPDATE', E: 1760880001000, T: 1760880001000,
  a: { m: 'ORDER', B: [{ a: 'USDT', wb: '4000', cw: '4000', bc: '0' }], P: [{ s, pa, ep, cr: '0', up, mt, iw, ps: 'BOTH' }] },
});
const algo = (o) => ({
  e: 'ALGO_UPDATE', T: 1760880001000, E: 1760880001000,
  o: { caid: 'x', at: 'CONDITIONAL', ps: 'BOTH', f: 'GTC', p: '0', R: false, cp: false, pP: false, V: 'NONE', wt: 'MARK_PRICE', pm: 'NONE', ...o },
});

ws(PRIVATE, position('BTCUSDT', '0.010', '95000.0', '12.5'));
const eth = { s: 'ETHUSDT', c: 'web', S: 'BUY', o: 'LIMIT', q: '1.000', p: '3000.00', i: 1001, ot: 'LIMIT' };
ws(PRIVATE, order({ ...eth, X: 'NEW' }));
ws(PRIVATE, order({ ...eth, t: 10001, x: 'TRADE', X: 'PARTIALLY_FILLED', l: '0.400', z: '0.400', L: '2999.50', ap: '2999.50' }));
ws(PRIVATE, order({ ...eth, t: 10002, x: 'TRADE', X: 'FILLED', l: '0.600', z: '1.000', L: '2999.00', ap: '2999.20' }));
ws(PRIVATE, algo({ aid: 2001, s: 'ETHUSDT', S: 'SELL', o: 'STOP_MARKET', X: 'NEW', q: '0', tp: '2900.00', cp: true }));
const maker = { s: 'ETHUSDT', c: 'maker', S: 'SELL', o: 'LIMIT', f: 'GTX', q: '0.500', p: '2999.00', i: 1002, ot: 'LIMIT' };
ws(PRIVATE, order({ ...maker, X: 'NEW' }));
ws(PRIVATE, order({ ...maker, x: 'EXPIRED', X: 'EXPIRED', r: 'GTX_ORDER_REJECT' }));
const big = { s: 'BTCUSDT', c: 'big', S: 'SELL', o: 'LIMIT', q: '0.002', p: '99000.0', i: '@8389765519993909999', ot: 'LIMIT', R: true };
ws(PRIVATE, order({ ...big, X: 'NEW' }));

// User commands. Responses are keyed by the canonical request, so a request
// whose parameters differ between implementations goes unserved.
command({ op: 'placeOrders', orders: [{ symbol: 'BTCUSDT', type: 'limit', side: 'buy', amount: 0.0021, price: 90500.04 }] });
http('POST', '/fapi/v1/order', 'symbol=BTCUSDT&positionSide=BOTH&side=BUY&type=LIMIT&quantity=0.002&price=90500&timeInForce=GTC',
  { orderId: 1003, symbol: 'BTCUSDT', status: 'NEW', clientOrderId: 'x', price: '90500', origQty: '0.002' });
ws(PRIVATE, order({ s: 'BTCUSDT', c: 'x', S: 'BUY', o: 'LIMIT', q: '0.002', p: '90500.0', i: 1003, ot: 'LIMIT', X: 'NEW' }));
command({ op: 'cancelOrders', ids: ['1003'] });
http('DELETE', '/fapi/v1/order', 'symbol=BTCUSDT&orderId=1003', { orderId: 1003, symbol: 'BTCUSDT', status: 'CANCELED', origQty: '0.002', executedQty: '0' });
command({ op: 'cancelSymbolOrders', symbol: 'ETHUSDT' });
http('DELETE', '/fapi/v1/allOpenOrders', 'symbol=ETHUSDT', { code: 200, msg: 'The operation of cancel all open order is done.' });
http('DELETE', '/fapi/v1/algoOpenOrders', 'symbol=ETHUSDT', { code: 200, msg: 'success' });
ws(PRIVATE, position('ETHUSDT', '1.000', '2999.20', '1.80', 'isolated', '301.72'));

mkdirSync(dirname(values.out), { recursive: true });
records.sort((a, b) => a.t - b.t);
// Ids past 2^53 are written as raw JSON numbers, as Binance sends them.
const raw = (text) => text.replace(/"@(\d+)"/g, '$1').replace(/\\"@(\d+)\\"/g, '$1');
writeFileSync(values.out, `${records.map((r) => raw(JSON.stringify(r))).join('\n')}\n`);
console.log(JSON.stringify({ out: values.out, records: records.length }));
