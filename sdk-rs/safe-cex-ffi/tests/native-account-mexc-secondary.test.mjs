import assert from 'node:assert/strict';
import { createHash, createHmac } from 'node:crypto';
import { createServer } from 'node:http';
import { createRequire } from 'node:module';
import { test } from 'node:test';

const native = createRequire(import.meta.url)('../dist/node/safe_cex_ffi.node');

for (const secondaryKey of ['read-key', '', undefined]) {
  test(`MEXC native secondary routing: ${secondaryKey === undefined ? 'absent' : secondaryKey || 'empty'}`, async t => {
    const seen = [];
    const server = createServer(async (request, response) => {
      const chunks = [];
      for await (const chunk of request) chunks.push(chunk);
      seen.push({ method: request.method, url: request.url, headers: request.headers, body: Buffer.concat(chunks).toString() });
      response.writeHead(200, { 'content-type': 'application/json' });
      response.end('{"success":true,"code":0,"data":[]}');
    });
    await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
    const origin = `http://127.0.0.1:${server.address().port}`;
    let id;
    t.after(async () => {
      try { if (id !== undefined) await native.nativeAccountClose(id); }
      finally { server.closeAllConnections(); await new Promise(resolve => server.close(resolve)); }
    });
    id = await native.nativeAccountOpen({ exchange: 'mexc', options: {
      key: 'primary-key', secret: 'primary-secret', overrideProxyUrl: `${origin}/primary`,
      secondary_key: secondaryKey, secondary_secret: 'read-secret', secondaryProxyUrl: `${origin}/secondary`,
    } }, () => {}, () => {});
    const account = await native.nativeAccountCallMethod(id, 'getAccount', []);
    assert.equal(account.userId, `mexc:${createHash('sha256').update('primary-key').digest('hex').slice(0, 24)}`);
    await native.nativeAccountCallMethod(id, 'cancelAllOrders', []);
    assert.equal(seen.length, 2);
    assert.deepEqual(seen.map(request => request.method), ['GET', 'POST']);
    for (const request of seen) {
      const secondary = request.method === 'GET' && Boolean(secondaryKey);
      const key = secondary ? 'read-key' : 'primary-key';
      const secret = secondary ? 'read-secret' : 'primary-secret';
      assert.equal(request.headers.apikey, key);
      const url = new URL(request.url, 'http://loopback');
      assert.ok(url.pathname.startsWith(secondary ? '/secondary/' : '/primary/'), request.url);
      const data = request.method === 'GET' ? url.search.slice(1) : request.body;
      const expected = createHmac('sha256', secret).update(`${key}${request.headers['request-time']}${data}`).digest('hex');
      assert.equal(request.headers.signature, expected);
    }
    assert.ok(seen[0].url.endsWith('/api/v1/private/account/assets'));
    assert.ok(seen[1].url.endsWith('/api/v1/private/order/cancel_all'));
    assert.deepEqual(JSON.parse(seen[1].body), {});
  });
}
