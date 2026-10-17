import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { createRequire } from 'node:module';
import { test } from 'node:test';

const native = createRequire(import.meta.url)('../dist/node/safe_cex_ffi.node');
for (const asynchronous of [false, true]) {
  test(`hosted signer ${asynchronous ? 'rejection' : 'throw'} retains error name, message and code`, async t => {
    const requests = [];
    const server = createServer((request, response) => {
      requests.push(request.url);
      response.writeHead(500).end();
    });
    await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
    let id;
    t.after(async () => {
      try { if (id !== undefined) await native.nativeAccountClose(id); }
      finally { server.closeAllConnections(); await new Promise(resolve => server.close(resolve)); }
    });
    const failure = Object.assign(new Error('Remote signer unavailable'), {
      name: 'SignerUnavailableError', code: 'SIGNER_OFFLINE',
    });
    const sign = asynchronous ? async () => { throw failure; } : () => { throw failure; };
    id = await native.nativeAccountOpen({ exchange: 'binance', options: {
      key: 'hosted-key', secret: '', overrideProxyUrl: `http://127.0.0.1:${server.address().port}`,
    }, accountName: 'hosted-account' }, () => {}, () => {}, sign);
    await assert.rejects(native.nativeAccountCallMethod(id, 'getAccount', []), error => {
      assert.equal(error.message, failure.message);
      assert.equal(error.name, failure.name);
      assert.equal(error.code, failure.code);
      return true;
    });
    assert.deepEqual(requests, [], 'rejected signing must not send a venue request');
  });
}
