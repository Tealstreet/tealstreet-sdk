// Run after building the Backpack family; this uses its real registry and WASM.
import assert from 'node:assert/strict';
import test from 'node:test';
import { createPrivateKey, sign } from 'node:crypto';
import { createRequire } from 'node:module';
import { EngineHost } from '../hosts/wasm/js/host.mjs';

const require = createRequire(import.meta.url);
const { WasmEngine } = require('../hosts/wasm/pkg/node/safe_cex_wasm.js');
const seed = Buffer.alloc(32, 9);
const key = createPrivateKey({
  key: Buffer.concat([Buffer.from('302e020100300506032b657004220420', 'hex'), seed]),
  format: 'der', type: 'pkcs8',
});
const routes = [
  ['fetchFills', 'fills', 'fillHistoryQueryAll'],
  ['fetchOrderHistory', 'orders', 'orderHistoryQueryAll'],
  ['fetchIncome', 'funding', 'fundingHistoryQueryAll'],
];

for (const hasRead of [false, true]) {
  test(`public-only private history ${hasRead ? 'uses the read-only provider' : 'keeps the native missing refusal'}`, async () => {
    const native = new WasmEngine('backpack', JSON.stringify({
      publicDataOnly: true,
      key: 'malformed-primary-must-not-be-probed', secret: 'not-base64',
      ...(hasRead ? { readCredentials: { key: 'provider-read-key', secret: seed.toString('base64') } } : {}),
    }));
    let now = 2000, timerId = 0, claims = 0, dispatches = 0;
    const timers = new Map(), trace = [];
    // Observe the actual native claim, retaining its exact response and receiver.
    const engine = new Proxy(native, {
      get(target, property) {
        const value = Reflect.get(target, property, target);
        if (property === 'httpClaim') return (...args) => {
          const result = value.apply(target, args);
          assert.equal(JSON.parse(result).status, 'released');
          claims++;
          trace.push('claim');
          return result;
        };
        return typeof value === 'function' ? value.bind(target) : value;
      },
    });
    const transport = {
      now: () => now,
      setTimeout(callback, delay) { timers.set(++timerId, { callback, delay }); return timerId; },
      clearTimeout(id) { timers.delete(id); },
      request() { assert.fail('history must not use raw HTTP'); },
      connect() { assert.fail('public-only history must not start a private socket'); },
      prepareHttp(request) {
        trace.push('prepare');
        assert.equal(request.intent, 'secondary');
        return {
          bind(packet) {
            trace.push('bind');
            assert.equal(packet.request, request.request);
            assert.equal(packet.method, 'GET');
            assert.equal(packet.body, undefined);
            assert.equal(packet.signal.aborted, false);
            assert.equal(packet.headers['X-API-Key'], 'provider-read-key');
            assert.equal(packet.headers['X-Window'], '60000');
            assert.equal(packet.headers['X-Timestamp'], String(now));
            const [, path, instruction] = routes[dispatches];
            assert.equal(packet.url, `https://api.backpack.exchange/wapi/v1/history/${path}?limit=1&sortDirection=Desc`);
            const payload = `instruction=${instruction}&limit=1&sortDirection=Desc&timestamp=${now}&window=60000`;
            assert.equal(packet.headers['X-Signature'], sign(null, Buffer.from(payload), key).toString('base64'));
            return {
              ready: () => !packet.signal.aborted,
              dispatch() {
                assert.equal(claims, dispatches + 1, 'actual claim precedes each physical handoff');
                assert.equal(trace.at(-1), 'claim');
                dispatches++;
                trace.push('dispatch');
                return Promise.resolve({ status: 200, headers: '', body: new TextEncoder().encode('[]') });
              },
            };
          },
        };
      },
    };
    const host = new EngineHost(engine, transport, {}, { ownsEngine: true });
    try {
      assert.equal(native.requiresExternalSigner(), false, 'no hosted signing admission');
      for (const [op] of routes) {
        const command = host.command({ op, limit: 1, ...(op === 'fetchIncome' ? { incomeType: 'FUNDING' } : {}) });
        if (hasRead) assert.deepEqual(await command, []);
        else await assert.rejects(command, (error) => {
          assert.equal(error.message, 'Backpack credentials unavailable');
          assert.equal(error.kind, undefined);
          assert.equal(error.outcome, undefined);
          return true;
        });
        assert.equal(host.managedHttp.resources.size, 0);
        now++;
      }
      assert.equal(claims, hasRead ? 3 : 0);
      assert.equal(dispatches, hasRead ? 3 : 0);
      assert.deepEqual(trace, hasRead ? Array.from({ length: 3 }, () => ['prepare', 'bind', 'claim', 'dispatch']).flat() : []);
    } finally {
      host.dispose();
    }
    assert.equal(timers.size, 0);
  });
}
