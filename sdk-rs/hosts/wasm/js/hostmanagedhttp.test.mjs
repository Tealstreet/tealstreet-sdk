import assert from 'node:assert/strict';
import test from 'node:test';
import { EngineHost, nativeTransport } from './host.mjs';
import { prepareFetch } from './managed-http.mjs';

const tick = async () => { for (let i = 0; i < 20; i++) await Promise.resolve(); };
const token = 'p1:7:18446744073709551615:1';
const permit = 'r1:7:18446744073709551615:1:1';
const packet = { request: 7, method: 'POST', url: 'https://venue.test/read', headers: [], timeoutMs: 1000, body: '{}' };
const prepare = { op: 'managedHttp', phase: 'prepare', preparation: token, intent: 'public', packet };
const bind = { op: 'managedHttp', phase: 'bind', preparation: token, permit, packet };
const cancel = { op: 'managedHttp', phase: 'cancel', preparation: token };
function rig(prepareHttp) {
  const events = [], timers = new Map();
  let now = 0, next = 0, until;
  const engine = {
    httpPreparationPending: (value) => value === token,
    httpPrepared(value, resource) { events.push(['prepared', value, resource]); return JSON.stringify([bind]); },
    httpPreparationFailed(value, code) { events.push(['failed', value, code]); return ''; },
    httpClaim(value, time) { events.push(['claim', value, time]); return JSON.stringify(until === undefined || time >= until ? { status: 'released' } : { status: 'wait', until }); },
    prepareDispatch() { events.push(['drain']); return ''; },
    httpStart() { events.push(['start']); return ''; },
    httpChunk() { events.push(['chunk']); return ''; },
    httpEnd() { events.push(['end']); return JSON.stringify([cancel]); },
    httpFailed() { events.push(['httpFailed']); return JSON.stringify([cancel]); },
  };
  const transport = {
    now: () => now, prepareHttp,
    request() { throw new Error('No legacy fallback'); },
    setTimeout(fn, ms) { const id = ++next; timers.set(id, { fn, at: now + ms }); return id; },
    clearTimeout(id) { timers.delete(id); },
  };
  const host = new EngineHost(engine, transport);
  return { host, events, timers, at(value) { now = value; }, wait(value) { until = value; } };
}

test('prepared JS handoff follows claim immediately, before drain and response callbacks', async () => {
  let r;
  r = rig(async (request) => {
    assert.equal(request.intent, 'public');
    return { bind(value) {
      assert.equal(value.signal.aborted, false);
      return { ready: () => true, dispatch() { r.events.push(['send']); return Promise.resolve({ status: 200, body: new Uint8Array([1]) }); } };
    } };
  });
  r.host.execute(prepare); await tick();
  assert.deepEqual(r.events, [['prepared', token, '1'], ['claim', permit, 0], ['send'], ['drain'], ['start'], ['chunk'], ['end']]);
  assert.equal(r.host.managedHttp.resources.size, 0);
  r.host.dispose();
});

test('pacing wait retains resource, consumes no send, rechecks readiness at final wake', async () => {
  let ready = true, sends = 0;
  const r = rig(() => ({ bind: () => ({ ready: () => ready, dispatch() { sends++; return new Promise(() => {}); } }) }));
  r.wait(450); r.host.execute(prepare); await tick();
  assert.equal(sends, 0); assert.equal(r.timers.size, 1);
  ready = false; r.at(450); [...r.timers.values()][0].fn();
  assert.equal(sends, 0);
  assert.deepEqual(r.events.at(-1), ['failed', token, 'timedOut']);
  r.host.dispose();
});

test('an always-unready provider settles once without a reprepare microtask loop', async () => {
  let prepared = 0;
  const r = rig(() => { prepared++; return { bind: () => ({ ready: () => false, dispatch() { assert.fail('unready'); } }) }; });
  // Faithful engine policy: only stale signals request another preparation.
  r.host.engine.httpPreparationFailed = (value, code) => {
    r.events.push(['failed', value, code]);
    return code === 'stale' ? JSON.stringify([prepare]) : '';
  };
  r.host.execute(prepare); await tick();
  assert.equal(prepared, 1);
  assert.deepEqual(r.events, [['prepared', token, '1'], ['failed', token, 'timedOut']]);
  assert.equal(r.host.managedHttp.resources.size, 0);
  r.host.dispose();
});

test('unsupported custom transport and bind failures never call legacy request or claim', async () => {
  for (const provider of [undefined, () => ({ bind() { throw new Error('private transport detail'); } })]) {
    const r = rig(provider); r.host.execute(prepare); await tick();
    assert.deepEqual(r.events.at(-1), ['failed', token, provider ? 'invalid' : 'unsupported']);
    assert.equal(r.events.filter(([kind]) => kind === 'claim').length, 0);
    assert.equal(r.host.managedHttp.resources.size, 0);
    r.host.dispose();
  }
});

test('cancelled preparation completion cannot replace live resources or call engine', async () => {
  let finish, closed = 0, signal;
  const r = rig((request) => { signal = request.signal; return new Promise((resolve) => { finish = resolve; }); });
  r.host.execute(prepare); r.host.execute(cancel);
  assert.equal(signal.aborted, true);
  finish({ bind() { throw new Error('must not bind'); }, cancel() { closed++; } });
  await tick();
  assert.deepEqual(r.events, []); assert.equal(closed, 1);
  r.host.dispose();
});

test('cancellation during body delivery stops same response and later callbacks', async () => {
  let r, closed = false, pulledAgain = false;
  r = rig(() => ({ bind: () => ({ ready: () => true, dispatch: () => Promise.resolve({ status: 200, body: (async function* () { try { yield new Uint8Array([1]); pulledAgain = true; await new Promise(() => {}); } finally { closed = true; } })() }) }) }));
  r.host.engine.httpChunk = () => { r.events.push(['chunk']); return JSON.stringify([cancel]); };
  r.host.execute(prepare); await tick();
  assert.equal(r.events.filter(([kind]) => kind === 'chunk').length, 1);
  assert.equal(r.events.filter(([kind]) => kind === 'end').length, 0);
  assert.equal(r.host.managedHttp.resources.size, 0);
  assert.equal(pulledAgain, false, 'retirement must not pull another potentially hung chunk');
  assert.equal(closed, true, 'retirement closes the body iterator');
  r.host.dispose();
});

test('throwing post-handoff diagnostics cannot strand a rejected response or become pre-send failure', async () => {
  const r = rig(() => ({ bind: () => ({ ready: () => true, dispatch() { r.events.push(['send']); return Promise.reject(new Error('private transport failure')); } }) }));
  r.host.engine.prepareDispatch = () => JSON.stringify([{ op: 'event', level: 'warn', code: 'fixture', message: 'diagnostic' }]);
  r.host.handlers.onEvent = () => { throw new Error('consumer failure'); };
  r.host.handlers.onLog = () => { throw new Error('consumer logger failure'); };
  r.host.execute(prepare); await tick();
  assert.deepEqual(r.events, [['prepared', token, '1'], ['claim', permit, 0], ['send'], ['httpFailed']]);
  assert.equal(r.host.managedHttp.resources.size, 0);
  r.host.dispose();
});

test('managed retirement aborts the exact primitive provider and ignores its late result', async () => {
  let finish, signal;
  const events = [], timers = new Map();
  const host = new EngineHost({ useExternalSigner() {}, signed() { events.push('signed'); return ''; }, signFailed() { events.push('failed'); return ''; } }, {
    now: () => 0, setTimeout(fn) { timers.set(1, fn); return 1; }, clearTimeout(id) { timers.delete(id); },
  }, {}, { signer: (_, context) => { signal = context.signal; return new Promise((resolve) => { finish = resolve; }); } });
  host.execute({ op: 'sign', attempt: 7, work: { receipt: {}, context: {}, operations: [{ kind: 'present', key: 'fixture' }] }, timeoutMs: 1000 });
  await tick(); assert.equal(signal.aborted, false); assert.equal(host.primitiveSignings.size, 1);
  host.execute({ ...cancel, signing: 7 });
  assert.equal(signal.aborted, true); assert.equal(host.primitiveSignings.size, 0); assert.equal(timers.size, 0);
  finish({ receipt: {}, values: [[1]] }); await tick();
  assert.deepEqual(events, []);
  host.dispose();
});

test('a delayed sign action cannot restart a provider after engine retirement', async () => {
  let calls = 0;
  const host = new EngineHost({ useExternalSigner() {}, signingPending: () => false }, {
    now: () => 0, setTimeout() { assert.fail('no retired timer'); }, clearTimeout() {},
  }, {}, { signer: () => { calls++; return { values: [], receipt: {} }; } });
  host.execute({ op: 'sign', attempt: 7, work: { receipt: {}, operations: [] }, timeoutMs: 1000 });
  await tick(); assert.equal(calls, 0); assert.equal(host.primitiveSignings.size, 0); host.dispose();
});

test('default fetch validates before handoff and invokes fetch synchronously exactly once', async () => {
  const calls = [];
  const request = { ...packet, headers: {}, signal: new AbortController().signal };
  const prepared = prepareFetch(request, (value) => { calls.push(value); return Promise.resolve(new Response('ok')); });
  assert.throws(() => prepared.bind({ ...request, url: 'https://other.test/read' }), /mismatch/);
  assert.equal(calls.length, 0);
  const bound = prepared.bind(request);
  assert.equal(bound.ready(), true); assert.equal(calls.length, 0);
  const response = bound.dispatch(); assert.equal(calls.length, 1); assert.equal(bound.ready(), false);
  assert.throws(() => bound.dispatch());
  assert.equal((await response).status, 200);
  assert.equal(typeof nativeTransport().prepareHttp, 'function');
});

test('a nonce-stale final Claim sends nothing and drains cancel before fresh preparation', async () => {
  const repairedToken = 'p1:7:18446744073709551615:2';
  const repairedPermit = 'r1:7:18446744073709551615:2:2';
  const repairedPrepare = { ...prepare, preparation: repairedToken };
  const repairedBind = { ...bind, preparation: repairedToken, permit: repairedPermit };
  const events = [];
  let needsRepair = false;
  const engine = {
    httpPreparationPending: () => true,
    httpPrepared(value) {
      events.push(['prepared', value]);
      return JSON.stringify([value === token ? bind : repairedBind]);
    },
    httpPreparationFailed() { assert.fail('Stale Claim is settled only by the engine repair'); },
    httpClaim(value) {
      events.push(['claim', value]);
      if (value === permit) { needsRepair = true; return JSON.stringify({ status: 'denied' }); }
      assert.equal(value, repairedPermit);
      return JSON.stringify({ status: 'released' });
    },
    prepareDispatch() {
      events.push(['drain']);
      if (!needsRepair) return '';
      needsRepair = false;
      return JSON.stringify([cancel, repairedPrepare]);
    },
  };
  const host = new EngineHost(engine, {
    now: () => 1000,
    request() { assert.fail('no raw fallback'); },
    prepareHttp() {
      events.push(['prepare']);
      return { bind() {
        return { ready: () => true, dispatch() {
          events.push(['send']);
          return new Promise(() => {});
        } };
      } };
    },
    setTimeout() { assert.fail('nonce repair has no transport retry timer'); },
    clearTimeout() {},
  });
  host.execute(prepare); await tick();
  assert.deepEqual(events, [
    ['prepare'], ['prepared', token], ['claim', permit], ['drain'],
    ['prepare'], ['prepared', repairedToken], ['claim', repairedPermit], ['send'], ['drain'],
  ]);
  assert.equal(host.managedHttp.resources.size, 1);
  host.dispose();
  assert.equal(host.managedHttp.resources.size, 0);
});
