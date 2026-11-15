import test from 'node:test';
import assert from 'node:assert/strict';
import { HYPERLIQUID_SPOT_REPLAY, validateHyperliquidSpotRequest } from './hyperliquidspot.mjs';
import { hyperliquidSpotMeta, hyperliquidSpotBalance } from './private-tape-hyperliquidspot.mjs';
import { spotRig, spotResponse } from './spot-host-rig.mjs';
function rig({ nativePurr = false, mismatchedStatusCoin = false } = {}) {
  // PURR's native id is the literal pair; other pairs use the catalogue's @index.
  // https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/info-endpoint#perpetuals-vs-spot
  const coin = nativePurr ? 'PURR/USDC' : '@1';
  const meta = structuredClone(hyperliquidSpotMeta);
  if (nativePurr) {
    meta[0].universe[0].name = coin;
    meta[0].universe[0].index = 0;
    meta[1][0].coin = coin;
  }
  let oid = 900;
  let rejectCancel = false;
  // What the venue lists: a read after the ack must show the order it just accepted.
  const open = new Map();
  const r = spotRig('hyperliquidspot', HYPERLIQUID_SPOT_REPLAY, (request) => {
    validateHyperliquidSpotRequest({ ...request, now: 1792454400000 });
    const body = JSON.parse(request.body);
    if (body.type === 'spotMetaAndAssetCtxs') return spotResponse(meta);
    if (body.type === 'spotClearinghouseState') return spotResponse(hyperliquidSpotBalance);
    if (body.type === 'frontendOpenOrders') return spotResponse([...open.values()]);
    if (body.type === 'orderStatus') return spotResponse({ status: 'order', order: { status: 'open', statusTimestamp: 1792454400000, order: { ...open.get(body.oid), ...(mismatchedStatusCoin && { coin: 'HYPE/USDC' }) } } });
    assert(new URL(request.url).pathname === '/exchange');
    if (body.action.type === 'cancel' && !rejectCancel) for (const { o } of body.action.cancels) open.delete(o);
    if (body.action.type === 'cancel') return spotResponse({ status: 'ok', response: { type: 'cancel', data: { statuses: rejectCancel ? [{ error: 'Order was never placed' }] : ['success'] } } });
    assert.equal(body.action.type, 'order');
    const statuses = body.action.orders.map(() => ({ resting: { oid: ++oid } }));
    for (const { resting } of statuses) open.set(resting.oid, { coin, side: 'B', limitPx: '90', sz: '0.25', origSz: '0.25', oid: resting.oid, timestamp: 1792454400000, orderType: 'Limit', tif: 'Gtc', reduceOnly: false, isTrigger: false, isPositionTpsl: false, triggerPx: '0', triggerCondition: 'N/A', children: [] });
    return spotResponse({ status: 'ok', response: { type: 'order', data: { statuses } } });
  });
  return { ...r, rejectCancel() { rejectCancel = true; } };
}
test('compiled Hyperliquid Spot keeps accepted native IDs for cancel and validates the cancel ACK', async () => {
  const r = rig();
  try {
    await r.start(); r.assertLoaded();
    const ids = await r.command({ op: 'placeOrders', orders: [{ symbol: 'PURR/USDC', type: 'limit', side: 'buy', amount: 0.25, price: 90 }] });
    assert.deepEqual(ids, ['901']);
    await r.command({ op: 'cancelOrders', ids });
    const writes = r.requests.filter((request) => new URL(request.url).pathname === '/exchange').map((request) => JSON.parse(request.body).action);
    assert.deepEqual(writes[1], { type: 'cancel', cancels: [{ a: 10001, o: 901 }] });
    assert.equal(writes[0].orders[0].r, false);
    assert.equal(writes[0].builder, undefined);
  } finally { r.dispose(); }
});
test('compiled Hyperliquid Spot failed cancel cannot authorize amend replacement', async () => {
  const r = rig();
  try {
    await r.start();
    await r.command({ op: 'placeOrders', orders: [{ symbol: 'PURR/USDC', type: 'limit', side: 'buy', amount: 0.25, price: 90 }] });
    r.rejectCancel();
    await assert.rejects(r.command({ op: 'amendOrder', id: '901', price: 91 }), {
      message: 'The outcome is unknown. Check Open Orders',
      outcome: 'unknown',
    });
    const actions = r.requests.filter((request) => new URL(request.url).pathname === '/exchange').map((request) => JSON.parse(request.body).action.type);
    assert.deepEqual(actions, ['order', 'cancel']);
  } finally { r.dispose(); }
});
test('compiled Hyperliquid Spot unsupported standalone protections reject before exchange writes', async () => {
  const r = rig();
  try {
    await r.start();
    for (const type of ['stop_market', 'take_profit_market', 'trailing_stop_market'])
      await assert.rejects(r.command({ op: 'placeOrders', orders: [{ symbol: 'PURR/USDC', type, side: 'sell', amount: 1, price: 80 }] }));
    assert.equal(r.requests.filter((request) => new URL(request.url).pathname === '/exchange').length, 0);
  } finally { r.dispose(); }
});


test('compiled Hyperliquid Spot accepts the documented native PURR pair in amend sizing', async () => {
  const r = rig({ nativePurr: true });
  try {
    await r.start();
    await r.command({ op: 'placeOrders', orders: [{ symbol: 'PURR/USDC', type: 'limit', side: 'buy', amount: 0.25, price: 90 }] });
    r.rejectCancel();
    await assert.rejects(r.command({ op: 'amendOrder', id: '901', price: 91 }), {
      message: 'The outcome is unknown. Check Open Orders',
      outcome: 'unknown',
    });
    const actions = r.requests.filter((request) => new URL(request.url).pathname === '/exchange').map((request) => JSON.parse(request.body).action);
    assert.deepEqual(actions.map(action => action.type), ['order', 'cancel']);
    assert.deepEqual(actions[1].cancels, [{ a: 10000, o: 901 }]);
  } finally { r.dispose(); }
});

test('compiled Hyperliquid Spot refuses a different sizing coin before cancel', async () => {
  const r = rig({ mismatchedStatusCoin: true });
  try {
    await r.start();
    await r.command({ op: 'placeOrders', orders: [{ symbol: 'PURR/USDC', type: 'limit', side: 'buy', amount: 0.25, price: 90 }] });
    await assert.rejects(r.command({ op: 'amendOrder', id: '901', price: 91 }), /sizing response does not identify the amended order/);
    const actions = r.requests.filter((request) => new URL(request.url).pathname === '/exchange').map((request) => JSON.parse(request.body).action.type);
    assert.deepEqual(actions, ['order']);
  } finally { r.dispose(); }
});
