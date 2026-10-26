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
  ['scope-less read', { kind: 'managed', request: 29, generation: 0 }, 'secondary', { kind: 'read', scope: null }],
  ['primary mutation', { kind: 'managed', request: 31, generation: 0 }, 'primary', { kind: 'mutation' }],
  ...['account', 'balance', 'orders', 'positions', 'settings'].map((scope) =>
    [`read scope ${scope}`, { kind: 'managed', request: 37, generation: 1 }, 'secondary', { kind: 'read', scope }]),
  ...['clientOrderLookup', 'postUnknownOrderRead', 'degradedRead', 'catalog', 'history'].map((reason) =>
    [`primary read reason ${reason}`, { kind: 'managed', request: 41, generation: 1 }, 'primary', { kind: 'primaryRead', reason, scope: null }]),
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

const validMeta = () => ({ task: 7, owner: { kind: 'managed', request: 19, generation: 3 },
  deadlineMs: 20_001.25, slot: 'secondary', role: { kind: 'read', scope: 'orders' } });

for (const [name, change] of [
  ['task mismatch', (m) => { m.task = 8; }],
  ['unsafe task', (m) => { m.task = 2 ** 53; }],
  ['unsafe owner request', (m) => { m.owner.request = 2 ** 53; }],
  ['unsafe owner generation', (m) => { m.owner.generation = 2 ** 53; }],
  ['negative owner request', (m) => { m.owner.request = -1; }],
  ['negative owner generation', (m) => { m.owner.generation = -1; }],
  ['fractional owner request', (m) => { m.owner.request = 19.25; }],
  ['fractional owner generation', (m) => { m.owner.generation = 3.25; }],
  ['string task', (m) => { m.task = '7'; }],
  ['unknown slot', (m) => { m.slot = 'fallback'; }],
  ['unknown owner', (m) => { m.owner.kind = 'command'; }],
  ['unknown role', (m) => { m.role.kind = 'background'; }],
  ['unknown scope', (m) => { m.role.scope = 'wallet'; }],
  ['missing scope', (m) => { delete m.role.scope; }],
  ['unknown primary reason', (m) => { m.role = { kind: 'primaryRead', reason: 'arbitrary', scope: null }; }],
  ['missing owner request', (m) => { delete m.owner.request; }],
  ['missing owner generation', (m) => { delete m.owner.generation; }],
  ['null deadline', (m) => { m.deadlineMs = null; }],
  ['string deadline', (m) => { m.deadlineMs = '20001.25'; }],
  ['extra root key', (m) => { m.key = 'private-descriptor-sentinel'; }],
  ['extra owner credential', (m) => { m.owner.credential = 'private-descriptor-sentinel'; }],
  ['extra role field', (m) => { m.role.account = 'private-descriptor-sentinel'; }],
  ['Session owner with extra request', (m) => { m.owner = { kind: 'session', generation: 1, request: 19 }; }],
  ['mutation role with extra scope', (m) => { m.role = { kind: 'mutation', scope: null }; }],
  ['Session owner with read role', (m) => { m.owner = { kind: 'session', generation: 1 }; }],
  ['managed owner with SessionAuth role', (m) => { m.slot = 'primary'; m.role = { kind: 'sessionAuth' }; }],
  ['secondary mutation', (m) => { m.role = { kind: 'mutation' }; }],
  ['secondary primary-only read', (m) => { m.role = { kind: 'primaryRead', reason: 'history', scope: 'account' }; }],
]) {
  test(`${name} is queried but never forwarded to the provider`, async () => {
    const malformed = validMeta();
    change(malformed);
    let context;
    const r = rig({ readMeta: () => JSON.stringify(malformed), signer: (request, actual) => {
      context = actual;
      return { receipt: request.receipt, values: [[1]] };
    } });
    try {
      assert.doesNotThrow(() => r.host.execute(action));
      await tick();
      assert.deepEqual(r.reads, [7], 'the live export must actually be consulted');
      assert.deepEqual(Object.keys(context), ['signal']);
      assert.equal(r.events[0]?.[0], 'signed');
      assert.doesNotMatch(JSON.stringify(r.events), /private-descriptor-sentinel/);
      assert.equal(r.timers.size, 0);
    } finally { r.host.dispose(); }
  });
}

for (const [name, readMeta] of [
  ['ordinary export error', () => { throw new Error('private-export-sentinel'); }],
  ['unparsable JSON', () => '{'],
  ['null JSON', () => 'null'],
  ['array JSON', () => '[]'],
  ['non-string export', () => validMeta()],
  ['nonfinite deadline', () => JSON.stringify(validMeta()).replace('20001.25', '1e309')],
]) {
  test(`${name} preserves v1 with absent metadata`, async () => {
    let context;
    const r = rig({ readMeta, signer: (request, actual) => {
      context = actual;
      return { receipt: request.receipt, values: [[1]] };
    } });
    try {
      assert.doesNotThrow(() => r.host.execute(action));
      await tick();
      assert.deepEqual(r.reads, [7]);
      assert.deepEqual(Object.keys(context), ['signal']);
      assert.equal(r.events[0]?.[0], 'signed');
      assert.doesNotMatch(JSON.stringify(r.events), /private-export-sentinel/);
      assert.equal(r.timers.size, 0);
    } finally { r.host.dispose(); }
  });
}

for (const ErrorType of [WebAssembly.RuntimeError, RangeError]) {
  test(`${ErrorType.name} from metadata remains fatal without provider or pending revival`, async () => {
    let calls = 0;
    const r = rig({ readMeta: () => { throw new ErrorType('fatal-export-sentinel'); }, signer: () => { calls++; } });
    try {
      assert.doesNotThrow(() => r.host.execute(action));
      await tick();
      assert.deepEqual(r.reads, [7]);
      assert.equal(r.host.disposed, true);
      assert.equal(calls, 0);
      assert.deepEqual(r.events, []);
      assert.equal(r.timers.size, 0);
      assert.equal(r.host.primitiveSignings.size, 0);
    } finally { r.host.dispose(); }
  });
}
