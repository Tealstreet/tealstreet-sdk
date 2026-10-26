import assert from 'node:assert/strict';
import test from 'node:test';
import { EngineHost } from './host.mjs';

// Actual EngineHost boundary, with a fixture WASM export. This is neither
// engine ownership evidence nor the later real-WASM Kraken adapter proof.
const work = {
  receipt: { stampMs: 1000, nonce: '9007199254740993' },
  context: { venue: 'kraken', method: 'GET', host: 'futures.kraken.com', path: '/api/v3/openorders' },
  operations: [{ kind: 'present', key: 'origin/credential' }],
};
const action = { op: 'sign', attempt: 7, timeoutMs: 1000, work };
const tick = async () => { for (let n = 0; n < 8; n++) await Promise.resolve(); };

function rig({ readMeta, live = true, signer }) {
  const events = [], reads = [], timers = new Map();
  let next = 1;
  const engine = {
    useExternalSigner() {},
    signingPending: () => live,
    signed(...args) { events.push(['signed', ...args]); return ''; },
    signFailed(...args) { events.push(['failed', ...args]); return ''; },
  };
  if (readMeta) engine.signMeta = (task) => { reads.push(task); return readMeta(task); };
  const host = new EngineHost(engine, {
    now: () => 1234,
    setTimeout(fn) { const id = next++; timers.set(id, fn); return id; },
    clearTimeout(id) { timers.delete(id); },
  }, {}, { signer });
  return { host, events, reads, timers };
}

for (const [name, owner, slot, role] of [
  ['selected secondary read', { kind: 'managed', request: 19, generation: 3 }, 'secondary', { kind: 'read', scope: 'orders' }],
  ['primary history read', { kind: 'managed', request: 23, generation: 4 }, 'primary', { kind: 'primaryRead', reason: 'history', scope: 'account' }],
  ['genuine Session login', { kind: 'session', generation: 5 }, 'primary', { kind: 'sessionAuth' }],
]) {
  test(`${name} forwards the engine's complete immutable ownership metadata`, async () => {
    const expected = { task: 7, owner, deadlineMs: 20_001.25, slot, role };
    let context;
    const r = rig({ readMeta: () => JSON.stringify(expected), signer: (request, actual) => {
      context = actual;
      return { receipt: request.receipt, values: [[1]] };
    } });
    try {
      r.host.execute(action);
      await tick();
      assert.deepEqual(context?.meta, expected);
      assert.equal(context.signal.aborted, false);
      for (const value of [context.meta, context.meta.owner, context.meta.role]) assert.equal(Object.isFrozen(value), true);
      assert.throws(() => { context.meta.deadlineMs = 99_999; }, TypeError);
      assert.deepEqual(r.reads, [7]);
      assert.equal(r.events[0]?.[0], 'signed');
      assert.equal(r.timers.size, 0);
    } finally { r.host.dispose(); }
  });
}

test('metadata is captured once at action delivery before the provider microtask', async () => {
  let current = { task: 7, owner: { kind: 'managed', request: 19, generation: 3 }, deadlineMs: 20_001.25,
    slot: 'secondary', role: { kind: 'read', scope: 'balance' } };
  const expected = structuredClone(current);
  let seen;
  const r = rig({ readMeta: () => JSON.stringify(current), signer: (request, context) => {
    seen = context.meta;
    return { receipt: request.receipt, values: [[1]] };
  } });
  try {
    r.host.execute(action);
    assert.deepEqual(r.reads, [7]);
    current = { ...current, deadlineMs: 99_999, slot: 'primary' };
    await tick();
    assert.deepEqual(seen, expected);
    assert.deepEqual(r.reads, [7]);
    assert.equal(r.timers.size, 0);
  } finally { r.host.dispose(); }
});

test('an old engine preserves the v1 provider context without invented metadata', async () => {
  let seen;
  const r = rig({ signer: (request, context) => { seen = context; return { receipt: request.receipt, values: [[1]] }; } });
  try {
    r.host.execute(action);
    await tick();
    assert.deepEqual(Object.keys(seen), ['signal']);
    assert.equal(r.events[0]?.[0], 'signed');
    assert.deepEqual(r.reads, []);
    assert.equal(r.timers.size, 0);
  } finally { r.host.dispose(); }
});

test('an ownerless task preserves v1 without fabricating a slot or role', async () => {
  let seen;
  const r = rig({ readMeta: () => undefined, signer: (request, context) => {
    seen = context;
    return { receipt: request.receipt, values: [[1]] };
  } });
  try {
    r.host.execute(action);
    await tick();
    assert.equal(Object.hasOwn(seen, 'meta'), false);
    assert.equal(r.events[0]?.[0], 'signed');
    assert.equal(r.timers.size, 0);
  } finally { r.host.dispose(); }
});

test('a retired signing task invokes neither the metadata export nor the provider', async () => {
  let calls = 0;
  const r = rig({ live: false, readMeta: () => { throw new Error('retired metadata'); }, signer: () => { calls++; } });
  try {
    r.host.execute(action);
    await tick();
    assert.equal(calls, 0);
    assert.deepEqual(r.reads, []);
    assert.deepEqual(r.events, []);
    assert.equal(r.timers.size, 0);
  } finally { r.host.dispose(); }
});
