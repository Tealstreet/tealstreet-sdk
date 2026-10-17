import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { createRequire } from 'node:module';
import { test } from 'node:test';
const native = createRequire(import.meta.url)('../dist/node/safe_cex_ffi.node');

test('real native order results retain NaN and both infinities', { timeout: 10000 }, async t => {
  const seen = [];
  const server = createServer((request, response) => {
    seen.push(request.url);
    const orders = ['not-a-number', 'Infinity', '-Infinity'].map((price, i) => ({
      orderId: i + 1, clientOrderId: `client-${i}`, symbol: 'BTCUSDT', side: 'BUY', type: 'LIMIT',
      origQty: '1', price, stopPrice: 'not-a-number', status: 'NEW', executedQty: '0',
      reduceOnly: false, positionSide: 'BOTH', time: 1,
    }));
    response.writeHead(200, { 'content-type': 'application/json' });
    response.end(JSON.stringify(request.url.includes('/openAlgoOrders') ? [] : orders));
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  let id;
  t.after(async () => {
    try { if (id !== undefined) await native.nativeAccountClose(id); }
    finally { server.closeAllConnections(); await new Promise(resolve => server.close(resolve)); }
  });
  id = await native.nativeAccountOpen({ exchange: 'binance', optionsJson: JSON.stringify({
    key: 'test-key', secret: 'test-secret', overrideProxyUrl: `http://127.0.0.1:${server.address().port}`,
  }) }, () => {}, () => {});
  const orders = JSON.parse(await native.nativeAccountCallMethod(id, 'fetchOrders', '[null]'));
  assert.equal(seen.length, 2, 'both production signed Binance order reads reached loopback HTTP');
  assert.ok(Number.isNaN(orders[0].price), `expected NaN, received ${orders[0].price}`);
  assert.equal(orders[1].price, Infinity);
  assert.equal(orders[2].price, -Infinity);
});
