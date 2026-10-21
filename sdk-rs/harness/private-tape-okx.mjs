// Deterministic examples of OKX v5 instruments, account-position-risk,
// positions, orders, algo orders and command responses. Sizes are contracts;
// execution fillSz converts to base units using ctVal, not lotSz.
// Schemas: https://www.okx.com/docs-v5/en/#order-book-trading-trade
//         https://www.okx.com/docs-v5/en/#trading-account-rest-api
// Run: corepack yarn rust:private-tape-okx --out <file> [--venue okxeudemo]
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname } from 'node:path';
import { parseArgs } from 'node:util';
import { venueOf } from './venues.mjs';

const { values } = parseArgs({ options: {
  out: { type: 'string' },
  venue: { type: 'string', default: 'okx' },
} });
venueOf(values.venue);
if (!['okx', 'okxeu', 'okxdemo', 'okxeudemo'].includes(values.venue)) throw new Error('Expected an OKX venue');
const out = values.out ?? `sdk-rs/harness/tapes/${values.venue}-private.jsonl`;
const eea = values.venue.startsWith('okxeu');
const demo = values.venue.endsWith('demo');
const REST = eea ? 'https://eea.okx.com' : demo ? 'https://openapi.okx.com' : 'https://www.okx.com';
const WS = `wss://${eea ? (demo ? 'wseeapap' : 'wseea') : (demo ? 'wspap' : 'ws')}.okx.com/ws/v5`;
const BROKER = 'e847386590ce4dBC';
const GENERATED = `${BROKER}0000000000000000`;
const BTC = 'BTC-USDT-SWAP';
const ETH = 'ETH-USDC-SWAP';
const INVERSE = 'BTC-USD-SWAP';
const XPERP = 'BTC-USD_UM_XPERP-310404';
const records = [];
let clock = 0;
let commandCount = 0;
const envelope = (data) => ({ code: '0', msg: '', data });
const http = (method, path, response, requestBody, metadata = {}) => {
  const record = (path, data) => records.push({
    t: 0, type: 'http', method, url: `${REST}${path}`, status: 200,
    ...metadata, ...(requestBody === undefined ? {} : { requestBody }), body: JSON.stringify(envelope(data)),
  });
  if (eea && method === 'GET' && /^\/api\/v5\/(account\/(account-position-risk|positions)|trade\/(orders-pending|orders-algo-pending))\?/.test(path)) {
    const native = new URL(REST + path);
    // Collateral is account currency scope; both readers use the SWAP risk read.
    if (native.pathname.endsWith('/account-position-risk')) {
      if (native.searchParams.get('instType') !== 'SWAP') throw new Error(`Unexpected OKX collateral scope ${path}`);
      record(path, response);
      return;
    }
    // Retain each native product request exactly; unfiltered requests must fail.
    if (response.some((row) => !['SWAP', 'FUTURES'].includes(row.instType)))
      throw new Error(`Missing native OKX product in ${path}`);
    for (const instType of ['SWAP', 'FUTURES']) {
      native.searchParams.set('instType', instType);
      record(native.pathname + native.search, response.filter((row) => row.instType === instType));
    }
    return;
  }
  record(path, response);
};
const ws = (socket, event) => records.push({ t: clock += 400, type: 'ws', url: `${WS}/${socket}`, text: JSON.stringify(event) });
const command = (args) => {
  records.push({ t: clock += 400, type: 'command', ...args });
  return ++commandCount;
};
const push = (socket, channel, rows) => {
  if (channel === 'account') return ws(socket, { arg: { channel }, data: rows });
  if (socket === 'public') {
    for (const row of rows) ws(socket, { arg: { channel, instId: row.instId }, data: [row] });
    return;
  }
  for (const instType of ['SWAP', 'FUTURES']) {
    const product = rows.filter((row) => row.instType === instType);
    if (product.length) ws(socket, { arg: { channel, instType }, data: product });
  }
  if (rows.some((row) => !['SWAP', 'FUTURES'].includes(row.instType))) throw new Error(`Missing native OKX product in ${channel}`);
};
const instrument = (instId, base, quote, ctVal, tickSz) => ({
  instType: 'SWAP', instId, uly: `${base}-${quote}`, instFamily: `${base}-${quote}`,
  ctType: 'linear', ctVal, ctValCcy: base, settleCcy: quote, baseCcy: '', quoteCcy: '',
  state: 'live', lotSz: '0.01', minSz: '0.01', tickSz, lever: '100',
  maxIcebergSz: '1000', maxLmtSz: '1000', maxMktSz: '1000', maxStopSz: '1000', maxTriggerSz: '1000', maxTwapSz: '1000',
});
const inverseInstrument = { ...instrument(INVERSE, 'BTC', 'USD', '100', '0.1'), ctType: 'inverse', ctValCcy: 'USD', settleCcy: 'BTC' };
const xperpInstrument = { ...instrument(XPERP, 'BTC', 'USD', '0.0001', '0.1'), instType: 'FUTURES', ruleType: 'xperp' };
const instruments = [instrument(BTC, 'BTC', 'USDT', '0.01', '0.1'), instrument(ETH, 'ETH', 'USDC', '0.1', '0.01'), inverseInstrument];
http('GET', `/api/v5/${eea ? 'account' : 'public'}/instruments?instType=SWAP`, instruments);
if (eea) http('GET', '/api/v5/account/instruments?instType=FUTURES', [xperpInstrument,
  { ...xperpInstrument, instId: 'BTC-USD-261225', ruleType: 'normal' }]);
http('GET', '/api/v5/market/tickers?instType=SWAP', [
  { instId: INVERSE, last: '40000', open24h: '40000', bidPx: '39999.9', askPx: '40000.1', volCcy24h: '100', vol24h: '10000' },
  { instId: BTC, last: '40000', open24h: '40000', bidPx: '39999.9', askPx: '40000.1', volCcy24h: '100', vol24h: '10000' },
  { instId: ETH, last: '3000', open24h: '3000', bidPx: '2999.99', askPx: '3000.01', volCcy24h: '1000', vol24h: '10000' },
]);
if (eea) http('GET', '/api/v5/market/tickers?instType=FUTURES', [{ instId: XPERP, last: '40000', open24h: '40000', bidPx: '39999.9', askPx: '40000.1', volCcy24h: '100', vol24h: '10000' }]);
http('GET', '/api/v5/account/config', [{ posMode: 'net_mode', acctLv: '2', uid: '123456', autoLoan: false }]);
http('GET', '/api/v5/account/account-position-risk?instType=SWAP', [{
  ts: '1760880000000',
  balData: [{ ccy: 'USDT', eq: '4000', cashBal: '3950', disEq: '4000' }, { ccy: 'USDC', eq: '1000', cashBal: '970', disEq: '1000' }, { ccy: 'BTC', eq: '1', cashBal: '1', disEq: '40000' }, ...(eea ? [{ ccy: 'USD', eq: '100', cashBal: '100', disEq: '100' }] : [])],
  posData: [],
}]);
const position = (instId, pos, avgPx, lever, upl, imr, mmr, ccy) => ({
  instType: 'SWAP', instId, posId: `${instId}-position`, posSide: 'net', pos, avgPx, lever, upl, imr, mmr, ccy,
  markPx: instId === ETH ? '3000' : '40000', liqPx: instId === ETH ? '2000' : '30000',
  mgnMode: 'cross', margin: '', notionalUsd: instId === BTC ? '2000' : '3000', uTime: '1760880000000',
});
const inversePosition = { ...position(INVERSE, '5', '39000', '10', '0.0003205128205128205', '0.00125', '0.000125', 'BTC'), markPx: '40000', liqPx: '30000', notionalUsd: '500' };
const xperpPosition = { ...position(XPERP, '10', '39000', '10', '1', '4', '0.4', 'USD'), instType: 'FUTURES', markPx: '40000', liqPx: '30000', notionalUsd: '40' };
const productPositions = [inversePosition, ...(eea ? [xperpPosition] : [])];
const positions = [position(BTC, '5', '39000', '10', '50', '200', '20', 'USDT'), position(ETH, '10', '2900', '20', '100', '150', '15', 'USDC'), ...productPositions];
http('GET', '/api/v5/account/positions?instType=SWAP', positions);
const order = (fields) => ({
  instType: 'SWAP', instId: BTC, ordId: '101', algoId: '', clOrdId: 'resting101', tag: BROKER,
  ordType: 'limit', side: 'buy', posSide: 'net', tdMode: 'cross', state: 'live',
  sz: '4', accFillSz: '1', px: '38000', avgPx: '38000', lever: '10', ccy: 'USDT', reduceOnly: 'false',
  fillSz: '0', fillPx: '', tradeId: '', fillTime: '', uTime: '1760880000000', ...fields,
});
const algo = (fields) => ({
  instType: 'SWAP', instId: ETH, algoId: '201', algoClOrdId: 'website', ordId: '',
  ordType: 'conditional', side: 'sell', posSide: 'net', tdMode: 'cross', state: 'live',
  sz: '2', reduceOnly: 'true', lever: '20', ccy: 'USDC', last: '3000',
  tpTriggerPx: '', slTriggerPx: '', tpOrdPx: '-1', slOrdPx: '-1',
  tpTriggerPxType: 'last', slTriggerPxType: 'mark', cTime: '1760880000000', uTime: '1760880000000', ...fields,
});
const oco = algo({ algoId: '201', ordType: 'oco', tpTriggerPx: '3200', slTriggerPx: '2800' });
const stop = algo({ algoId: '202', slTriggerPx: '2850' });
const trigger = algo({ algoId: '203', ordType: 'trigger', side: 'buy', reduceOnly: 'false', triggerPx: '3100', triggerPxType: 'last', ordPx: '-1' });
const trailing = algo({ algoId: '204', ordType: 'move_order_stop', callbackSpread: '100', moveTriggerPx: '100' });
const inverseOrder = order({ instId: INVERSE, ordId: 'inverse-101', clOrdId: 'inverse101', sz: '4', accFillSz: '1', px: '40000', avgPx: '40000', ccy: 'BTC' });
const xperpOrder = order({ instType: 'FUTURES', instId: XPERP, ordId: 'xperp-101', clOrdId: 'xperp101', sz: '10', accFillSz: '2', px: '40000', avgPx: '40000', ccy: 'USD' });
const productOrders = [inverseOrder, ...(eea ? [xperpOrder] : [])];
http('GET', '/api/v5/trade/orders-pending?instType=SWAP', [order({}), ...productOrders]);
for (const [kind, rows] of [['oco', [oco]], ['conditional', [stop]], ['trigger', [trigger]], ['move_order_stop', [trailing]]]) {
  http('GET', `/api/v5/trade/orders-algo-pending?instType=SWAP&ordType=${kind}`, rows);
}

ws('private', { event: 'login', code: '0', msg: '' });
ws('business', { event: 'login', code: '0', msg: '' });
push('public', 'mark-price', [{ instId: BTC, markPx: '40000' }, { instId: ETH, markPx: '3000' }, { instId: INVERSE, markPx: '40000' }, ...(eea ? [{ instId: XPERP, markPx: '40000' }] : [])]);
push('business', 'orders-algo', [oco, stop]);
push('business', 'algo-advance', [trigger, trailing]);

// Two executions, each delivered twice: expected amounts are 0.01 and 0.02 BTC.
const partial = order({ state: 'partially_filled', accFillSz: '2', fillSz: '1', fillPx: '38000', tradeId: '7001', fillTime: '1760880001000', uTime: '1760880001000' });
const filled = order({ state: 'filled', accFillSz: '4', fillSz: '2', fillPx: '37999.9', tradeId: '7002', fillTime: '1760880002000', uTime: '1760880002000' });
for (const row of [partial, partial, filled, filled]) push('private', 'orders', [row]);

command({ op: 'placeOrders', orders: [{ symbol: 'BTCUSDT', type: 'limit', side: 'buy', amount: 2.1, price: 38500.06, clientOrderId: 'OKXCaller123' }] });
http('POST', '/api/v5/trade/batch-orders', [{ ordId: '301', clOrdId: 'OKXCaller123', sCode: '0', sMsg: '' }], [
  { instId: BTC, tag: BROKER, tdMode: 'cross', side: 'buy', ordType: 'limit', px: '38500.1', clOrdId: 'OKXCaller123', sz: '2.1' },
]);
push('private', 'orders', [order({ ordId: '301', clOrdId: 'OKXCaller123', sz: '2.1', accFillSz: '0', px: '38500.1' })]);
command({ op: 'amendOrder', id: '301', price: 38600.1, amount: 3.2 });
http('POST', '/api/v5/trade/amend-order', [{ ordId: '301', sCode: '0', sMsg: '' }], { instId: BTC, ordId: '301', newPx: '38600.1', newSz: '3.2' });
push('private', 'orders', [order({ ordId: '301', clOrdId: 'OKXCaller123', sz: '3.2', accFillSz: '0', px: '38600.1', uTime: '1760880003000' })]);
command({ op: 'cancelOrders', ids: ['301'] });
http('POST', '/api/v5/trade/cancel-batch-orders', [{ ordId: '301', sCode: '0', sMsg: '' }], [{ instId: BTC, ordId: '301' }]);
push('private', 'orders', [order({ ordId: '301', clOrdId: 'OKXCaller123', sz: '3.2', accFillSz: '0', px: '38600.1', state: 'canceled', uTime: '1760880004000' })]);

command({ op: 'placeOrders', orders: [{ symbol: 'ETHUSDC', type: 'conditional', side: 'buy', amount: 2, triggerPrice: 3100, triggerType: 'last_price' }] });
http('POST', '/api/v5/trade/order-algo', [{ algoId: '302', algoClOrdId: GENERATED, sCode: '0', sMsg: '' }], {
  instId: ETH, tdMode: 'cross', algoClOrdId: GENERATED, tag: BROKER, side: 'buy', ordType: 'trigger',
  sz: '2', triggerPx: '3100', orderPx: '-1', triggerPxType: 'last',
});
const placedTrigger = algo({ algoId: '302', ordType: 'trigger', side: 'buy', reduceOnly: 'false', triggerPx: '3100', triggerPxType: 'last', ordPx: '-1', sz: '2' });
push('business', 'algo-advance', [placedTrigger]);
command({ op: 'amendOrder', id: '302', price: 3150, amount: 3 });
http('POST', '/api/v5/trade/amend-algos', [{ algoId: '302', sCode: '0', sMsg: '' }], {
  instId: ETH, algoId: '302', newTriggerPx: '3150', newOrdPx: '-1', newTriggerPxType: 'last', newSz: '3',
});
push('business', 'orders-algo', [{ ...placedTrigger, triggerPx: '3150', sz: '3', uTime: '1760880005000' }]);
command({ op: 'cancelOrders', ids: ['302'] });
http('POST', '/api/v5/trade/cancel-algos', [{ algoId: '302', sCode: '0', sMsg: '' }], [{ instId: ETH, algoId: '302' }]);
push('business', 'orders-algo', [{ ...placedTrigger, triggerPx: '3150', sz: '3', state: 'canceled', uTime: '1760880006000' }]);

command({ op: 'fetchPublicTrades', symbol: 'BTCUSDT', limit: 2 });
http('GET', '/api/v5/market/trades?instId=BTC-USDT-SWAP&limit=2', [
  { instId: BTC, tradeId: '8001', px: '40000', sz: '5', ts: '1760880007000', side: 'buy' },
  { instId: BTC, tradeId: '8002', px: '39999.9', sz: '2', ts: '1760880007001', side: 'sell' },
]);

// A later positions snapshot must not leak into startup or the login resync.
const leverageCommand = command({ op: 'setLeverage', symbol: 'BTCUSDT', leverage: 12 });
http('POST', '/api/v5/account/set-leverage', [{ instId: BTC, lever: '12', mgnMode: 'cross', posSide: 'net' }],
  { instId: BTC, lever: '12', mgnMode: 'cross' });
http('GET', '/api/v5/account/positions?instType=SWAP', [
  { ...positions[0], lever: '12', imr: '166.66666666666666', uTime: '1760880007000' }, positions[1], ...productPositions,
], undefined, { afterCommand: leverageCommand });

// Native contract-count writes and per-execution base conversion on products
// formerly filtered from the Rust lane. Caller IDs remain exact on the wire.
const productLifecycle = (instId, symbol, id, clientOrderId, ccy, instType) => {
  command({ op: 'placeOrders', orders: [{ symbol, type: 'limit', side: 'buy', amount: 2, price: 40000, clientOrderId }] });
  http('POST', '/api/v5/trade/batch-orders', [{ ordId: id, clOrdId: clientOrderId, sCode: '0', sMsg: '' }], [
    { instId, tag: BROKER, tdMode: 'cross', side: 'buy', ordType: 'limit', px: '40000', clOrdId: clientOrderId, sz: '2' },
  ]);
  const row = order({ instId, instType, ordId: id, clOrdId: clientOrderId, sz: '2', accFillSz: '0', px: '40000', avgPx: '40000', ccy, uTime: '1760880011000' });
  push('private', 'orders', [row]);
  command({ op: 'amendOrder', id, price: 40100, amount: 3 });
  http('POST', '/api/v5/trade/amend-order', [{ ordId: id, sCode: '0', sMsg: '' }], { instId, ordId: id, newPx: '40100', newSz: '3' });
  const partial = { ...row, sz: '3', px: '40100', avgPx: '40000', state: 'partially_filled', accFillSz: '1', fillSz: '1', fillPx: '40000', fillTime: '1760880012000', tradeId: `${id}1`, uTime: '1760880012000' };
  push('private', 'orders', [partial]);
  command({ op: 'cancelOrders', ids: [id] });
  http('POST', '/api/v5/trade/cancel-batch-orders', [{ ordId: id, sCode: '0', sMsg: '' }], [{ instId, ordId: id }]);
  push('private', 'orders', [{ ...partial, state: 'canceled', fillSz: '0', uTime: '1760880013000' }]);
};
productLifecycle(INVERSE, 'BTCUSD', '1301', 'OKXInverse123', 'BTC', 'SWAP');
if (eea) productLifecycle(XPERP, 'BTCUSD_UM_XPERP310404', '2301', 'OKXXperp123', 'USD', 'FUTURES');

// Fills exercise the recent/history overlap, distinct executions of one order,
// seconds input bounds, contract-to-base conversion and optional fee/PnL fields.
const historicalFill = {
  instId: BTC, tradeId: '9001', billId: '9101', ordId: '101', side: 'buy',
  fillPx: '38000', fillSz: '1', fillTime: '1760880001000', ts: '1760880001001',
  fee: '-0.38', feeCcy: 'USDT', fillPnl: '0', execType: 'T', posSide: 'net',
};
command({ op: 'fetchFills', symbol: 'BTCUSDT', from: 1760879999, to: 1760880100, limit: 3 });
const fillsQuery = 'instType=SWAP&instId=BTC-USDT-SWAP&begin=1760879999000&end=1760880100000&limit=3';
http('GET', `/api/v5/trade/fills?${fillsQuery}`, [historicalFill]);
http('GET', `/api/v5/trade/fills-history?${fillsQuery}`, [historicalFill, {
  ...historicalFill, tradeId: '9002', billId: '9102', fillSz: '2', fillPx: '37999.9',
  fillTime: '1760880002000', fee: '-0.759998', fillPnl: '1', execType: 'M',
}]);

command({ op: 'fetchOrderHistory', symbol: 'BTCUSDT', from: 1760879999000, to: 1760880100000, limit: 2 });
http('GET', '/api/v5/trade/orders-history?instType=SWAP&instId=BTC-USDT-SWAP&begin=1760879999000&end=1760880100000&limit=100', [
  order({ state: 'filled', cTime: '1760880000000', uTime: '1760880002000', accFillSz: '4' }),
  order({ ordId: '303', state: 'canceled', cTime: '1760880000001', uTime: '1760880003000', sz: '2', accFillSz: '0', avgPx: '', px: '39000' }),
]);

command({ op: 'fetchIncome', from: 1760879999000, to: 1760880100000, limit: 2 });
http('GET', '/api/v5/account/bills-archive?instType=SWAP&begin=1760879999000&end=1760880100000&limit=100', [
  { instId: BTC, billId: '9201', tradeId: '9002', type: '2', pnl: '1', fee: '-0.759998', balChg: '2.240002', ccy: 'USDT', ts: '1760880003000' },
  { instId: ETH, billId: '9202', tradeId: '', type: '8', pnl: '0', fee: '0', balChg: '-0.2', ccy: 'USDC', ts: '1760880004000' },
]);

if (eea) http('GET', '/api/v5/account/bills-archive?instType=FUTURES&begin=1760879999000&end=1760880100000&limit=100', [
  { instId: XPERP, billId: '9203', tradeId: 'future-fill', type: '2', pnl: '2', fee: '-0.1', balChg: '1.9', ccy: 'USD', ts: '1760880005000' },
]);

// The retained products use the same real TS native lot/control paths.
for (const [index, row] of productOrders.entries()) {
  const native = row.instId, symbol = native.replace(/-SWAP$/, '').replaceAll('-', '');
  const client = `ProductCaller${index}`, id = String(401 + index), instType = row.instType;
  const price = index === 0 ? 50000 : 40000;
  command({ op: 'placeOrders', orders: [{ symbol, type: 'limit', side: 'buy', amount: 3, price, clientOrderId: client }] });
  http('POST', '/api/v5/trade/batch-orders', [{ ordId: id, clOrdId: client, sCode: '0', sMsg: '' }], [
    { instId: native, tag: BROKER, tdMode: 'cross', side: 'buy', ordType: 'limit', px: String(price), clOrdId: client, sz: '3' },
  ]);
  const live = order({ ...row, ordId: id, clOrdId: client, sz: '3', accFillSz: '0', px: String(price), avgPx: '', uTime: '1760880007000' });
  push('private', 'orders', [live]);
  command({ op: 'cancelOrders', ids: [id] });
  http('POST', '/api/v5/trade/cancel-batch-orders', [{ ordId: id, sCode: '0', sMsg: '' }], [{ instId: native, ordId: id }]);
  push('private', 'orders', [{ ...live, state: 'canceled', uTime: '1760880008000' }]);
  const fill = { ...historicalFill, instId: native, tradeId: `product-execution-${index}`, billId: `930${index}`, fillPx: String(price), fillSz: '1', feeCcy: row.ccy, fee: '-0.001', fillPnl: '0.002' };
  command({ op: 'fetchFills', symbol, from: 1760879999, to: 1760880100, limit: 1 });
  http('GET', `/api/v5/trade/fills?instType=${instType}&instId=${native}&begin=1760879999000&end=1760880100000&limit=1`, [fill]);
  command({ op: 'fetchOrderHistory', symbol, from: 1760879999000, to: 1760880100000, limit: 1 });
  http('GET', `/api/v5/trade/orders-history?instType=${instType}&instId=${native}&begin=1760879999000&end=1760880100000&limit=100`, [{ ...live, state: 'canceled', cTime: '1760880000000', avgPx: String(price), accFillSz: '1' }]);
  command({ op: 'fetchIncome', symbol, from: 1760879999000, to: 1760880100000, limit: 1 });
  http('GET', `/api/v5/account/bills-archive?instType=${instType}&instId=${native}&begin=1760879999000&end=1760880100000&limit=100`, [
    { instId: native, billId: `940${index}`, tradeId: fill.tradeId, type: '2', pnl: '0.002', fee: '-0.001', balChg: '0.001', ccy: row.ccy, ts: '1760880009000' },
  ]);
}

// The fixed old end requires the historical route even if recent returns rows.
command({ op: 'fetchCandles', symbol: 'BTCUSDT', interval: '1m', from: 1760879940000, to: 1760880120000, limit: 2 });
const candlesQuery = 'instId=BTC-USDT-SWAP&bar=1m&limit=2&before=1760880000000&after=1760880120000';
const candleRows = [
  ['1760880060000', '40000', '40100', '39900', '40050', '10', '0.1', '4000', '1'],
  ['1760880000000', '39900', '40000', '39800', '40000', '20', '0.2', '8000', '1'],
];
http('GET', `/api/v5/market/candles?${candlesQuery}`, candleRows.slice(0, 1));
http('GET', `/api/v5/market/history-candles?${candlesQuery}`, candleRows);

// Both private pushes now differ from every REST seed: ignoring either handler
// changes the final balance/position tables and fails parity.
push('private', 'positions', [
  { ...position(BTC, '6', '39100', '12', '54', '200', '24', 'USDT'), notionalUsd: '2400', uTime: '1760880010000' },
  { ...position(ETH, '8', '2910', '20', '72', '120', '12', 'USDC'), notionalUsd: '2400', uTime: '1760880010000' },
  ...productPositions,
]);
push('private', 'account', [{ totalEq: eea ? '45400' : '45300', details: [{ ccy: 'USDT', eq: '4100' }, { ccy: 'USDC', eq: '1200' }, { ccy: 'BTC', eq: '1' }, ...(eea ? [{ ccy: 'USD', eq: '100' }] : [])] }]);
mkdirSync(dirname(out), { recursive: true });
records.sort((a, b) => a.t - b.t);
writeFileSync(out, `${records.map((r) => JSON.stringify(r)).join('\n')}\n`);
console.log(JSON.stringify({ out, venue: values.venue, records: records.length, executions: eea ? 4 : 3, fillBaseAmounts: [0.01, 0.02, 0.0025, ...(eea ? [0.0001] : [])] }));
