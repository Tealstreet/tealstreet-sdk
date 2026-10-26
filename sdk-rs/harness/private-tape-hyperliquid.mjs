// Deterministic Hyperliquid perps account tape from documented /info,
// /exchange and websocket payloads. The signing key is the PUBLIC official SDK
// test vector, never a funded wallet or a local credential.
//   corepack yarn -s rust:private-tape-hyperliquid --out <file>
// `--spot` records the same account with `enableSpotMarkets`: merged spot pairs,
// holdings and a spot order lifecycle.
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname } from 'node:path';
import { parseArgs } from 'node:util';

const { values } = parseArgs({
  options: {
    out: { type: 'string' },
    spot: { type: 'boolean', default: false },
  },
});
const SPOT = values.spot;
values.out ??= `sdk-rs/harness/tapes/hyperliquid${SPOT ? '-spot' : ''}-private.jsonl`;
const REST = 'https://api.hyperliquid.xyz';
const SOCKET = 'wss://api-ui.hyperliquid.xyz/ws';
const USER = '0x14791697260e4c9a71f18484c9f997b308e59325';
const BUILDER = { b: '0xea629e1e8534b82f704a818643f2daf38cb7fd4d', f: 10 };
const KEY = '0x0123456789012345678901234567890123456789012345678901234567890123';
const EPOCH = 1792454400000;
const cloid = (n) => `0x${n.toString(16).padStart(32, '0')}`;
const records = [{
  t: 0, type: 'config', private: true,
  options: { key: USER, secret: KEY, applicationId: USER, ...(SPOT ? { enableSpotMarkets: true } : {}) },
}];
let clock = 0;
let commandIndex = 0;
const at = () => (clock += 400); // Beyond the TS write client's 3 RPS guard.
const http = (path, requestBody, body, status = 200) => records.push({
  t: 0, type: 'http', method: 'POST', url: `${REST}${path}`,
  requestBody: JSON.stringify(requestBody), status, body: JSON.stringify(body),
});
const info = (type, body, extra = {}) => http('/info', { type, ...extra }, body);
// Reconciliation reads require both their recorded time and the command ACK.
// Timer pumping may advance beyond a future snapshot before its command runs;
// stage both reads and writes so Tape.readStage keeps that future state hidden.
const frontendOrders = (rows, dex = '', availableAt = clock) => {
  info('frontendOpenOrders', rows.map((row) => ({
    children: [], isPositionTpsl: row.isTrigger && row.origSz === '0',
    triggerCondition: 'N/A', triggerPx: '0', ...row,
  })), { user: USER, ...(dex ? { dex } : {}) });
  records.at(-1).availableAt = availableAt;
  records.at(-1).afterCommand = commandIndex;
};
const ws = (channel, data) => records.push({ t: at(), type: 'ws', url: SOCKET, text: JSON.stringify({ channel, data }) });
const command = (body) => {
  commandIndex++;
  records.push({ t: at(), type: 'command', ...body });
};
const reply = (action, statuses) => {
  http('/exchange', {
    action, nonce: EPOCH, signature: { r: '0x00', s: '0x00', v: 27 },
  }, { status: 'ok', response: { type: action.type, data: { statuses } } });
  records.at(-1).afterCommand = commandIndex;
};

const ctx = (price) => ({
  midPx: String(price), markPx: String(price), oraclePx: String(price),
  prevDayPx: String(price), funding: '0.0001', dayNtlVlm: '10000', dayBaseVlm: String(10000 / price), openInterest: '100',
});
info('metaAndAssetCtxs', [{ universe: [
  { name: 'BTC', szDecimals: 5, maxLeverage: 50, onlyIsolated: false },
  { name: 'ETH', szDecimals: 4, maxLeverage: 50, onlyIsolated: false },
] }, [ctx(100), ctx(200)]]);
info('perpDexs', [null, { name: 'xyz', fullName: 'Synthetic HIP-3', deployer: USER, oracleUpdater: USER }]);
info('metaAndAssetCtxs', [{ universe: [{ name: 'xyz:FOO', szDecimals: 2, maxLeverage: 10, onlyIsolated: false }] }, [ctx(50)]], { dex: 'xyz' });
// Spot discovery runs only with enableSpotMarkets. UBTC is Unit-bridged (shown as
// BTC); HYPE quotes in USDH; contexts join by coin, not position.
info('spotMetaAndAssetCtxs', SPOT ? [{
  tokens: [
    { name: 'USDC', index: 0, szDecimals: 8, weiDecimals: 8, tokenId: `0x${'00'.repeat(16)}`, isCanonical: true, fullName: null },
    { name: 'UBTC', index: 1, szDecimals: 5, weiDecimals: 10, tokenId: `0x${'01'.repeat(16)}`, isCanonical: false, fullName: 'Unit Bitcoin' },
    { name: 'HYPE', index: 2, szDecimals: 2, weiDecimals: 8, tokenId: `0x${'02'.repeat(16)}`, isCanonical: false, fullName: 'Hyperliquid' },
    { name: 'USDH', index: 3, szDecimals: 2, weiDecimals: 8, tokenId: `0x${'03'.repeat(16)}`, isCanonical: false, fullName: 'USDH' },
  ],
  universe: [
    { name: '@1', tokens: [1, 0], index: 1, isCanonical: false },
    { name: '@2', tokens: [2, 3], index: 2, isCanonical: false },
  ],
}, [
  { coin: '@2', midPx: '30', markPx: '30.1', prevDayPx: '29', dayNtlVlm: '3000', dayBaseVlm: '100', circulatingSupply: '1000', totalSupply: '1000' },
  { coin: '@1', midPx: '60000', markPx: '60010', prevDayPx: '59000', dayNtlVlm: '600000', dayBaseVlm: '10', circulatingSupply: '21', totalSupply: '21' },
]] : [{ universe: [], tokens: [] }, []]);
info('userRole', { role: 'user' }, { user: USER });
info('userAbstraction', 'disabled', { user: USER });
info('userFees', { userCrossRate: '0.00045', userAddRate: '0.00015' }, { user: USER });
info('maxBuilderFee', 10, { user: USER, builder: BUILDER.b });

const position = (coin, size, price, leverage = 10, mode = 'cross') => ({ type: 'oneWay', position: {
  coin, szi: String(size), entryPx: String(price), positionValue: String(Math.abs(size) * price),
  unrealizedPnl: '0', liquidationPx: String(price / 2), marginUsed: String(Math.abs(size) * price / leverage),
  leverage: { type: mode, value: leverage, rawUsd: '0' }, maxLeverage: 50,
  cumFunding: { allTime: '0', sinceChange: '0', sinceOpen: '0' }, returnOnEquity: '0',
} });
const state = (assetPositions, used, time = EPOCH) => ({
  assetPositions, time, withdrawable: String(1000 - used), crossMaintenanceMarginUsed: '0',
  marginSummary: { accountValue: '1000', totalMarginUsed: String(used), totalNtlPos: '200', totalRawUsd: '1000' },
  crossMarginSummary: { accountValue: '1000', totalMarginUsed: String(used), totalNtlPos: '200', totalRawUsd: '1000' },
});
const core = state([position('BTC', 2, 100)], 20);
const hip3 = state([position('xyz:FOO', 4, 50)], 20);
info('clearinghouseState', core, { user: USER });
info('clearinghouseState', hip3, { user: USER, dex: 'xyz' });
frontendOrders([], '', 0);
frontendOrders([], 'xyz', 0);
const spotBalances = SPOT ? [
  { coin: 'USDC', token: 0, total: '500', hold: '0', entryNtl: '0' },
  { coin: 'UBTC', token: 1, total: '0.5', hold: '0.1', entryNtl: '29000' },
  { coin: 'HYPE', token: 2, total: '10', hold: '0', entryNtl: '290' },
] : [];
info('spotClearinghouseState', { balances: spotBalances }, { user: USER });

ws('allMids', { mids: { BTC: '100', ETH: '200', 'xyz:FOO': '50' } });
ws('clearinghouseState', { user: USER, dex: '', clearinghouseState: core });
ws('clearinghouseState', { user: USER, dex: 'xyz', clearinghouseState: hip3 });
ws('activeAssetData', { user: USER, coin: 'ETH', markPx: '200', leverage: { type: 'isolated', value: 7, rawUsd: '0' }, maxTradeSzs: ['100', '100'], availableToTrade: ['1000', '1000'] });

const order = (coin, oid, side, price, size, extra = {}) => ({
  coin, oid, side, limitPx: String(price), sz: String(size), origSz: String(size),
  timestamp: EPOCH + clock, reduceOnly: false, isTrigger: false, orderType: 'Limit', tif: 'Gtc', ...extra,
});
const update = (row, status) => ws('orderUpdates', [{ order: row, status, statusTimestamp: EPOCH + clock }]);
const external = order('ETH', 1001, 'B', 190, 1, { cloid: cloid(101), tif: 'Alo' });
update(external, 'open');
update(external, 'canceled');
const fill = {
  coin: 'BTC', px: '100', sz: '0.5', side: 'B', time: EPOCH + clock,
  startPosition: '2', dir: 'Open Long', closedPnl: '0', oid: 1002, tid: 9001,
  crossed: true, fee: '0.01', feeToken: 'USDC', hash: `0x${'11'.repeat(32)}`, twapId: null,
};
// Snapshot is not an execution; overlapping live channels must emit only once.
ws('userFills', { user: USER, isSnapshot: true, fills: [fill] });
ws('userFills', { user: USER, isSnapshot: false, fills: [fill] });
ws('userEvents', { fills: [fill] });

const limit = (a, b, p, s, r, c, tif = 'Gtc') => ({ a, b, p, s, r, t: { limit: { tif } }, c });
const trigger = (a, b, p, s, r, px, tpsl, c) => ({ a, b, p, s, r, t: { trigger: { isMarket: true, triggerPx: px, tpsl } }, c });

// Caller cloid belongs to the bracket entry; both protective wires mint their
// own IDs. Replay normalizes only generated cloids, nonce and signature.
command({ op: 'placeOrders', orders: [{ symbol: 'BTC-PERP', type: 'limit', side: 'buy', amount: 0.25, price: 90, stopLoss: 80, takeProfit: 120, clientOrderId: cloid(1) }] });
reply({ type: 'order', orders: [
  limit(0, true, '90', '0.25', false, cloid(1)),
  trigger(0, false, '90', '0.25', true, '80', 'sl', cloid(201)),
  trigger(0, false, '90', '0.25', true, '120', 'tp', cloid(202)),
], grouping: 'normalTpsl', builder: BUILDER }, [{ resting: { oid: 3001 } }, 'waitingForFill', 'waitingForFill']);
const bracketEntry = order('BTC', 3001, 'B', 90, 0.25, { cloid: cloid(1) });
frontendOrders([bracketEntry]);
update(bracketEntry, 'open');
command({ op: 'cancelOrders', ids: ['3001'] });
reply({ type: 'cancel', cancels: [{ a: 0, o: 3001 }] }, ['success']);
frontendOrders([]);
update(bracketEntry, 'canceled');

// An explicit full-position stop returns a deferred status without an oid.
command({ op: 'placeOrders', orders: [{ symbol: 'BTC-PERP', type: 'stop_market', side: 'sell', amount: 0, price: 80, reduceOnly: true, clientOrderId: cloid(2) }] });
reply({ type: 'order', orders: [trigger(0, false, '95', '0', true, '80', 'sl', cloid(2))], grouping: 'positionTpsl', builder: BUILDER }, ['waitingForTrigger']);
const stop = order('BTC', 3002, 'A', 95, 0, { cloid: cloid(2), reduceOnly: true, isTrigger: true, triggerPx: '80', orderType: 'Stop Market', tif: '' });
update(stop, 'open');
frontendOrders([stop]);
command({ op: 'cancelOrders', ids: ['3002'] });
reply({ type: 'cancel', cancels: [{ a: 0, o: 3002 }] }, ['success']);
frontendOrders([]);
update(stop, 'canceled');

// HIP-3 asset index is 110000 for the first deployed DEX. Caller IDs also
// survive a normal standalone trigger and a market-as-Gtc order.
command({ op: 'placeOrders', orders: [{ symbol: 'XYZ|FOO-PERP', type: 'take_profit_market', side: 'sell', amount: 1, price: 60, reduceOnly: true, clientOrderId: cloid(3) }] });
reply({ type: 'order', orders: [trigger(110000, false, '47.5', '1', true, '60', 'tp', cloid(3))], grouping: 'na', builder: BUILDER }, [{ resting: { oid: 3003 } }]);
const hipStop = order('xyz:FOO', 3003, 'A', 47.5, 1, { cloid: cloid(3), reduceOnly: true, isTrigger: true, triggerPx: '60', orderType: 'Take Profit Market', tif: '' });
update(hipStop, 'open');
frontendOrders([hipStop], 'xyz');
command({ op: 'cancelOrders', ids: ['3003'] });
reply({ type: 'cancel', cancels: [{ a: 110000, o: 3003 }] }, ['success']);
frontendOrders([], 'xyz');
update(hipStop, 'canceled');
command({ op: 'placeOrders', orders: [{ symbol: 'ETH-PERP', type: 'market', side: 'buy', amount: 0.1, clientOrderId: cloid(4) }] });
reply({ type: 'order', orders: [limit(1, true, '210', '0.1', false, cloid(4))], grouping: 'na', builder: BUILDER }, [{ filled: { oid: 3004, totalSz: '0.1', avgPx: '200' } }]);
update(order('ETH', 3004, 'B', 210, 0, { cloid: cloid(4), origSz: '0.1' }), 'filled');

if (SPOT) {
  ws('spotState', { user: USER, spotState: { balances: spotBalances } });
  // A spot limit on the Unit pair: asset 10000 + index, never reduce-only, 8 - szDecimals price decimals.
  command({ op: 'placeOrders', orders: [{ symbol: 'UBTC/USDC', type: 'limit', side: 'buy', amount: 0.01, price: 59000, clientOrderId: cloid(5) }] });
  reply({ type: 'order', orders: [limit(10001, true, '59000', '0.01', false, cloid(5))], grouping: 'na', builder: BUILDER }, [{ resting: { oid: 3005 } }]);
  const spotOrder = order('@1', 3005, 'B', 59000, 0.01, { cloid: cloid(5) });
  update(spotOrder, 'open');
  frontendOrders([spotOrder]);
  command({ op: 'cancelOrders', ids: ['3005'] });
  reply({ type: 'cancel', cancels: [{ a: 10001, o: 3005 }] }, ['success']);
  frontendOrders([]);
  update(spotOrder, 'canceled');
}

// Authoritative snapshots converge after the optimistic fill and any heal.
// Preserve ETH's setting-only row by repeating its activeAssetData last.
ws('clearinghouseState', { user: USER, dex: '', clearinghouseState: state([position('BTC', 2.5, 100)], 25, EPOCH + clock + 400) });
ws('clearinghouseState', { user: USER, dex: 'xyz', clearinghouseState: { ...hip3, time: EPOCH + clock + 400 } });
ws('activeAssetData', { user: USER, coin: 'ETH', markPx: '200', leverage: { type: 'isolated', value: 7, rawUsd: '0' }, maxTradeSzs: ['100', '100'], availableToTrade: ['1000', '1000'] });

mkdirSync(dirname(values.out), { recursive: true });
records.sort((a, b) => a.t - b.t);
writeFileSync(values.out, `${records.map((record) => JSON.stringify(record)).join('\n')}\n`);
console.log(JSON.stringify({ out: values.out, records: records.length, frames: records.filter((r) => r.type === 'ws').length, commands: records.filter((r) => r.type === 'command').length, durationMs: clock }));
