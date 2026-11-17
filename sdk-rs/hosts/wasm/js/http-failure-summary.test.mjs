import assert from 'node:assert/strict';
import test from 'node:test';
import { EngineHost } from './host.mjs';
import { ManagedHttp } from './managed-http.mjs';

const tick = async () => { for (let i = 0; i < 20; i++) await Promise.resolve(); };
const modes = ['legacy', 'managed-sync', 'managed-async', 'group'];
const packet = { request: 1, method: 'GET', url: 'https://venue.test/info?token=PRIVATE', headers: [] };
const response = () => ({ status: 200, body: new Uint8Array() });
const named = (name, extra = {}) => Object.assign(new Error('PRIVATE'), { name, ...extra });

async function probe(mode, error, { healthy = false, disposed = false, onSend = () => {} } = {}) {
  const failures = [], received = [], order = [];
  const engine = {
    httpFailed: (id, message) => { failures.push([id, message]); return ''; },
    httpStart: (_id, status) => { received.push(status); return ''; },
    httpChunk: () => '', httpEnd: () => '',
    httpClaim: () => '{"status":"released"}',
    httpGroupPending: () => true,
    httpClaimGroup: (_permit, _now, bounds) => { bounds.fill(1000); return 1; },
    prepareDispatch: () => '',
  };
  const send = (id) => {
    order.push(`send:${id}`);
    onSend(id);
    if (id === 1 && !healthy) {
      if (mode === 'managed-async') return Promise.reject(error);
      throw error;
    }
    return Promise.resolve(response());
  };
  if (mode === 'legacy') {
    const host = new EngineHost(engine, { now: () => 100, request: () => send(1) });
    host.disposed = disposed;
    await host.request(packet);
    assert.equal(host.requests.size, 0);
    host.dispose();
  } else {
    const host = {
      disposed, engine, stageCauses: new Map(), handlers: {},
      transport: { now: () => 100, clearTimeout() {} },
      invoke: (fn, ...args) => fn(...args), run() {}, stage() {},
      receive: async (_id, value, active) => { if (active()) received.push(value.status); },
      failEngine: assert.fail,
    };
    const manager = new ManagedHttp(host);
    for (const id of mode === 'group' ? [1, 2] : [1]) {
      manager.resources.set(String(id), {
        phase: 'bound', groupRequest: id, controller: new AbortController(),
        diagnostic: { method: 'GET', path: '/info' },
        bound: { ready: () => true, dispatch: () => send(id) },
      });
    }
    const resource = manager.resources.get('1');
    if (mode === 'managed-sync') manager.claim('1', resource, 'permit', 1);
    else if (mode === 'managed-async') {
      resource.phase = 'released';
      await manager.receive('1', resource, 1, healthy ? Promise.resolve(response()) : Promise.reject(error));
    } else manager.execute({ phase: 'bindGroup', permit: 'group', members: [{ preparation: '1' }, { preparation: '2' }] });
    await tick();
    if (!disposed) assert.equal(manager.resources.size, 0);
    manager.dispose();
  }
  return { failures, received, order };
}

const samples = [
  ['timeout', () => named('TradeTunnelTimeoutError')],
  ['timeout', () => named('TradeTunnelReadinessError')],
  ['disconnect', () => named('TradeTunnelError', { originalError: 'TradeTunnel connection lost, reconnecting...' })],
  ['direct-fetch', () => new TypeError('Failed to fetch')],
  ['response-decode', () => named('TradeTunnelResponseDecodeError')],
  ['aborted', () => new DOMException('PRIVATE', 'AbortError')],
  ['timeout', () => new DOMException('PRIVATE', 'TimeoutError')],
  ['tunnel', () => named('TradeTunnelError', { originalError: { private: 'PRIVATE' } })],
];
for (const mode of modes) {
  for (const [cause, makeError] of samples) {
    test(`${mode} preserves only closed ${cause} cause from ${makeError().name}`, async () => {
      const result = await probe(mode, makeError());
      assert.deepEqual(result.failures, [[1, `HTTP transport request failed (cause: ${cause})`]]);
      if (mode === 'group') assert.deepEqual(result.order, ['send:1', 'send:2']);
    });
  }
  test(`${mode} healthy response stays healthy`, async () => {
    const result = await probe(mode, undefined, { healthy: true });
    assert.deepEqual(result.failures, []);
    assert.equal(result.received.length, mode === 'group' ? 2 : 1);
  });
}

test('unknown, oversized, cyclic and hostile errors never expose caller text or accessors', async () => {
  const hostile = {};
  for (const key of ['name', 'message', 'cause', 'config', 'url', 'originalError'])
    Object.defineProperty(hostile, key, { get() { assert.fail(`read hostile ${key} getter`); } });
  const cyclic = { message: 'PRIVATE', config: { Authorization: 'PRIVATE' } };
  cyclic.cause = cyclic;
  const errors = [undefined, null, 'PRIVATE', new Error('PRIVATE'.repeat(10000)), cyclic, hostile,
    new Proxy({}, { getOwnPropertyDescriptor() { throw new Error('PRIVATE'); } })];
  for (const mode of modes) for (const error of errors) {
    const { failures } = await probe(mode, error);
    assert.equal(failures.length, 1);
    assert.match(failures[0][1], /^HTTP transport request failed(?: \(cause: unknown\))?$/);
    assert.ok(failures[0][1].length <= 80);
    assert.doesNotMatch(failures[0][1], /PRIVATE|https:|Authorization/);
  }
});

test('group does not inspect the first thrown error between physical handoffs', async () => {
  const order = [];
  const error = new Proxy(named('TradeTunnelTimeoutError'), {
    getOwnPropertyDescriptor(target, key) { order.push('inspect'); return Reflect.getOwnPropertyDescriptor(target, key); },
  });
  const { failures } = await probe('group', error, { onSend: (id) => order.push(`send:${id}`) });
  assert.deepEqual(order.slice(0, 2), ['send:1', 'send:2']);
  assert.deepEqual(failures, [[1, 'HTTP transport request failed (cause: timeout)']]);
  assert.ok(order.slice(2).includes('inspect'));
});

test('disposed managed and legacy hosts publish no transport failure', async () => {
  for (const mode of ['legacy', 'managed-async']) {
    const { failures } = await probe(mode, named('TradeTunnelTimeoutError'), { disposed: true });
    assert.deepEqual(failures, []);
  }
});

test('closed cause survives command rejection and the engine log projection without changing kind/outcome', async () => {
  let command, sends = 0;
  const logs = [];
  const host = new EngineHost({
    command(json) { command = JSON.parse(json).command; return JSON.stringify([{ op: 'http', ...packet }]); },
    httpFailed(_id, message) {
      return JSON.stringify([{ op: 'event', code: 'test.failure', level: 'error', message },
        { op: 'done', command, error: message, detail: { message, kind: 'network', outcome: 'unknown' } }]);
    },
  }, {
    now: () => 100,
    request() { sends++; throw named('TradeTunnelTimeoutError', { config: { secret: 'PRIVATE' }, url: packet.url }); },
  }, { onLog: (_level, message) => logs.push(message) });
  await assert.rejects(host.command({ op: 'fetchCandles' }), (error) => {
    assert.equal(error.message, 'HTTP transport request failed (cause: timeout)');
    assert.equal(error.kind, 'network');
    assert.equal(error.outcome, 'unknown');
    return true;
  });
  assert.deepEqual(logs, ['HTTP transport request failed (cause: timeout)']);
  assert.equal(sends, 1);
  assert.equal(host.requests.size, 0);
  host.dispose();
});
