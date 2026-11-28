import assert from 'node:assert/strict';
import test from 'node:test';
import { EngineHost, socketMessages } from './host.mjs';
import { prepareFetch } from './managed-http.mjs';

const flush = () => new Promise((resolve) => setImmediate(resolve));
function deferred() {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

// An account switch retires one fixed host and constructs another. Deliberately
// collide every native token/ID and physical timer handle across those hosts.
const token = 'p1:7:1:1';
const permit = 'r1:7:1:1:1';
const packet = (owner) => ({ request: 7, method: 'POST', url: 'https://venue.test/order', headers: [], body: owner, timeoutMs: 1000 });
const prepare = (owner) => ({ op: 'managedHttp', phase: 'prepare', preparation: token, intent: 'primary', packet: packet(owner) });
const cancel = { op: 'managedHttp', phase: 'cancel', preparation: token };
const response = (owner) => ({ status: 200, body: new TextEncoder().encode(owner) });

function httpOwner(owner, options = {}) {
  const events = [], timers = new Map(), reply = deferred();
  let now = 0, next = 0, signal;
  const bound = {
    ready: () => true,
    ...(options.admission ? { admitDispatch: options.admission } : {}),
    dispatch() { events.push(['send', owner]); return options.dispatch ? options.dispatch() : reply.promise; },
    cancel() { events.push(['cancelBound']); },
  };
  const prepared = {
    bind(request) { assert.equal(request.body, owner); return bound; },
    cancel() { events.push(['cancelPrepared']); },
  };
  const engine = {
    httpPreparationPending: (value) => value === token,
    httpPrepared(value, resource) {
      events.push(['prepared', value, resource]);
      return JSON.stringify([{ op: 'managedHttp', phase: 'bind', preparation: value, permit, packet: packet(owner) }]);
    },
    httpPreparationFailed(...args) { events.push(['preparationFailed', ...args]); return ''; },
    httpClaim(value) { events.push(['claim', value]); return JSON.stringify(options.waitUntil > now ? { status: 'wait', until: options.waitUntil } : { status: 'released' }); },
    prepareDispatch: () => '',
    httpStart(id, status) { events.push(['start', id, status]); return ''; },
    httpChunk(id, bytes) { events.push(['chunk', id, [...bytes]]); return ''; },
    httpEnd(id) { events.push(['end', id]); return JSON.stringify([cancel]); },
    httpFailed(...args) { events.push(['failed', ...args]); return JSON.stringify([cancel]); },
  };
  const host = new EngineHost(engine, {
    now: () => now,
    prepareHttp(request) {
      signal = request.signal;
      assert.equal(request.body, owner);
      return options.prepareHttp ? options.prepareHttp(request) : prepared;
    },
    request() { assert.fail('managed work cannot fall back to legacy HTTP'); },
    setTimeout(fn) { const id = ++next; timers.set(id, fn); return id; },
    clearTimeout(id) { timers.delete(id); },
  });
  return { host, events, timers, reply, prepared, signal: () => signal, at(value) { now = value; } };
}

for (const phase of ['preparation', 'admission', 'pacing', 'response', 'body']) {
  for (const reject of phase === 'pacing' ? [false] : [false, true]) {
    test(`owner switch during managed HTTP ${phase} (${reject ? 'reject' : 'resolve'}) preserves B's colliding resource`, async (t) => {
      const held = deferred();
      let handoff, bodyClosed = false, latePreparedCancelled = 0;
      const options = {};
      if (phase === 'preparation') options.prepareHttp = () => held.promise;
      if (phase === 'admission') options.admission = (claim) => { handoff = claim; return held.promise; };
      if (phase === 'pacing') options.waitUntil = 100;
      if (phase === 'response') options.dispatch = () => held.promise;
      if (phase === 'body') options.dispatch = () => ({ status: 200, body: (async function* () {
        try { yield new Uint8Array([65]); yield await held.promise; }
        finally { bodyClosed = true; }
      })() });
      const a = httpOwner('A', options);
      t.after(() => a.host.dispose());
      a.host.execute(prepare('A'));
      await flush();
      if (phase === 'preparation') assert.deepEqual(a.events, []);
      if (phase === 'admission') assert.equal(typeof handoff, 'function');
      if (phase === 'body') assert.deepEqual(a.events.filter(([kind]) => kind === 'chunk'), [['chunk', 7, [65]]]);
      const wake = phase === 'pacing' ? [...a.timers.values()][0] : undefined;
      if (phase === 'pacing') assert.equal(typeof wake, 'function');
      const sendsBeforeSwitch = a.events.filter(([kind]) => kind === 'send').length;
      assert.equal(sendsBeforeSwitch, ['response', 'body'].includes(phase) ? 1 : 0);
      a.host.dispose();
      assert.equal(a.signal().aborted, true);
      const aBefore = structuredClone(a.events);

      const b = httpOwner('B');
      t.after(() => b.host.dispose());
      b.host.execute(prepare('B'));
      await flush();
      const bResource = b.host.managedHttp.resources.get(token);
      assert.ok(bResource);
      const bBefore = structuredClone(b.events);
      // Fire callbacks even after clearTimeout/abort, as if already queued.
      if (wake) { a.at(100); wake(); }
      else if (reject) held.reject(new Error('old A provider failed'));
      else if (phase === 'preparation') held.resolve({
        bind() { assert.fail('retired A preparation bound'); },
        cancel() { latePreparedCancelled++; },
      });
      else if (phase === 'admission') { handoff(); held.resolve(); }
      else if (phase === 'body') held.resolve(new Uint8Array([65]));
      else held.resolve({ status: 200, body: { [Symbol.asyncIterator]() { assert.fail('retired A response consumed'); } } });
      await flush();
      assert.deepEqual(a.events, aBefore, 'no late A engine entry or send');
      assert.deepEqual(b.events, bBefore, 'no A settlement delivered to B');
      assert.equal(b.host.managedHttp.resources.get(token), bResource, 'A cleanup cannot consume B');
      assert.equal(a.host.managedHttp.resources.size, 0);
      if (phase === 'preparation') assert.equal(latePreparedCancelled, reject ? 0 : 1);
      if (phase === 'body') assert.equal(bodyClosed, true);

      b.reply.resolve(response('B'));
      await flush();
      assert.deepEqual(b.events.filter(([kind]) => ['start', 'chunk', 'end', 'failed'].includes(kind)), [
        ['start', 7, 200], ['chunk', 7, [66]], ['end', 7],
      ]);
      assert.equal(b.events.filter(([kind]) => kind === 'send').length, 1);
      assert.equal(b.host.managedHttp.resources.size, 0);
    });
  }
}

for (const phase of ['headers', 'body']) {
  test(`owner switch during prepared fetch ${phase} keeps response readers with A`, async (t) => {
    const held = deferred();
    let reads = 0, cancelled = 0, released = 0;
    const stream = { getReader: () => ({
      read() { reads++; return held.promise; },
      async cancel() { cancelled++; },
      releaseLock() { released++; },
    }) };
    const raw = { status: 200, headers: new Headers(), body: stream };
    const a = httpOwner('A', { prepareHttp: (request) => prepareFetch(request, () => phase === 'headers' ? held.promise : Promise.resolve(raw)) });
    t.after(() => a.host.dispose());
    a.host.execute(prepare('A'));
    await flush();
    assert.equal(reads, phase === 'body' ? 1 : 0);
    a.host.dispose();
    const aBefore = structuredClone(a.events);
    const b = httpOwner('B');
    t.after(() => b.host.dispose());
    b.host.execute(prepare('B'));
    await flush();
    const bBefore = structuredClone(b.events);
    const bResource = b.host.managedHttp.resources.get(token);
    held.resolve(phase === 'headers' ? raw : { done: false, value: new Uint8Array([65]) });
    await flush();
    assert.deepEqual(a.events, aBefore);
    assert.deepEqual(b.events, bBefore);
    assert.equal(b.host.managedHttp.resources.get(token), bResource);
    assert.equal(reads, phase === 'body' ? 1 : 0);
    assert.equal(cancelled, phase === 'body' ? 1 : 0);
    assert.equal(released, phase === 'body' ? 1 : 0);
    b.reply.resolve(response('B'));
    await flush();
    assert.deepEqual(b.events.find(([kind]) => kind === 'chunk'), ['chunk', 7, [66]]);
    assert.equal(b.host.managedHttp.resources.size, 0);
  });
}

for (const phase of ['response', 'body']) {
  for (const reject of [false, true]) {
    test(`owner switch during legacy HTTP ${phase} (${reject ? 'reject' : 'resolve'}) cannot settle B request 7`, async (t) => {
      function owner(name, bodyWait = false) {
        const held = deferred(), events = [];
        let signal, closed = false;
        const host = new EngineHost({
          httpStart(id) { events.push(['start', id]); return ''; },
          httpChunk(id, bytes) { events.push(['chunk', id, [...bytes]]); return ''; },
          httpEnd(id) { events.push(['end', id]); return ''; },
          httpFailed(id) { events.push(['failed', id]); return ''; },
        }, {
          now: () => 0,
          request(request) {
            assert.equal(request.body, name);
            signal = request.signal;
            if (!bodyWait) return held.promise;
            return { status: 200, body: (async function* () {
              try { yield new Uint8Array([65]); yield await held.promise; }
              finally { closed = true; }
            })() };
          },
        });
        t.after(() => host.dispose());
        host.execute({ op: 'http', ...packet(name) });
        return { host, events, held, signal: () => signal, closed: () => closed };
      }
      const a = owner('A', phase === 'body');
      await flush();
      assert.deepEqual(a.events, phase === 'body' ? [['start', 7], ['chunk', 7, [65]]] : []);
      a.host.dispose();
      assert.equal(a.signal().aborted, true);
      const aBefore = structuredClone(a.events);
      const b = owner('B');
      const bRequest = b.host.requests.get(7);
      assert.ok(bRequest);
      if (reject) a.held.reject(new Error('old A request failed'));
      else a.held.resolve(phase === 'body' ? new Uint8Array([65]) : response('A'));
      await flush();
      assert.deepEqual(a.events, aBefore);
      assert.deepEqual(b.events, []);
      assert.equal(b.host.requests.get(7), bRequest);
      assert.equal(a.host.requests.size, 0);
      if (phase === 'body') assert.equal(a.closed(), true);
      b.held.resolve(response('B'));
      await flush();
      assert.deepEqual(b.events, [['start', 7], ['chunk', 7, [66]], ['end', 7]]);
      assert.equal(b.host.requests.size, 0);
    });
  }
}

const work = { receipt: {}, context: {}, operations: [{ kind: 'present', key: 'account/primary' }] };
const sign = { op: 'sign', attempt: 7, work, timeoutMs: 1000 };
function signingOwner(owner) {
  const provider = deferred(), events = [], timers = new Map();
  let signal;
  const host = new EngineHost({
    useExternalSigner() {},
    signed(id, value) { events.push(['signed', id, JSON.parse(value)]); return ''; },
    signFailed(...args) { events.push(['failed', ...args]); return ''; },
    addressSetupRetired(token) { events.push(['retired', token]); return ''; },
    prepareHostedSigning() { return JSON.stringify({ id: 7, work, deadlineMs: 1000 }); },
    finishHostedSigning(id, value) { events.push(['hosted', id, JSON.parse(value)]); return JSON.stringify({ owner }); },
    cancelHostedSigning(id) { events.push(['cancelHosted', id]); },
  }, {
    now: () => 0,
    setTimeout(fn) { timers.set(1, fn); return 1; },
    clearTimeout(id) { timers.delete(id); },
  }, {}, { signer: (_, context) => { signal = context.signal; return provider.promise; } });
  return { host, provider, events, timers, signal: () => signal };
}

for (const phase of ['provider', 'retirement join', 'hosted capture']) {
  for (const reject of [false, true]) {
    test(`owner switch during signing ${phase} (${reject ? 'reject' : 'resolve'}) cannot settle B's equal attempt`, async (t) => {
      const a = signingOwner('A');
      t.after(() => a.host.dispose());
      let aOutcome;
      if (phase === 'hosted capture') aOutcome = a.host.hostedSigning({}).then(() => assert.fail('A capture completed'), (error) => error);
      else a.host.execute(sign);
      await flush();
      const staleTimeout = a.timers.get(1);
      assert.equal(typeof staleTimeout, 'function');
      if (phase === 'retirement join') {
        // Model the negotiated engine retirement action; the native admission
        // handshake is covered separately by hostcommand tests.
        a.host.addressSetupNegotiated = true;
        a.host.execute({ op: 'retireAddressSetup', task: 7, token: 'same-retirement-token' });
        await flush();
        assert.equal(a.host.signingJoins.has(7), true, 'provider is still running');
        assert.equal(a.events.some(([kind]) => kind === 'retired'), false);
      }
      a.host.dispose();
      assert.equal(a.signal().aborted, true);
      if (aOutcome) assert.ok(await aOutcome instanceof Error);
      const aBefore = structuredClone(a.events);
      const b = signingOwner('B');
      t.after(() => b.host.dispose());
      let bOutcome;
      if (phase === 'hosted capture') bOutcome = b.host.hostedSigning({});
      else b.host.execute(sign);
      await flush();
      const map = phase === 'hosted capture' ? b.host.hostedSignings : b.host.primitiveSignings;
      const bPending = map.get(7);
      assert.ok(bPending);
      const bJoin = b.host.signingJoins.get(7);
      staleTimeout();
      if (reject) a.provider.reject(new Error('old A signer failed'));
      else a.provider.resolve({ receipt: {}, values: [[65]] });
      await flush();
      assert.deepEqual(a.events, aBefore);
      assert.deepEqual(b.events, []);
      assert.equal(map.get(7), bPending);
      assert.equal(b.host.signingJoins.get(7), bJoin);
      assert.equal(b.timers.size, 1, 'A cleanup cannot clear B timeout handle 1');
      assert.equal(a.host.signingJoins.size, 0);
      b.provider.resolve({ receipt: {}, values: [[66]] });
      if (bOutcome) assert.deepEqual(await bOutcome, { owner: 'B' });
      await flush();
      assert.deepEqual(b.events, [[phase === 'hosted capture' ? 'hosted' : 'signed', 7, { receipt: {}, values: [[66]] }]]);
      assert.equal(map.size, 0);
      assert.equal(b.host.signingJoins.size, 0);
      assert.equal(b.timers.size, 0);
    });
  }
}

test('already queued A timer cannot consume B timer with the same ID and handle', (t) => {
  function owner() {
    const events = [], callbacks = new Map();
    const host = new EngineHost({ timer(id) { events.push(id); return ''; } }, {
      now: () => 100,
      setTimeout(fn) { callbacks.set(1, fn); return 1; },
      clearTimeout(id) { callbacks.delete(id); },
    });
    t.after(() => host.dispose());
    host.execute({ op: 'setTimer', timer: 7, at: 100 });
    return { host, events, wake: callbacks.get(1) };
  }
  const a = owner();
  a.host.dispose();
  const b = owner();
  a.wake(); a.wake();
  assert.deepEqual(a.events, []);
  assert.deepEqual(b.events, []);
  assert.equal(b.host.timers.get(7), 1);
  b.wake(); b.wake();
  assert.deepEqual(b.events, [7]);
  assert.equal(b.host.timers.size, 0);
});

test('owner switch during algo stop-all abandons A and preserves B waiter and queued wakeup', async (t) => {
  function owner(name) {
    const events = [], timers = new Map();
    let emits = [], deadline = 100;
    const report = { stoppedChasers: [name], stoppedTwaps: [], settled: true };
    const host = new EngineHost({
      algoStart() {},
      algoRunning: () => true,
      algoDeadline: () => deadline,
      algoStopAll() { events.push(['stop', name]); return ''; },
      algoAdvance() {
        events.push(['advance', name]);
        deadline = undefined;
        emits.push({ op: 'algos:stopped', data: report }, { op: 'store', data: name });
        return '';
      },
      algoEmits() { const output = JSON.stringify(emits); emits = []; return output; },
    }, {
      now: () => 100,
      setTimeout(fn) { timers.set(1, fn); return 1; },
      clearTimeout(id) { timers.delete(id); },
    }, { onAlgoEmit: (op, data) => events.push([op, data]) });
    t.after(() => host.dispose());
    host.algoStart(name);
    return { host, events, timers, report };
  }
  const a = owner('A');
  const aWait = a.host.algoStopAll();
  const staleWake = a.timers.get(1);
  assert.equal(typeof staleWake, 'function');
  assert.equal(a.host.algoStopAllWaiters.length, 1);
  a.host.dispose();
  assert.deepEqual(await aWait, { stoppedChasers: [], stoppedTwaps: [], settled: false });
  const aBefore = structuredClone(a.events);
  const b = owner('B');
  let bSettled = false;
  const bWait = b.host.algoStopAll().then((report) => { bSettled = true; return report; });
  const bWake = b.timers.get(1);
  staleWake(); staleWake();
  await flush();
  assert.deepEqual(a.events, aBefore);
  assert.deepEqual(b.events, [['stop', 'B']]);
  assert.equal(bSettled, false);
  assert.equal(b.host.algoStopAllWaiters.length, 1);
  assert.equal(b.timers.get(1), bWake);
  bWake();
  assert.deepEqual(await bWait, b.report);
  assert.deepEqual(b.events, [['stop', 'B'], ['advance', 'B'], ['store', 'B']]);
  assert.equal(b.host.algoStopAllWaiters.length, 0);
  assert.equal(b.host.algoTimer, undefined);
});

for (const reject of [false, true]) {
  test(`owner switch during Blob decode (${reject ? 'reject' : 'resolve'}) preserves B's reused socket`, async (t) => {
    function owner() {
      let callbacks;
      const events = [];
      const host = new EngineHost({
        wsOpen(id) { events.push(['open', id]); return ''; },
        wsMessage(id, text) { events.push(['message', id, text]); return ''; },
        wsClose(id) { events.push(['close', id]); return ''; },
      }, {
        now: () => 0,
        connect(_, handlers) { callbacks = handlers; return { close() {} }; },
      });
      t.after(() => host.dispose());
      host.execute({ op: 'wsConnect', socket: 7, url: 'wss://venue.test', channel: 'private' });
      callbacks.onOpen();
      // Real arrival-ordered decoder, with final delivery through the host's
      // captured socket callbacks, including its error/close continuation.
      const receive = socketMessages(callbacks.onMessage, () => callbacks.onClose(1003));
      return { host, events, callbacks, receive };
    }
    const held = deferred();
    let decoding = false;
    const blob = new Blob(['A']);
    blob.arrayBuffer = () => { decoding = true; return held.promise; };
    const a = owner();
    a.receive(blob);
    a.receive('queued A text');
    await flush();
    assert.equal(decoding, true);
    a.host.dispose();
    const aBefore = structuredClone(a.events);
    const b = owner();
    const socket = b.host.sockets.get(7);
    const bBefore = structuredClone(b.events);
    if (reject) held.reject(new Error('old A decode failed'));
    else held.resolve(new TextEncoder().encode('A').buffer);
    await flush();
    a.callbacks.onClose(1006);
    a.callbacks.onOpen();
    assert.deepEqual(a.events, aBefore);
    assert.deepEqual(b.events, bBefore);
    assert.equal(b.host.sockets.get(7), socket);
    assert.equal(b.host.socketStats.private.open, 1);
    assert.equal(b.host.socketStats.private.initialized, 0);
    b.receive('B');
    assert.deepEqual(b.events, [['open', 7], ['message', 7, 'B']]);
    assert.equal(b.host.socketStats.private.initialized, 1);
  });
}
