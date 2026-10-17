import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { createRequire } from 'node:module';
import { test } from 'node:test';

const native = createRequire(import.meta.url)('../dist/node/safe_cex_ffi.node');
const codes = [
  ['string', 'SIGNER_OFFLINE'], ['undefined', undefined], ['NaN', NaN],
  ['Infinity', Infinity], ['negative zero', -0], ['BigInt', 9007199254740993n],
  ['structured', { reason: undefined, retryAfter: Infinity, token: '\ud800' }],
];
for (const [label, code] of codes) {
  for (const asynchronous of [false, true]) {
    test(`hosted signer ${asynchronous ? 'rejection' : 'throw'} retains error name, message and ${label} code`, async t => {
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
        name: 'SignerUnavailableError', code,
      });
      const sign = asynchronous ? async () => { throw failure; } : () => { throw failure; };
      id = await native.nativeAccountOpen({ exchange: 'binance', options: {
        key: 'hosted-key', secret: '', overrideProxyUrl: `http://127.0.0.1:${server.address().port}`,
      }, accountName: 'hosted-account' }, () => {}, () => {}, sign);
      await assert.rejects(native.nativeAccountCallMethod(id, 'getAccount', []), error => {
        assert.equal(error.message, failure.message);
        assert.equal(error.name, failure.name);
        assert.ok(Object.hasOwn(error, 'code'));
        assert.deepEqual(error.code, failure.code);
        return true;
      });
      assert.deepEqual(requests, [], 'rejected signing must not send a venue request');
    });
  }
}

for (const asynchronous of [false, true]) {
  test(`signer ${asynchronous ? 'rejection' : 'throw'} reports a throwing code getter once`, async t => {
    const failure = new Error('Remote signer unavailable');
    Object.defineProperty(failure, 'code', { get() { throw new TypeError('code getter failed'); } });
    const sign = asynchronous ? async () => { throw failure; } : () => { throw failure; };
    const id = await native.nativeAccountOpen({ exchange: 'binance', options: {
      key: 'hosted-key', secret: '', overrideProxyUrl: 'http://127.0.0.1:1',
    } }, () => {}, () => {}, sign);
    t.after(() => native.nativeAccountClose(id));
    await assert.rejects(native.nativeAccountCallMethod(id, 'getAccount', []), error => {
      assert.equal(error.name, 'TypeError');
      assert.equal(error.message, 'code getter failed');
      return true;
    });
  });
}
