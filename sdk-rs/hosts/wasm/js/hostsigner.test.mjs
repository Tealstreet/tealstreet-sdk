import assert from 'node:assert/strict';
import test from 'node:test';
import { EngineHost } from './host.mjs';

const work = { context: { venue: 'reference', method: 'POST', host: 'fixture.invalid', path: '/private' }, operations: [{ kind: 'present', key: 'account/primary' }] };
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
    return [[1, 2, 3]];
  });
  r.host.execute(action);
  await tick();
  assert.deepEqual(r.events, [['installed'], ['signed', 7, '[[1,2,3]]', 1234]]);
  assert.equal(r.timers.size, 0);
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
  complete([[7]]);
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
  complete([[7]]);
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
