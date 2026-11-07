import assert from 'node:assert/strict';
import { test } from 'node:test';
import { recordingHttp } from './record-http.mjs';

const response = (body) => ({ status: 200, headers: [['content-type', 'application/json']], body });
const drain = async (body) => {
  const parts = [];
  for await (const chunk of body) parts.push(new TextDecoder().decode(chunk));
  return parts.join('');
};

test('managed HTTP through prepareHttp is recorded like a plain request', async () => {
  const sent = [];
  const live = {
    request: async (request) => { sent.push(['request', request]); return response(new TextEncoder().encode('{"plain":1}')); },
    prepareHttp: async (request) => ({
      expiresAt: 7,
      cancel() {},
      bind: (packet) => ({
        ready: () => true,
        cancel() {},
        dispatch: async () => { sent.push(['dispatch', packet]); return response((async function* () { yield new TextEncoder().encode('{"managed":'); yield new TextEncoder().encode('2}'); })()); },
      }),
    }),
  };
  const records = [];
  const statuses = [];
  const http = recordingHttp(live, {}, (record) => records.push(record), (status) => statuses.push(status));

  const packet = { method: 'GET', url: 'https://fapi.bitunix.com/api/v1/futures/market/trading_pairs', headers: [] };
  const prepared = await http.prepareHttp(packet);
  assert.equal(prepared.expiresAt, 7);
  const bound = prepared.bind(packet);
  assert.equal(bound.ready(), true);
  const managed = await bound.dispatch();
  assert.equal(await drain(managed.body), '{"managed":2}');
  await http.request({ method: 'GET', url: 'https://example.test/plain' });

  assert.deepEqual(records.map(({ requestOrdinal, method, url, status, body }) => ({ requestOrdinal, method, url, status, body })), [
    { requestOrdinal: 0, method: 'GET', url: packet.url, status: 200, body: '{"managed":2}' },
    { requestOrdinal: 1, method: 'GET', url: 'https://example.test/plain', status: 200, body: '{"plain":1}' },
  ]);
  assert.deepEqual(statuses, [200, 200]);
  assert.deepEqual(sent.map(([path]) => path), ['dispatch', 'request']);
});

test('a transport without managed HTTP exposes no prepareHttp', () => {
  const http = recordingHttp({ request: async () => response(new Uint8Array()) }, {}, () => {}, () => {});
  assert.equal(http.prepareHttp, undefined);
});

test('a declared public-CORS read records direct egress on both paths', async () => {
  const routes = [];
  const live = {
    request: async (request) => { routes.push(request.route); return response(new Uint8Array()); },
    prepareHttp: async (request) => { routes.push(request.route); return { bind: () => ({ ready: () => true, dispatch: async () => response(new Uint8Array()) }) }; },
  };
  const http = recordingHttp(live, { recordPublicCorsDirect: true }, () => {}, () => {});
  await http.request({ method: 'GET', url: 'https://example.test/a', route: 'publicCors' });
  await http.prepareHttp({ method: 'GET', url: 'https://example.test/b', route: 'publicCors' });
  assert.deepEqual(routes, ['direct', 'direct']);
});
