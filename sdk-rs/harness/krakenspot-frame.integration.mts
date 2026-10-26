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
test('compiled Kraken Spot emits each native execution once at last trade quantity and price', async () => {
  const r = rig();
  try {
    await r.start();
    const socket = r.sockets.find((socket) => socket.url.includes('ws-auth.'));
    const trade = { channel: 'executions', type: 'update', data: [{ order_id: 'O-100', symbol: 'BTC/USD', side: 'buy', order_type: 'limit', exec_type: 'trade', exec_id: 'T-1', limit_price: 90, order_qty: 2, cum_qty: 0.25, last_qty: 0.25, last_price: 87.5, avg_price: 89.5, timestamp: new Date(KRAKEN_SPOT_EPOCH).toISOString() }] };
    await r.send(socket, { ...trade, type: 'snapshot' });
    assert.equal(r.frames.reduce((sum, frame) => sum + (frame.fills()?.count ?? 0), 0), 0);
    await r.send(socket, trade);
    await r.send(socket, trade);
    const markets = r.frames.map((frame) => frame.markets()).filter(Boolean).at(-1);
    const fills = r.frames.flatMap((frame) => {
      const table = frame.fills();
      return table ? Array.from({ length: table.count }, (_, i) => ({ id: table.orderId(i), symbol: markets.symbol(table.market[i]), price: table.rows[i * 2], amount: table.rows[i * 2 + 1] })) : [];
    });
    assert.deepEqual(fills, [{ id: 'O-100', symbol: 'XBT/USD', price: 87.5, amount: 0.25 }]);
  } finally { r.dispose(); }
});
test('compiled Kraken Spot documented native triggers retain their protective price and safe amend identity', async () => {
  const r = rig();
  try {
    await r.start();
    const socket = r.sockets.find((socket) => socket.url.includes('ws-auth.'));
    await r.send(socket, { channel: 'executions', type: 'update', data: [{ order_id: 'O-STOP', symbol: 'BTC/USD', side: 'sell', order_type: 'stop-loss', exec_type: 'new', order_qty: 2, cum_qty: 0,
      triggers: { reference: 'last', price: 80, actual_price: 80, price_type: 'static', status: 'untriggered' }, timestamp: new Date(KRAKEN_SPOT_EPOCH).toISOString() }] });
    const result = await r.command({ op: 'amendOrder', id: 'O-STOP', price: 81 });
    assert.deepEqual(result, ['O-STOP']);
    const body = new URLSearchParams(r.requests.findLast((request) => new URL(request.url).pathname.endsWith('/AmendOrder')).body);
    body.delete('nonce');
    assert.deepEqual(Object.fromEntries(body), { txid: 'O-STOP', trigger_price: '81' });
    const orders = r.frames.map((frame) => frame.orders()).filter(Boolean).at(-1);
    const row = Array.from({ length: orders.count }, (_, i) => i).find((i) => orders.id(i) === 'O-STOP');
    assert.notEqual(row, undefined);
    assert.equal(orders.price[row], 80);
  } finally { r.dispose(); }
});
test('compiled Kraken Spot batches final trade and filled lifecycle without duplicating an execution', async () => {
  const r = rig();
  try {
    await r.start();
    const socket = r.sockets.find((socket) => socket.url.includes('ws-auth.'));
    const row = { order_id: 'O-100', symbol: 'BTC/USD', side: 'buy', order_type: 'limit', limit_price: 90, order_qty: 2, cum_qty: 2, avg_price: 89.5, timestamp: new Date(KRAKEN_SPOT_EPOCH).toISOString() };
    // Executions with order_status:true includes both trade and lifecycle rows.
    const message = { channel: 'executions', type: 'update', data: [
      { ...row, exec_type: 'trade', exec_id: 'T-FINAL', last_qty: 2, last_price: 87.5 },
      { ...row, exec_type: 'filled' },
    ] };
    await r.send(socket, message);
    await r.send(socket, message);
    const fills = r.frames.flatMap((frame) => {
      const table = frame.fills();
      return table ? Array.from({ length: table.count }, (_, i) => ({ id: table.orderId(i), price: table.rows[i * 2], amount: table.rows[i * 2 + 1] })) : [];
    });
    assert.deepEqual(fills, [{ id: 'O-100', price: 87.5, amount: 2 }]);
    const orders = r.frames.map((frame) => frame.orders()).filter(Boolean).at(-1);
    const index = Array.from({ length: orders.count }, (_, i) => i).find((i) => orders.id(i) === 'O-100');
    assert.notEqual(index, undefined);
    assert.equal(orders.enums[index] & 0xff, 1); // Closed; the SDK filters terminal rows.
    assert.equal(orders.filled[index], 2);
    assert.equal(orders.remaining[index], 0);
  } finally { r.dispose(); }
});
