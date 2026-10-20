import test from 'node:test';
import assert from 'node:assert/strict';
import { HYPERLIQUID_SPOT_REPLAY, validateHyperliquidSpotRequest } from './hyperliquidspot.mjs';
import { hyperliquidSpotMeta, hyperliquidSpotBalance } from './private-tape-hyperliquidspot.mjs';
import { spotRig, spotResponse } from './spot-host-rig.mjs';
function rig() {
  let oid = 900;
  let rejectCancel = false;
  const r = spotRig('hyperliquidspot', HYPERLIQUID_SPOT_REPLAY, (request) => {
    validateHyperliquidSpotRequest({ ...request, now: 1792454400000 });
    const body = JSON.parse(request.body);
    if (body.type === 'spotMetaAndAssetCtxs') return spotResponse(hyperliquidSpotMeta);
    if (body.type === 'spotClearinghouseState') return spotResponse(hyperliquidSpotBalance);
    assert(new URL(request.url).pathname === '/exchange');
    if (body.action.type === 'cancel') return spotResponse({ status: 'ok', response: { type: 'cancel', data: { statuses: rejectCancel ? [{ error: 'Order was never placed' }] : ['success'] } } });
    assert.equal(body.action.type, 'order');
    return spotResponse({ status: 'ok', response: { type: 'order', data: { statuses: body.action.orders.map(() => ({ resting: { oid: ++oid } })) } } });
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
    await assert.rejects(r.command({ op: 'amendOrder', id: '901', price: 91 }));
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
