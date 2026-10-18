import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { createRequire } from 'node:module';
import { test } from 'node:test';
import { setTimeout as delay } from 'node:timers/promises';
const native = createRequire(import.meta.url)('../dist/node/safe_cex_ffi.node');

async function open(t) {
  const seen = [];
  const server = createServer((request, response) => {
    seen.push(request.url);
    response.writeHead(400, { 'content-type': 'application/json' });
    response.end(JSON.stringify({ success: false, code: -1002, msg: 'API key expired' }));
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  let id;
  t.after(async () => {
    try { if (id !== undefined) await native.nativeAccountClose(id); }
    finally { server.closeAllConnections(); await new Promise(resolve => server.close(resolve)); }
  });
  const events = [], stores = [];
  id = await native.nativeAccountOpen({ exchange: 'woofi', accountName: 'test-account', options: {
    key: 'test-orderly-key', secret: 'ed25519:11111111111111111111111111111111', applicationId: 'test-account',
    overrideProxyUrl: `http://127.0.0.1:${server.address().port}`,
  } }, event => events.push(event), store => stores.push(store));
  return { events, stores, seen, id, call: async method => await native.nativeAccountCallMethod(id, method, []) };
}

test('Orderly auth expiry posts the TS deactivation status and suppresses the disposal Store reset', { timeout: 10000 }, async t => {
  const h = await open(t);
  assert.equal(await h.call('start'), false, 'auth deactivation cancels the real worker startup');
  await delay(50);
  assert.equal(h.seen.length, 1, 'the real signed account read reaches the local server');
  assert.ok(h.seen[0].includes('/v1/client/info'));
  assert.ok(h.events.some(event => event.event === 'error' && JSON.stringify(event.data).includes('API key expired')));
  assert.ok(h.events.some(event => event.event === 'deactivate-status' && event.data === true));
  assert.equal(h.stores.length, 0, 'worker disposal clears store delivery before resetting the Store');
});

