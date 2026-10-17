// node --test after scripts/build.mjs node: loads the actual napi addon and
// drives its command channel, native registry account, HTTP and Store watches.
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { createRequire } from 'node:module';
import { before, after, test } from 'node:test';
import { setTimeout as delay } from 'node:timers/promises';

const native = createRequire(import.meta.url)('../dist/node/safe_cex_ffi.node');
const order = { orderId: 77, clientOrderId: 'lost-response', symbol: 'BTCUSDT', side: 'BUY', type: 'LIMIT', origQty: '1', price: '10', stopPrice: '0', status: 'NEW', executedQty: '0', reduceOnly: false, positionSide: 'BOTH', time: 1 };
const seen = [];
const server = createServer((request, response) => {
  const url = new URL(request.url.replace(/^\/https?:\/\/[^/]+/, ''), 'http://local');
  seen.push(url.pathname);
  const body = url.pathname === '/fapi/v1/openOrders' ? [order] : url.pathname === '/fapi/v1/openAlgoOrders' ? [] : undefined;
  response.writeHead(body ? 200 : 404, { 'content-type': 'application/json' });
  response.end(JSON.stringify(body ?? { code: -1, msg: 'unexpected test route' }));
});
let origin;
before(async () => {
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  origin = `http://127.0.0.1:${server.address().port}`;
});
after(async () => {
  server.closeAllConnections();
  await new Promise(resolve => server.close(resolve));
  assert.ok(seen.every(path => ['/fapi/v1/openOrders', '/fapi/v1/openAlgoOrders'].includes(path)));
});
async function open(t) {
  const stores = [];
  const id = await native.nativeAccountOpen(
    { exchange: 'binance', options: { key: 'test-key', secret: 'test-secret', overrideProxyUrl: origin } },
    () => {}, data => stores.push(data),
  );
  t.after(() => native.nativeAccountClose(id));
  return { stores, call: async (method, ...args) => await native.nativeAccountCallMethod(id, method, args) };
}
async function eventually(predicate) {
  for (let i = 0; i < 200; i++) {
    if (predicate()) return;
    await delay(10);
  }
  assert.fail('native Store update did not arrive');
}

test('reconciles a lost placement response through napi and the native Store', async t => {
  const { call } = await open(t);
  const orders = await call('fetchAndSetOrders', null);
  assert.equal(orders[0].id, '77');
  const requestsBeforeLookup = seen.length;
  const found = await call('findOrderByClientOrderId', 'BTCUSDT', 'lost-response');
  assert.equal(found.found, true);
  assert.equal(found.order.id, '77');
  assert.deepEqual(await call('findOrderByClientOrderId', 'ETHUSDT', 'lost-response'), { found: false, definitive: false });
  assert.deepEqual(await call('findOrderByClientOrderId', 'BTCUSDT', 'unknown'), { found: false, definitive: false });
  assert.equal(seen.length, requestsBeforeLookup, 'Binance checks its Store and unloaded-market guard without fetching orders again');
});

test('removeOrders accepts TS id objects and forwards the mutated Store', async t => {
  const { call, stores } = await open(t);
  await call('fetchAndSetOrders', null);
  await eventually(() => stores.some(store => store.orders.some(order => order.id === '77')));
  const count = stores.length;
  assert.equal(await call('removeOrders', [{ id: '77' }]), null);
  await eventually(() => stores.length > count && stores.at(-1).orders.length === 0);
});

test('fetchLastLogsFromWebsockets returns an empty array before sockets start', async t => {
  const { call } = await open(t);
  assert.deepEqual(await call('fetchLastLogsFromWebsockets'), []);
});

test('getActiveTickerSymbols reads the initial symbol selection', async t => {
  const { call } = await open(t);
  assert.deepEqual(await call('getActiveTickerSymbols'), []);
});

test('setActiveTickerSymbols normalizes and retains symbols across napi calls', async t => {
  const { call } = await open(t);
  assert.equal(await call('setActiveTickerSymbols', [' btcusdt ', 'ETHUSDT', 'BTCUSDT', 'ETHUSDT', '']), null);
  assert.deepEqual(await call('getActiveTickerSymbols'), [' btcusdt ', 'ETHUSDT', 'BTCUSDT']);
});
