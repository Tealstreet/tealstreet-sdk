import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { createRequire } from 'node:module';
import { test } from 'node:test';
import { setTimeout as delay } from 'node:timers/promises';
import { openStartedAccount } from './support/native-account.mjs';
const native = createRequire(import.meta.url)('../dist/node/safe_cex_ffi.node');

test('real native order results retain NaN and both infinities', { timeout: 10000 }, async t => {
  const seen = [];
  const server = createServer((request, response) => {
    seen.push(request.url);
    const orders = ['not-a-number', 'Infinity', '-Infinity', '-0'].map((price, i) => ({
      orderId: i + 1, clientOrderId: `client-${i}`, symbol: 'BTCUSDT', side: 'BUY', type: i === 3 ? 'UNKNOWN' : 'LIMIT',
      origQty: '1', price, stopPrice: i === 3 ? '-0' : 'not-a-number', status: 'NEW', executedQty: '0',
      reduceOnly: false, positionSide: 'BOTH', time: 1,
    }));
    response.writeHead(200, { 'content-type': 'application/json' });
    response.end(JSON.stringify(request.url.includes('/openAlgoOrders') ? [] : orders));
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  let id;
  const stores = [];
  t.after(async () => {
    try { if (id !== undefined) await native.nativeAccountClose(id); }
    finally { server.closeAllConnections(); await new Promise(resolve => server.close(resolve)); }
  });
  id = await native.nativeAccountOpen({ exchange: 'binance', options: {
    key: 'test-key', secret: 'test-secret', overrideProxyUrl: `http://127.0.0.1:${server.address().port}`,
  } }, () => {}, store => stores.push(store));
  const orders = await native.nativeAccountCallMethod(id, 'fetchOrders', [null]);
  assert.equal(seen.length, 2, 'both production signed Binance order reads reached loopback HTTP');
  assert.ok(Number.isNaN(orders[0].price), `expected NaN, received ${orders[0].price}`);
  assert.equal(orders[1].price, Infinity);
  assert.equal(orders[2].price, -Infinity);
  assert.ok(Object.is(orders[3].price, -0));
  assert.equal(Object.hasOwn(orders[3], 'type'), true);
  assert.equal(orders[3].type, undefined);
  assert.equal(Object.hasOwn(orders[3], 'parentId'), false);
  const snapshot = await native.nativeAccountCallMethod(id, 'fetchAndSetOrders', []);
  assert.ok(Number.isNaN(snapshot[0].price));
  for (let i = 0; i < 100 && !stores.some(store => store.orders?.length === 4); i++) await delay(10);
  const stored = stores.find(store => store.orders?.length === 4).orders;
  assert.ok(Number.isNaN(stored[0].price));
  assert.equal(stored[1].price, Infinity);
  assert.equal(stored[2].price, -Infinity);
  assert.equal(Object.hasOwn(stored[3], 'type'), true);
  assert.equal(stored[3].type, undefined);
  await assert.rejects(native.nativeAccountCallMethod(id, 'setLeverage', ['BTCUSDT', NaN]), /Market BTCUSDT not found/);
});

test('worker void result stays undefined through the native registry', { timeout: 10000 }, async t => {
  const h = await openStartedAccount(t);
  assert.equal(await h.call('fetchAndSetPrivateSnapshot'), undefined);
});
