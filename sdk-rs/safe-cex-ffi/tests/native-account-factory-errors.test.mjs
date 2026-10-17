import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { createRequire } from 'node:module';
import { test } from 'node:test';
import { setTimeout as delay } from 'node:timers/promises';
const native = createRequire(import.meta.url)('../dist/node/safe_cex_ffi.node');
const token = 'eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJhY2N0LTEiLCJleHAiOjQxMDI0NDQ4MDB9.c2ln';
for (const [status, body, prefix] of [
  [401, { message: 'Invalid JWT' }, 'Tealstreet proxy auth failed'],
  [403, { code: -2015, msg: 'Invalid API-key' }, 'Upstream exchange auth failed via proxy'],
  [502, { message: 'gateway unavailable' }, 'Proxy server error'],
]) {
  test(`the shared JS factory forwards a ${status} proxy error through napi`, { timeout: 10000 }, async t => {
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
    id = await native.nativeAccountOpen({ exchange: 'bitrue', optionsJson: JSON.stringify({
      key: 'test-key', secret: 'test-secret', tealstreetProxy: true, overrideProxyUrl: `http://127.0.0.1:${server.address().port}`,
    }) }, event => events.push(JSON.parse(event)), () => {}, undefined, async () => token);
    await assert.rejects(native.nativeAccountCallMethod(id, 'validateAccount', '[]'), /Request failed with status code/);
    for (let i = 0; i < 100 && !events.some(event => event.event === 'error' && JSON.stringify(event.data).includes(prefix)); i++) await delay(10);
    assert.ok(requests.some(request => request.url.includes('/fapi/v2/account') && request.headers.authorization), JSON.stringify(requests));
    assert.ok(events.some(event => event.event === 'error' && JSON.stringify(event.data).includes(`[bitrue] ${prefix} (${status}): `)), JSON.stringify(events));
  });
}
