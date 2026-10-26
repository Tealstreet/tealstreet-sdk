import assert from 'node:assert/strict';
import test from 'node:test';
import { EngineHost } from './host.mjs';

const work = { receipt: { stampMs: 1000, expiryMs: 2000 }, context: { venue: 'reference', method: 'POST', host: 'fixture.invalid', path: '/private' }, operations: [{ kind: 'present', key: 'account/primary' }] };
const action = { op: 'sign', attempt: 7, work, timeoutMs: 1000 };
const tick = async () => { for (let n = 0; n < 8; n++) await Promise.resolve(); };
function rig(signer) {
  const events = [];
  const timers = new Map();
  let next = 1;
  const engine = {
    useExternalSigner() { events.push(['installed']); },
    signed(...args) { events.push(['signed', ...args]); return ''; },
    signFailed(...args) { events.push(['failed', ...args]); return ''; },
  };
  const host = new EngineHost(engine, {
    now: () => 1234,
    setTimeout(fn) { const id = next++; timers.set(id, fn); return id; },
    clearTimeout(id) { timers.delete(id); },
  }, {}, { signer });
  return { host, events, timers };
}

test('injected signer sees opaque refs and context, returns one atomic batch', async () => {
  const r = rig(async (request, { signal }) => {
    assert.deepEqual(request, work);
    assert.equal(signal.aborted, false);
    return { values: [[1, 2, 3]], receipt: request.receipt };
  });
  r.host.execute(action);
  await tick();
  assert.deepEqual(r.events, [['installed'], ['signed', 7, JSON.stringify({ values: [[1, 2, 3]], receipt: work.receipt }), 1234]]);
  assert.equal(r.timers.size, 0);
  r.host.dispose();
});

test('bounded callback headers cross the same signed result without a side map', async () => {
  const result = { values: [[1, 2, 3]], receipt: work.receipt, headers: [['X-Proof', 'é'], ['x-proof', 'last']] };
  const r = rig(() => result);
  r.host.execute(action);
  await tick();
  assert.deepEqual(r.events, [['installed'], ['signed', 7, JSON.stringify(result), 1234]]);
  assert.equal(r.host.primitiveSignings.size, 0);
  assert.equal(r.timers.size, 0);
  r.host.dispose();
});

test('malformed callback headers fail before crossing into native signing', async () => {
  for (const headers of [null, {}, Array(1), [['X']], [['X', 'value', 'extra']], [['Bad Name', 'value']],
    [['X', 'value\r\nInjected: true']], [['X', '\0']], [['X', '\ud800']],
    [['X'.repeat(257), 'value']], [['X', 'x'.repeat(8193)]],
    Array.from({ length: 33 }, () => ['X', 'value']),
    Array.from({ length: 4 }, () => ['X', 'é'.repeat(4096)])]) {
    const r = rig(() => ({ values: [[1]], receipt: work.receipt, headers }));
    r.host.execute(action);
    await tick();
    assert.deepEqual(r.events, [['installed'], ['failed', 7, 'invalidResult', 1234]]);
    assert.equal(r.timers.size, 0);
    r.host.dispose();
  }
});

test('a timed-out provider cannot deliver late callback headers', async () => {
  let complete;
  const r = rig(() => new Promise((resolve) => { complete = resolve; }));
  r.host.execute(action);
  await tick();
  [...r.timers.values()][0]();
  complete({ values: [[1]], receipt: work.receipt, headers: [['X-Proof', 'late']] });
  await tick();
  assert.deepEqual(r.events, [['installed'], ['failed', 7, 'timedOut', 1234]]);
  assert.equal(r.host.primitiveSignings.size, 0);
  r.host.dispose();
});

test('result copying rejects sparse bytes and ignores provider array method overrides', async () => {
  for (const values of [Array(1), [Array(1)], [[1, , 3]]]) {
    values.some = () => false;
    values.map = () => [[1]];
    const r = rig(() => ({ values, receipt: work.receipt }));
    r.host.execute(action);
    await tick();
    assert.deepEqual(r.events, [['installed'], ['failed', 7, 'invalidResult', 1234]]);
    r.host.dispose();
  }
  const values = [[1, 2, 3]];
  values.some = values.map = values[0].some = () => { throw new Error('provider array method must not run'); };
  const r = rig(() => ({ values, receipt: work.receipt }));
  r.host.execute(action);
  await tick();
  assert.deepEqual(JSON.parse(r.events[1][2]), { values: [[1, 2, 3]], receipt: work.receipt });
  r.host.dispose();
});

test('callback headers are snapshotted once and array iterators cannot replace pairs', async () => {
  let reads = 0;
  const pair = ['X-Proof', 'bounded'];
  pair[Symbol.iterator] = () => { throw new Error('provider iterator must not run'); };
  const raw = {
    values: [[1]], receipt: work.receipt,
    get headers() {
      reads++;
      return reads === 1 ? [pair] : Array.from({ length: 33 }, () => ['X-Proof', 'swapped']);
    },
  };
  const r = rig(() => raw);
  r.host.execute(action);
  await tick();
  assert.equal(reads, 1);
  assert.deepEqual(JSON.parse(r.events[1][2]), { values: [[1]], receipt: work.receipt, headers: [['X-Proof', 'bounded']] });
  r.host.dispose();
});

test('hung signer times out, aborts and ignores a late completion', async () => {
  let complete;
  let signal;
  const r = rig((_, context) => { signal = context.signal; return new Promise((resolve) => { complete = resolve; }); });
  r.host.execute(action);
  await tick();
  [...r.timers.values()][0]();
  assert.equal(signal.aborted, true);
  complete({ values: [[7]], receipt: work.receipt });
  await tick();
  assert.deepEqual(r.events, [['installed'], ['failed', 7, 'timedOut', 1234]]);
  r.host.dispose();
});

test('provider failure and malformed result never expose arbitrary provider text', async () => {
  for (const signer of [() => { throw new Error('private-provider-text'); }, () => [[256]], () => [], () => [[1.5]]]) {
    const r = rig(signer);
    r.host.execute(action);
    await tick();
    assert.equal(r.events.length, 2);
    assert.equal(r.events[1][0], 'failed');
    assert.doesNotMatch(JSON.stringify(r.events), /private-provider-text/);
    assert.equal(r.timers.size, 0);
    r.host.dispose();
  }
});

test('disposal aborts provider and forbids later engine calls', async () => {
  let complete;
  let signal;
  const r = rig((_, context) => { signal = context.signal; return new Promise((resolve) => { complete = resolve; }); });
  r.host.execute(action);
  await tick();
  r.host.dispose();
  assert.equal(signal.aborted, true);
  complete({ values: [[7]], receipt: work.receipt });
  await tick();
  assert.deepEqual(r.events, [['installed']]);
  assert.equal(r.timers.size, 0);
});

test('opaque signing refs require the host provider before startup', () => {
  const engine = { requiresExternalSigner: () => true };
  assert.throws(() => new EngineHost(engine, {}, {}), (error) =>
    error.kind === 'invalidInput' && error.outcome === 'notSent');
  const host = new EngineHost({ ...engine, useExternalSigner() {} }, {}, {}, { signer: () => [] });
  host.dispose();
});


test('disposal releases the local-provider allocation and injected callback exactly once', () => {
  let frees = 0;
  const host = new EngineHost({ useExternalSigner() {}, free() { frees++; } }, {}, {}, { signer: () => [], ownsEngine: true });
  assert.equal(typeof host.signer, 'function');
  host.dispose();
  host.dispose();
  assert.equal(frees, 1);
  assert.equal(host.signer, undefined);
});


test('borrowed engines keep caller ownership and owned trap cleanup frees once', () => {
  for (const ownsEngine of [undefined, false, true]) {
    let frees = 0;
    let host;
    const engine = {
      start() { throw new WebAssembly.RuntimeError('unreachable'); },
      free() { frees++; host.dispose(); },
    };
    host = new EngineHost(engine, { now: () => 1 }, {}, { ownsEngine });
    host.start();
    host.dispose();
    assert.equal(frees, ownsEngine ? 1 : 0);
  }
});

test('missing, wrong and malformed receipt cannot replace the declared canonical facts', async () => {
  for (const receipt of [undefined, {}, { stampMs: 1001, expiryMs: 2000 }, { stampMs: 1000, expiryMs: 2001 },
    { stampMs: NaN }, { stampMs: 1.5 }, { expiryMs: Infinity }, { stampMs: -1 },
    { stampMs: Number.MAX_SAFE_INTEGER + 1 }, { stampMs: 1000, expiryMs: 1000 },
    { ...work.receipt, private: 'provider-text' }]) {
    const r = rig(() => ({ values: [[1]], receipt }));
    r.host.execute(action);
    await tick();
    assert.deepEqual(r.events, [['installed'], ['failed', 7, 'invalidResult', 1234]]);
    assert.equal(r.timers.size, 0);
    r.host.dispose();
  }
});

test('expiry-only and untimed receipts survive unchanged without a fabricated stamp', async () => {
  for (const receipt of [{}, { expiryMs: 2000 }]) {
    const r = rig((request) => ({ values: [[1]], receipt: request.receipt }));
    r.host.execute({ ...action, work: { ...work, receipt } });
    await tick();
    assert.deepEqual(JSON.parse(r.events[1][2]), { values: [[1]], receipt });
    r.host.dispose();
  }
});

test('provider mutation cannot rewrite the expected receipt snapshot', async () => {
  const r = rig((request) => { request.receipt.stampMs = 1999; return { values: [[1]], receipt: request.receipt }; });
  r.host.execute({ ...action, work: structuredClone(work) });
  await tick();
  assert.deepEqual(r.events, [['installed'], ['failed', 7, 'invalidResult', 1234]]);
  r.host.dispose();
});

test('declared signer algorithms reach the engine as exact JSON; undeclared stays unknown', () => {
  const declared = [];
  const engine = { useExternalSigner(...args) { declared.push(args); } };
  new EngineHost(engine, {}, {}, { signer: () => [], signerAlgorithms: ['hmacSha256', 'zkLinkContract'] }).dispose();
  new EngineHost(engine, {}, {}, { signer: () => [] }).dispose();
  assert.deepEqual(declared, [['["hmacSha256","zkLinkContract"]'], [undefined]]);
  assert.throws(() => new EngineHost(engine, {}, {}, { signer: () => [], signerAlgorithms: 'hmacSha256' }), TypeError);
});
