import test from 'node:test';
import assert from 'node:assert/strict';
import { KRAKEN_SPOT_REPLAY, createKrakenSpotRequestAudit } from './krakenspot.mjs';
import { krakenSpotPairs, krakenSpotTickers, KRAKEN_SPOT_EPOCH } from './private-tape-krakenspot.mjs';
import { spotRig, spotResponse } from './spot-host-rig.mjs';
function rig() {
  const audit = createKrakenSpotRequestAudit();
  return spotRig('krakenspot', KRAKEN_SPOT_REPLAY, (request) => {
    audit({ ...request, now: KRAKEN_SPOT_EPOCH });
    const path = new URL(request.url).pathname;
    if (path.endsWith('/AssetPairs')) return spotResponse(krakenSpotPairs);
    if (path.endsWith('/Ticker')) return spotResponse(krakenSpotTickers);
    if (path.endsWith('/Balance')) return spotResponse({ error: [], result: { ZUSD: '1000', XXBT: '2' } });
    if (path.endsWith('/OpenOrders')) return spotResponse({ error: [], result: { open: { 'O-100': { descr: { pair: 'XBTUSD', type: 'buy', ordertype: 'limit', price: '90' }, vol: '2', vol_exec: '0', oflags: 'post', opentm: KRAKEN_SPOT_EPOCH / 1000 } } } });
    if (path.endsWith('/GetWebSocketsToken')) return spotResponse({ error: [], result: { token: 'replay-spot-token', expires: 900 } });
    if (path.endsWith('/AmendOrder')) return spotResponse({ error: [], result: { amend_id: 'amend-123' } });
    assert.fail(`Unexpected Kraken Spot request ${path}`);
  });
}
test('compiled Kraken Spot atomic amend preserves txid and sends total quantity with post-only', async () => {
  const r = rig();
  try {
    await r.start(); r.assertLoaded();
    const result = await r.command({ op: 'amendOrder', id: 'O-100', price: 91, amount: 3 });
    assert.deepEqual(result, ['O-100']);
    const amend = r.requests.find((request) => new URL(request.url).pathname.endsWith('/AmendOrder'));
    const body = new URLSearchParams(amend.body); body.delete('nonce');
    assert.deepEqual(Object.fromEntries(body), { txid: 'O-100', order_qty: '3', limit_price: '91', post_only: 'true' });
    assert.equal(r.requests.some((request) => new URL(request.url).pathname.endsWith('/CancelOrder')), false);
  } finally { r.dispose(); }
});
test('compiled Kraken Spot reconnect fetches a new token before private subscriptions', async () => {
  const r = rig();
  try {
    await r.start();
    const first = r.sockets.find((socket) => socket.url.includes('ws-auth.'));
    assert(first.sent.some((frame) => frame.params?.channel === 'executions'));
    first.handlers.onClose(1006);
    await r.advance(5000);
    const privateSockets = r.sockets.filter((socket) => socket.url.includes('ws-auth.'));
    assert.equal(privateSockets.length, 2);
    assert.equal(r.requests.filter((request) => new URL(request.url).pathname.endsWith('/GetWebSocketsToken')).length, 2);
    assert(privateSockets[1].sent.some((frame) => frame.params?.token === 'replay-spot-token'));
  } finally { r.dispose(); }
});
