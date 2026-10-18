import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { createRequire } from 'node:module';
import { test } from 'node:test';
import { setTimeout as delay } from 'node:timers/promises';
import { sourceFactoryErrorMessage } from './support/factory-error-source.mjs';
const native = createRequire(import.meta.url)('../dist/node/safe_cex_ffi.node');
const token = 'eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJhY2N0LTEiLCJleHAiOjQxMDI0NDQ4MDB9.c2ln';
for (const [exchange, status, body, prefix] of [
  ['bitrue', 401, { message: 'Invalid JWT' }, 'Tealstreet proxy auth failed'],
  ['bitrue', 403, { code: -2015, msg: 'Invalid API-key' }, 'Upstream exchange auth failed via proxy'],
  ['bitrue', 502, { message: 'gateway unavailable' }, 'Proxy server error'],
  ['weex', 401, { message: 'Invalid JWT' }, 'Tealstreet proxy auth failed'],
  ['bingx', 401, { message: 'Invalid JWT' }, 'Tealstreet proxy auth failed'],
]) {
  test(`the shared JS factory forwards a ${status} ${exchange} proxy error through napi`, { timeout: 10000 }, async t => {
    const requests = [], events = [];
    const server = createServer((request, response) => {
      requests.push({ url: request.url, headers: request.headers });
      response.writeHead(status, { 'content-type': 'application/json' });
      response.end(JSON.stringify(body));
    });
    await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
    let id;
    t.after(async () => {
      try { if (id !== undefined) await native.nativeAccountClose(id); }
      finally { server.closeAllConnections(); await new Promise(resolve => server.close(resolve)); }
    });
    id = await native.nativeAccountOpen({ exchange, options: {
      key: 'test-key', secret: 'test-secret', passphrase: 'test-passphrase', tealstreetProxy: true, tealstreetTunnel: false, overrideProxyUrl: `http://127.0.0.1:${server.address().port}`,
    } }, event => events.push(event), () => {}, undefined, async () => token);
    if (exchange === 'bitrue') await assert.rejects(native.nativeAccountCallMethod(id, 'validateAccount', []), error => {
      const expected = sourceFactoryErrorMessage(exchange, status, body, '/fapi/v2/account');
      assert.ok(error.message.includes(expected), `source=${expected}; native=${error.message}`);
      return true;
    });
    else await native.nativeAccountCallMethod(id, 'validateAccount', []);
    for (let i = 0; i < 100 && !events.some(event => event.event === 'error' && JSON.stringify(event.data).includes(prefix)); i++) await delay(10);
    assert.ok(requests.some(request => request.headers.authorization && (exchange !== 'bitrue' || request.url.includes('/fapi/v2/account'))), JSON.stringify(requests));
    assert.ok(events.some(event => event.event === 'error' && JSON.stringify(event.data).includes(`[${exchange}] ${prefix} (${status}): `)), JSON.stringify(events));
  });
}
