// Write a deterministic Bybit linear private tape from documented V5 payloads
// (bybit-exchange.github.io/docs/v5): startup reads, private-stream events and
// user commands with their responses. Replay it with the gate.
//
//   corepack yarn rust:private-tape-bybit --out sdk-rs/harness/tapes/bybit-private.jsonl
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname } from 'node:path';
import { parseArgs } from 'node:util';

const { values } = parseArgs({ options: { out: { type: 'string', default: 'sdk-rs/harness/tapes/bybit-private.jsonl' } } });

const REST = 'https://api.bybit.com';
const PRIVATE = 'wss://stream.bybit.com/v5/private';
const PUBLIC = 'wss://stream.bybit.com/v5/public/linear';
const ok = (result, extra = {}) => ({ retCode: 0, retMsg: 'OK', result, retExtInfo: {}, time: 1760880000000, ...extra });

const records = [];
const http = (method, path, query, body, requestBody) =>
  records.push({
    t: 0,
    type: 'http',
    method,
    url: `${REST}${path}${query ? `?${query}` : ''}`,
    ...(requestBody ? { requestBody: JSON.stringify(requestBody) } : {}),
    status: 200,
    body: JSON.stringify(body),
  });
let clock = 0;
/** Frames and commands 400 ms apart. */
const at = () => (clock += 400);
const ws = (url, event) => records.push({ t: at(), type: 'ws', url, text: JSON.stringify(event) });
const command = (body) => records.push({ t: at(), type: 'command', ...body });

// "Get Instruments Info".
const instrument = (symbol, base, tick, step, max) => ({
  symbol, contractType: 'LinearPerpetual', status: 'Trading', baseCoin: base, quoteCoin: 'USDT', launchTime: '1585526400000',
  deliveryTime: '0', deliveryFeeRate: '', priceScale: '2', leverageFilter: { minLeverage: '1', maxLeverage: '100.00', leverageStep: '0.01' },
  priceFilter: { minPrice: tick, maxPrice: '1999999.80', tickSize: tick },
  lotSizeFilter: { maxOrderQty: max, minOrderQty: step, qtyStep: step, postOnlyMaxOrderQty: max, maxMktOrderQty: max, minNotionalValue: '5' },
  unifiedMarginTrade: true, fundingInterval: 480, settleCoin: 'USDT', copyTrading: 'both', isPreListing: false, preListingInfo: null, symbolType: '', tags: [],
});
http('GET', '/v5/market/instruments-info', 'category=linear&limit=1000', ok({
  category: 'linear', list: [instrument('BTCUSDT', 'BTC', '0.10', '0.001', '1190.000'), instrument('ETHUSDT', 'ETH', '0.01', '0.01', '7240.00')], nextPageCursor: '',
}));
http('GET', '/v5/market/instruments-info', 'category=linear&limit=1000&status=PreLaunch', ok({ category: 'linear', list: [], nextPageCursor: '' }));
// "Get Tickers".
const ticker = (symbol, last, bid, ask, volume) => ({
  symbol, lastPrice: last, indexPrice: last, markPrice: last, prevPrice24h: last, price24hPcnt: '0.012', highPrice24h: last, lowPrice24h: last,
  prevPrice1h: last, openInterest: '1000', openInterestValue: '1000000', turnover24h: '96000000', volume24h: volume, fundingRate: '0.0001',
  nextFundingTime: '1760889600000', predictedDeliveryPrice: '', basisRate: '', deliveryFeeRate: '', deliveryTime: '0', ask1Size: '1',
  bid1Price: bid, ask1Price: ask, bid1Size: '1', basis: '', preOpenPrice: '', preQty: '', curPreListingPhase: '', fundingIntervalHour: '8', fundingCap: '0.005',
});
http('GET', '/v5/market/tickers', 'category=linear', ok({
  category: 'linear', list: [ticker('BTCUSDT', '96000.00', '95999.90', '96000.00', '1000'), ticker('ETHUSDT', '3000.00', '2999.99', '3000.00', '50000')],
}));
// "Get Fee Rate", "Get Account Info", "Get API Key Information".
http('GET', '/v5/account/fee-rate', 'category=linear', ok({ list: [
  { symbol: 'BTCUSDT', takerFeeRate: '0.00055', makerFeeRate: '0.0002' },
  { symbol: 'ETHUSDT', takerFeeRate: '0.00055', makerFeeRate: '0.0002' },
] }));
http('GET', '/v5/account/info', '', ok({ marginMode: 'REGULAR_MARGIN', updatedTime: '1697078946000', unifiedMarginStatus: 6, dcpStatus: 'OFF', timeWindow: 10, smpGroup: 0, isMasterTrader: false, spotHedgingStatus: 'OFF' }));
http('GET', '/v5/user/query-api', '', ok({ id: '13770661', note: 'replay', apiKey: 'replay-key', readOnly: 0, vipLevel: 'No VIP', unified: 0, uta: 1, isMaster: true }));

// "Get Open & Closed Orders": a limit buy with attached TP/SL and a reduce-only stop.
const order = (o) => ({
  orderLinkId: '', blockTradeId: '', isLeverage: '', positionIdx: 0, cancelType: 'UNKNOWN', rejectReason: 'EC_NoError', avgPrice: '0',
  leavesValue: '0', cumExecQty: '0', cumExecValue: '0', cumExecFee: '0', timeInForce: 'GTC', orderType: 'Limit', stopOrderType: '',
  orderIv: '', triggerPrice: '', takeProfit: '', stopLoss: '', tpTriggerBy: '', slTriggerBy: '', triggerDirection: 0, triggerBy: '',
  lastPriceOnCreated: '', reduceOnly: false, closeOnTrigger: false, smpType: 'None', smpGroup: '0', smpOrderId: '', tpslMode: '',
  tpLimitPrice: '', slLimitPrice: '', placeType: '', createdTime: '1760880000000', updatedTime: '1760880000000', category: 'linear', ...o,
});
const resting = order({
  orderId: 'fd4300ae-7847-404e-b947-b46980a4d140', orderLinkId: 'test-000005', symbol: 'ETHUSDT', price: '2800.00', qty: '0.10',
  side: 'Buy', orderStatus: 'New', leavesQty: '0.10', takeProfit: '3200.00', stopLoss: '2700.00', tpTriggerBy: 'LastPrice', slTriggerBy: 'MarkPrice',
  tpslMode: 'Full',
});
const stop = order({
  orderId: '2146760a-0000-4000-8000-000000000001', symbol: 'ETHUSDT', price: '0', qty: '0.50', side: 'Sell', orderStatus: 'Untriggered',
  leavesQty: '0.50', orderType: 'Market', stopOrderType: 'Stop', triggerPrice: '2900.00', triggerBy: 'MarkPrice', triggerDirection: 2,
  reduceOnly: true, closeOnTrigger: true, timeInForce: 'IOC',
});
const page = (list) => ok({ list, nextPageCursor: '', category: 'linear' });
http('GET', '/v5/order/realtime', 'category=linear&settleCoin=USDT&limit=50&cursor=', page([resting, stop]));
http('GET', '/v5/order/realtime', 'category=linear&settleCoin=USDC&limit=50&cursor=', page([]));

// "Get Position Info": a one-way BTCUSDT long and a flat ETHUSDT row carrying its leverage.
const position = (p) => ({
  positionIdx: 0, riskId: 1, riskLimitValue: '2000000', side: '', size: '0', avgPrice: '0', positionValue: '0', tradeMode: 0,
  positionStatus: 'Normal', autoAddMargin: 0, adlRankIndicator: 0, leverage: '10', breakEvenPrice: '0', positionBalance: '0',
  markPrice: '96000', liqPrice: '', bustPrice: '', positionMM: '0', positionMMByMp: '0', positionIM: '0', positionIMByMp: '0',
  tpslMode: 'Full', takeProfit: '0.00', stopLoss: '0.00', trailingStop: '0.00', unrealisedPnl: '0', curRealisedPnl: '0',
  cumRealisedPnl: '0', seq: 5723621632, isReduceOnly: false, mmrSysUpdateTime: '', leverageSysUpdatedTime: '', sessionAvgPrice: '',
  createdTime: '1760880000000', updatedTime: '1760880000000', ...p,
});
const btc = position({
  symbol: 'BTCUSDT', side: 'Buy', size: '0.010', avgPrice: '95000', positionValue: '950', liqPrice: '80000.5', positionIM: '95',
  unrealisedPnl: '10', curRealisedPnl: '-0.5',
});
const eth = position({ symbol: 'ETHUSDT', leverage: '20', markPrice: '3000' });
http('GET', '/v5/position/list', 'category=linear&settleCoin=USDT&limit=200&cursor=', page([btc, eth]));
http('GET', '/v5/position/list', 'category=linear&settleCoin=USDC&limit=200&cursor=', page([]));

// "Get Wallet Balance".
http('GET', '/v5/account/wallet-balance', 'accountType=UNIFIED', ok({ list: [{
  totalEquity: '5010.5', accountIMRate: '0.02', accountIMRateByMp: '0.02', totalMarginBalance: '5010.5', totalInitialMargin: '95',
  totalInitialMarginByMp: '95', accountType: 'UNIFIED', totalAvailableBalance: '4915.5', accountMMRate: '0', accountMMRateByMp: '0',
  totalPerpUPL: '10', totalWalletBalance: '5000.5', accountLTV: '0', totalMaintenanceMargin: '3.8', totalMaintenanceMarginByMp: '3.8',
  coin: [{ coin: 'USDT', equity: '5010.5', usdValue: '5010.5', walletBalance: '5000.5', availableToWithdraw: '0', availableToBorrow: '',
    borrowAmount: '0', accruedInterest: '0', totalOrderIM: '0', totalPositionIM: '95', totalPositionMM: '3.8', unrealisedPnl: '10',
    cumRealisedPnl: '0', bonus: '0', collateralSwitch: true, marginCollateral: true, locked: '0', spotHedgingQty: '0', spotBorrow: '0' }],
}] }));

// A market-data frame so the replay has a public socket to wait on ("Ticker", a delta).
ws(PUBLIC, { topic: 'tickers.ETHUSDT', type: 'delta', data: { symbol: 'ETHUSDT', lastPrice: '3001.00', volume24h: '50001' }, cs: 1, ts: 1760880000400 });
// "Authentication": the success acknowledgement.
ws(PRIVATE, { success: true, ret_msg: '', op: 'auth', conn_id: 'cejreaspqfh3sjdnldmg-p' });

const stream = (topic, data) => ({ id: `${topic}-1`, topic, creationTime: 1760880001000, data });
// "Position": the long as the stream reports it.
ws(PRIVATE, stream('position', [{ ...btc, entryPrice: '95000', avgPrice: undefined, seq: 8327597863 }]));

// "Order" and "Execution": a buy filled in two executions.
const fill = order({ orderId: '5cf98598-39a7-459e-97bf-76ca765ee020', orderLinkId: 'web', symbol: 'ETHUSDT', price: '3000.00', qty: '1.00', side: 'Buy', avgPrice: '0' });
const execution = (qty, price, leaves) => stream('execution', [{
  category: 'linear', symbol: 'ETHUSDT', closedSize: '0', execFee: '0.66', execId: `exec-${qty}`, execPrice: price, execQty: qty,
  execType: 'Trade', execValue: '1200', feeRate: '0.00055', tradeIv: '', markIv: '', blockTradeId: '', markPrice: '3000', indexPrice: '',
  underlyingPrice: '', leavesQty: leaves, orderId: fill.orderId, orderLinkId: 'web', orderPrice: '3000.00', orderQty: '1.00',
  orderType: 'Limit', stopOrderType: 'UNKNOWN', side: 'Buy', execTime: '1760880001000', isLeverage: '0', isMaker: true,
  seq: 140612148849382, marketUnit: '', execPnl: '0', createType: 'CreateByUser', feeCurrency: 'USDT',
}]);
ws(PRIVATE, stream('order', [{ ...fill, orderStatus: 'New', leavesQty: '1.00' }]));
ws(PRIVATE, stream('order', [{ ...fill, orderStatus: 'PartiallyFilled', leavesQty: '0.60', cumExecQty: '0.40', cumExecValue: '1199.8', avgPrice: '2999.50' }]));
ws(PRIVATE, execution('0.40', '2999.50', '0.60'));
ws(PRIVATE, stream('order', [{ ...fill, orderStatus: 'Filled', leavesQty: '0', cumExecQty: '1.00', cumExecValue: '2999.2', avgPrice: '2999.20' }]));
ws(PRIVATE, execution('0.60', '2999.00', '0'));
// A post-only sell that would have taken liquidity.
const maker = order({ orderId: 'maker-0001', orderLinkId: 'maker', symbol: 'ETHUSDT', price: '2999.00', qty: '0.50', side: 'Sell', timeInForce: 'PostOnly' });
ws(PRIVATE, stream('order', [{ ...maker, orderStatus: 'New', leavesQty: '0.50' }]));
ws(PRIVATE, stream('order', [{ ...maker, orderStatus: 'Cancelled', leavesQty: '0', rejectReason: 'EC_PostOnlyWillTakeLiquidity' }]));

// User commands. Responses are keyed by the canonical request, body included,
// so a request whose parameters differ between implementations goes unserved.
const placed = '1666800494330512128';
command({ op: 'placeOrders', orders: [{ symbol: 'BTCUSDT', type: 'limit', side: 'buy', amount: 0.0021, price: 90500.04 }] });
http('POST', '/v5/order/create-batch', '', ok({ list: [{ category: 'linear', symbol: 'BTCUSDT', orderId: placed, orderLinkId: '', createAt: '1760880003000' }] }, { retExtInfo: { list: [{ code: 0, msg: 'OK' }] } }), {
  category: 'linear',
  request: [{ symbol: 'BTCUSDT', side: 'Buy', orderType: 'Limit', qty: '0.002', price: '90500', reduceOnly: false, timeInForce: 'GTC', closeOnTrigger: false, positionIdx: 0 }],
});
const placedRow = order({ orderId: placed, symbol: 'BTCUSDT', price: '90500.00', qty: '0.002', side: 'Buy', leavesQty: '0.002' });
ws(PRIVATE, stream('order', [{ ...placedRow, orderStatus: 'New' }]));
command({ op: 'amendOrder', id: placed, price: 90600 });
http('POST', '/v5/order/amend', '', ok({ orderId: placed, orderLinkId: '' }), { category: 'linear', symbol: 'BTCUSDT', orderId: placed, price: '90600' });
ws(PRIVATE, stream('order', [{ ...placedRow, price: '90600.00', orderStatus: 'New' }]));
command({ op: 'cancelOrders', ids: [placed] });
http('POST', '/v5/order/cancel-batch', '', ok({ list: [{ category: 'linear', symbol: 'BTCUSDT', orderId: placed, orderLinkId: '' }] }, { retExtInfo: { list: [{ code: 0, msg: 'OK' }] } }), {
  category: 'linear', request: [{ symbol: 'BTCUSDT', orderId: placed }],
});
ws(PRIVATE, stream('order', [{ ...placedRow, price: '90600.00', orderStatus: 'Cancelled', leavesQty: '0', rejectReason: 'EC_PerCancelRequest' }]));
command({ op: 'cancelSymbolOrders', symbol: 'ETHUSDT' });
http('POST', '/v5/order/cancel-all', '', ok({ list: [{ orderId: resting.orderId, orderLinkId: resting.orderLinkId }, { orderId: stop.orderId, orderLinkId: '' }], success: '1' }), {
  category: 'linear', symbol: 'ETHUSDT',
});
ws(PRIVATE, stream('order', [
  { ...resting, orderStatus: 'Cancelled', leavesQty: '0', rejectReason: 'EC_PerCancelRequest' },
  { ...stop, orderStatus: 'Deactivated', leavesQty: '0', rejectReason: 'EC_PerCancelRequest' },
]));

mkdirSync(dirname(values.out), { recursive: true });
records.sort((a, b) => a.t - b.t);
writeFileSync(values.out, `${records.map((r) => JSON.stringify(r)).join('\n')}\n`);
console.log(JSON.stringify({ out: values.out, records: records.length }));
