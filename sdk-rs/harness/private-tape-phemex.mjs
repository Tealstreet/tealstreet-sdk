// Deterministic PerpetualV2 account/trading oracle from the official REST and
// aop_p schemas: https://phemex-docs.github.io/#usds-m-perpetual-rest-api
// No Rust output participates in this fixture.
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname } from 'node:path';
import { parseArgs } from 'node:util';
const { values } = parseArgs({ options: { out: { type: 'string', default: 'sdk-rs/harness/tapes/phemex-private.jsonl' } } });
const REST = 'https://api.phemex.com';
const WS = 'wss://ws.phemex.com';
const records = [];
const ok = (data) => ({ code: 0, msg: '', data });
const http = (method, path, data, effectiveAt) => records.push({
  t: 0, type: 'http', method, url: REST + path, status: 200, body: JSON.stringify(data),
  ...(effectiveAt === undefined ? {} : { effectiveAt }),
});
const ws = (t, data, bootstrap = false) => records.push({ t, type: 'ws', url: WS, text: JSON.stringify(data), ...(bootstrap ? { bootstrap: true } : {}) });
const command = (t, data) => records.push({ t, type: 'command', ...data });
const stamp = (ms) => 4_000_000_000_000_000_000 + ms * 1_000_000;
const product = (symbol, base, quote, tick, step) => ({ symbol, type: 'PerpetualV2', baseCurrency: base, quoteCurrency: quote, settleCurrency: quote, qtyStepSize: step, tickSize: tick, maxOrderQtyRq: '100', maxLeverage: '100', status: 'Listed', priceScale: 0, ratioScale: 0, valueScale: 0 });
http('GET', '/public/products', ok({ perpProductsV2: [product('BTCUSDT', 'BTC', 'USDT', '0.1', '0.001'), product('ETHUSDC', 'ETH', 'USDC', '0.01', '0.01')] }));
const ticker = (symbol, last) => ({ symbol, openRp: last, highRp: last, lowRp: last, lastRp: last, markRp: last, indexRp: last, bidRp: last, askRp: last, volumeRq: '1000', turnoverRv: '1000000', openInterestRv: '100', fundingRateRr: '0.0001' });
http('GET', '/md/v3/ticker/24hr/all', { error: null, id: 0, result: [ticker('BTCUSDT', '70000'), ticker('ETHUSDC', '3000')] });
const account = (currency, balance, used) => ({ userID: 932867, accountID: 9328670003, currency, accountBalanceRv: balance, totalUsedBalanceRv: used, bonusBalanceRv: '0', status: 0 });
const position = (symbol, currency, side, size, entry, leverage, overrides = {}) => ({
  accountID: 9328670003, userID: 932867, symbol, currency, side, posSide: side === 'Sell' ? 'Short' : 'Long', posMode: 'Hedged', positionStatus: 'Normal',
  size, avgEntryPriceRp: entry, leverageRr: leverage, crossMargin: false, liquidationPriceRp: '0', markPriceRp: symbol === 'BTCUSDT' ? '70000' : '3000',
  assignedPosBalanceRv: '69', positionMarginRv: '69', usedBalanceRv: '69', valueRv: '690', cumClosedPnlRv: '0', cumFundingFeeRv: '0', cumTransactFeeRv: '0',
  transactTimeNs: stamp(0), ...overrides,
});
const btc = position('BTCUSDT', 'USDT', 'Buy', '0.01', '69000', '10');
const short = position('BTCUSDT', 'USDT', 'Sell', '0', '0', '10');
const eth = position('ETHUSDC', 'USDC', 'Buy', '0', '0', '20');
const usdt = account('USDT', '1000', '69');
const usdc = account('USDC', '200', '0');
const latestBtc = { ...btc, size: '0.02', valueRv: '1380', assignedPosBalanceRv: '138', positionMarginRv: '138', usedBalanceRv: '138', transactTimeNs: stamp(1600) };
const latestUsdt = account('USDT', '1100', '138');
http('GET', '/g-accounts/accountPositions?currency=USDT', ok({ account: usdt, positions: [btc, short] }), 0);
http('GET', '/g-accounts/accountPositions?currency=USDT', ok({ account: latestUsdt, positions: [latestBtc, short] }), 1600);
http('GET', '/g-accounts/accountPositions?currency=USDC', ok({ account: usdc, positions: [eth] }));
// Symbol-scoped reads used by order formatting/recovery use the same snapshot.
http('GET', '/g-accounts/accountPositions?currency=USDT&symbol=BTCUSDT', ok({ account: latestUsdt, positions: [latestBtc, short] }));
const order = (id, symbol, side, qty, price, changes = {}) => ({
  orderID: id, clOrdID: '', symbol, side, posSide: side === 'Buy' ? 'Long' : 'Short', currency: symbol.endsWith('USDC') ? 'USDC' : 'USDT',
  ordType: 'Limit', ordStatus: 'New', orderQty: qty, orderQtyRq: qty, execQty: '0', cumQty: '0', leavesQty: qty, priceRp: price, stopPxRp: '0',
  stopLossRp: '0', takeProfitRp: '0', stopDirection: 'UNSPECIFIED', timeInForce: 'GoodTillCancel', reduceOnly: false, execInst: '',
  execID: '00000000-0000-0000-0000-000000000000', execStatus: 'New', tradeType: 'New', execPriceRp: '0', execFeeRv: '0',
  actionTimeNs: stamp(0), transactTimeNs: stamp(0), ...changes,
});
const resting = order('fa64c6f2-47a4-4929-aab4-b7fa9bbc4323', 'ETHUSDC', 'Sell', '0.1', '3500');
const stop = order('2146760a-0000-4000-8000-000000000001', 'BTCUSDT', 'Sell', '0.01', '0', { ordType: 'Stop', ordStatus: 'Untriggered', stopDirection: 'Falling', stopPxRp: '68000', reduceOnly: true, execInst: 'CloseOnTrigger', triggerType: 'ByMarkPrice', posSide: 'Long' });
const executed = order('5cf98598-39a7-459e-97bf-76ca765ee020', 'BTCUSDT', 'Buy', '0.01', '70000');
const firstExecution = { ...executed, ordStatus: 'PartiallyFilled', execStatus: 'TakerFill', tradeType: 'Trade', execID: '8718cae0-0000-4000-8000-000000000001', execQty: '0.001', cumQty: '0.001', leavesQty: '0.009', execPriceRp: '70000', transactTimeNs: stamp(1000) };
const secondExecution = { ...executed, ordStatus: 'PartiallyFilled', execStatus: 'TakerFill', tradeType: 'Trade', execID: '8b8a8a00-0000-4000-8000-000000000002', execQty: '0.002', cumQty: '0.003', leavesQty: '0.007', execPriceRp: '70000', transactTimeNs: stamp(1100) };
const aop = (t, data) => ({ sequence: 59553602 + t, timestamp: stamp(t), type: 'incremental', ...data });
// Auth acknowledgement and first order snapshot are bootstrap records. AOP
// subscriptions must follow a verified user.auth before this snapshot arrives.
ws(100, { error: null, id: 999, result: { status: 'success' } }, true);
ws(200, { ...aop(0, { orders_p: [resting, stop, executed], positions_p: [btc, short, eth] }), type: 'snapshot' }, true);
ws(400, { id: 1, result: 'pong', error: null });
ws(800, aop(800, { orders_p: [{ ...resting, priceRp: '3400', transactTimeNs: stamp(800), action: 'Replace', tradeType: 'Replace' }] }));
// Distinct execution IDs on one order in one batch, followed by a duplicate
// delivery. The execution oracle must emit exactly two fills, not three.
ws(1200, aop(1200, { orders_p: [firstExecution, secondExecution] }));
ws(1400, aop(1400, { orders_p: [secondExecution] }));
ws(1600, aop(1600, { positions_p: [latestBtc], accounts_p: [latestUsdt] }));
ws(2000, aop(2000, { orders_p: [{ ...stop, ordStatus: 'Triggered', execStatus: 'Triggered', transactTimeNs: stamp(2000) }] }));
// Triggered means the stop fired, not that its ID can never become open again.
ws(2400, aop(2400, { orders_p: [{ ...stop, ordStatus: 'New', execStatus: 'New', stopPxRp: '68100', transactTimeNs: stamp(2400) }] }));
ws(2800, aop(2800, { orders_p: [{ ...executed, ordStatus: 'Canceled', execStatus: 'Canceled', cumQty: '0.003', leavesQty: '0.007', transactTimeNs: stamp(2800) }] }));
const placedId = '743fc923-cb01-4261-88d1-b35dba2cdac0';
const placed = order(placedId, 'BTCUSDT', 'Buy', '0.002', '69300', { clOrdID: 'caller-phemex', transactTimeNs: stamp(3200) });
command(3200, { op: 'placeOrders', orders: [{ symbol: 'BTCUSDT', type: 'limit', side: 'buy', amount: 0.002, price: 69300, clientOrderId: 'caller-phemex' }] });
http('PUT', '/g-orders/create?clOrdID=caller-phemex&symbol=BTCUSDT&posSide=Long&side=Buy&ordType=Limit&orderQtyRq=0.002&priceRp=69300&timeInForce=GoodTillCancel', ok(placed));
ws(3600, aop(3600, { orders_p: [placed] }));
command(4400, { op: 'cancelOrders', ids: [placedId] });
http('DELETE', `/g-orders/cancel?symbol=BTCUSDT&posSide=Long&orderID=${placedId}`, ok({ ...placed, ordStatus: 'Canceled' }));
ws(4800, aop(4800, { orders_p: [{ ...placed, ordStatus: 'Canceled', execStatus: 'Canceled', transactTimeNs: stamp(4800) }] }));
// This authoritative final update follows the one-second terminal-event REST
// reconcile, exercising the documented WS account equity convention as well.
ws(6400, aop(6400, { accounts_p: [latestUsdt], positions_p: [latestBtc] }));
ws(7200, { id: 2, result: 'pong', error: null });
mkdirSync(dirname(values.out), { recursive: true });
records.sort((a, b) => a.t - b.t);
writeFileSync(values.out, records.map((r) => JSON.stringify(r)).join('\n') + '\n');
console.log(JSON.stringify({ out: values.out, records: records.length }));
