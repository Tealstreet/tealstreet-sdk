import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { createRequire } from 'node:module';
import { test } from 'node:test';

const native = createRequire(import.meta.url)('../dist/node/safe_cex_ffi.node');
const address = '0x14791697260e4c9a71f18484c9f997b308e59325';

for (const recvWindow of [32, undefined]) {
  test(`Hyperliquid info read awaits its response with recvWindow ${recvWindow ?? 'absent'}`, { timeout: 10000 }, async t => {
    const seen = [], timers = [];
    let completed = 0;
    const server = createServer(async (request, response) => {
      const chunks = [];
      for await (const chunk of request) chunks.push(chunk);
      seen.push({ url: request.url, body: JSON.parse(Buffer.concat(chunks).toString()) });
      timers.push(setTimeout(() => {
        if (!response.destroyed) {
          response.writeHead(200, { 'content-type': 'application/json' });
          response.end('[]');
          completed++;
        }
      }, 120));
    });
    await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
    let id;
    t.after(async () => {
      for (const timer of timers) clearTimeout(timer);
      try { if (id !== undefined) await native.nativeAccountClose(id); }
      finally { server.closeAllConnections(); await new Promise(resolve => server.close(resolve)); }
    });
    id = await native.nativeAccountOpen({ exchange: 'hyperliquid', optionsJson: JSON.stringify({
      key: address, secret: '', applicationId: address,
      overrideProxyUrl: `http://127.0.0.1:${server.address().port}`,
      extra: recvWindow === undefined ? undefined : { recvWindow },
    }) }, () => {}, () => {});
    const result = JSON.parse(await native.nativeAccountCallMethod(id, 'fetchIncomeHistory', JSON.stringify([
      { startTime: 1000, endTime: 2000, incomeType: 'FUNDING_FEE' },
    ])));
    assert.deepEqual(result, []);
    assert.equal(completed, 1, 'the real info response must arrive before the method settles');
    assert.equal(seen.length, 1, 'a successful source read must not retry');
    assert.ok(seen[0].url.endsWith('/info'));
    assert.equal(seen[0].body.type, 'userFunding');
    assert.equal(seen[0].body.user, address);
  });
}
