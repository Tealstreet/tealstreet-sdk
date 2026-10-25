import assert from 'node:assert/strict';
import test from 'node:test';
import { EngineHost } from './host.mjs';

const tick = async () => { for (let i = 0; i < 30; i++) await Promise.resolve(); };
const tokens = [7, 8].map((id) => `p1:${id}:1:1`);
const permit = 'g1;r1:7:1:1:1;r1:8:1:2:1';
const packet = (id) => ({ request: id, method: 'POST', url: `https://venue.test/${id}`, headers: [], timeoutMs: 1000, body: '{}' });
const binding = { op: 'managedHttp', phase: 'bindGroup', permit, members: tokens.map((preparation, i) => ({ preparation, packet: packet(7 + i) })) };
const cancel = () => JSON.stringify(tokens.map((preparation) => ({ op: 'managedHttp', phase: 'cancel', preparation })));
function rig(options = {}) {
  const events = [], timers = new Map();
  let now = 0, next = 0, prepared = 0;
  const engine = {
    httpPreparationPending: (token) => tokens.includes(token),
    httpGroupPending: (value) => value === permit,
    httpPrepared(token) { events.push(['prepared', token]); return ++prepared === 2 ? JSON.stringify([binding]) : ''; },
    httpPreparationFailed(token, code) { events.push(['preparationFailed', token, code]); return cancel(); },
    httpClaimGroup(value, time, bounds) {
      events.push(['claim', value, time]);
      assert.equal(bounds.length, 2);
      assert.equal(bounds.buffer instanceof ArrayBuffer, true);
      assert.equal(events.filter(([kind]) => kind === 'send').length, 0);
      if (options.claim) return options.claim(bounds);
      bounds.fill(1000); return 1;
    },
    prepareDispatch() { events.push(['drain']); return ''; },
    httpFailed(id) { events.push(['httpFailed', id]); return ''; },
    timer() { events.push(['timer']); return ''; },
    wsOpen() { events.push(['socketOpen']); return ''; },
    wsClose(_, code) { events.push(['socketClosed', code]); return ''; },
    wsMessage(_, text) { events.push(['text', text]); return ''; },
    wsBinary(_, data) { events.push(['binary', [...data]]); return ''; },
  };
  let host, socketCallbacks;
  const transport = {
    now: () => now,
    connect(_, callbacks) { socketCallbacks = callbacks; return { close() { events.push(["socketDispose"]); } }; },
    request() { assert.fail('legacy request forbidden'); },
    async prepareHttp(request) {
      return { bind() {
        if (options.bindFailure === request.request) throw new Error('private bind detail');
        return {
          ready() { events.push(['ready', request.request]); return options.ready?.(host, request.request) ?? true; },
          dispatch() {
            events.push(['send', request.request]);
            options.dispatch?.(host, request.request);
            return new Promise(() => {});
          },
          cancel() { events.push(['cancel', request.request]); },
        };
      } };
    },
    setTimeout(fn, delay) { const id = ++next; timers.set(id, { fn, at: now + delay }); return id; },
    clearTimeout(id) { timers.delete(id); },
  };
  host = new EngineHost(engine, transport);
  return { host, events, timers, socket() { return socketCallbacks; }, openSocket() { host.execute({ op: "wsConnect", socket: 42, url: "wss://venue.test", channel: "public" }); socketCallbacks.onOpen(); }, at(value) { now = value; }, async start() {
    for (let i = 0; i < 2; i++) host.execute({ op: 'managedHttp', phase: 'prepare', preparation: tokens[i], intent: 'primary', packet: packet(7 + i) });
    await tick();
  } };
}

test('all resources are checked before one claim and both sends precede callbacks', async () => {
  const r = rig(); await r.start();
  const events = r.events.map(([kind]) => kind);
  assert.deepEqual(events, ['prepared', 'prepared', 'ready', 'ready', 'claim', 'send', 'send', 'drain']);
  r.host.dispose();
});

for (const attack of ['dispose', 'failEngine', 'cleanup', 'cancel', 'run', 'invoke', 'execute', 'command', 'settleCommand', 'hostExecute', 'reconnect']) {
  test(`first dispatch cannot interrupt its sibling through ${attack}`, async () => {
    const r = rig({ dispatch(host, id) {
      if (id !== 7) return;
      assert.throws(() => {
        if (attack === 'dispose') host.dispose();
        if (attack === 'failEngine') host.failEngine(new Error('hostile fatal call'));
        if (attack === 'command') host.command({ op: 'query' });
        if (attack === 'reconnect') host.reconnectSockets();
        if (attack === 'settleCommand') host.settleCommand(99, 'forged');
        if (attack === 'hostExecute') host.execute({ op: 'http', request: 99 });
        if (attack === 'cleanup') host.cleanup();
        if (attack === 'cancel') host.managedHttp.cancel(tokens[1]);
        if (attack === 'run') host.run('[]');
        if (attack === 'invoke') host.invoke(host.engine.timer, 1, 0);
        if (attack === 'execute') host.managedHttp.execute({ phase: 'cancel', preparation: tokens[1] });
      }, /cannot be interrupted/);
      assert.equal(host.disposed, false);
      assert.equal(host.dead, false);
      assert.equal(host.commands.size, 1);
      if (attack === 'reconnect') { assert.equal(host.sockets.has(42), true); assert.equal(host.socketStats.public.open, 1); }
      assert.equal(host.managedHttp.resources.size, 2);
    } });
    if (attack === 'reconnect') {
      r.host.socketStats.public.open = 1;
      r.host.socketRecords.set(42, { open: true, initialized: false, channel: 'public', stats: r.host.socketStats.public });
      r.host.sockets.set(42, { close() { r.events.push(['socketClose']); } });
    }
    r.host.commands.set(99, { resolve() { assert.fail('command settled inside handoff'); }, reject() {} });
    await r.start();
    assert.deepEqual(r.events.filter(([kind]) => kind === 'send'), [['send', 7], ['send', 8]]);
    assert.equal(r.events.some(([kind]) => kind === 'cancel' || kind === 'timer'), false);
    r.host.dispose();
  });
}

test('a second bind failure emits no claim and no transport send', async () => {
  const r = rig({ bindFailure: 8 }); await r.start();
  assert.equal(r.events.some(([kind]) => kind === 'claim' || kind === 'send'), false);
  assert.equal(r.events.filter(([kind]) => kind === 'preparationFailed').length, 1);
  assert.equal(r.host.managedHttp.resources.size, 0);
  r.host.dispose();
});

test('a synchronous first send exception still hands off the second before failure delivery', async () => {
  const r = rig({ dispatch(_, id) { if (id === 7) throw new Error('private transport detail'); } });
  await r.start();
  const relevant = r.events.filter(([kind]) => ['send', 'httpFailed', 'preparationFailed'].includes(kind));
  assert.deepEqual(relevant, [['send', 7], ['send', 8], ['httpFailed', 7]]);
  r.host.dispose();
});

test('claim/output exceptions fail-stop without a false preparation refusal', async () => {
  const r = rig({ claim() { throw new Error('output copy failed after commit'); } }); await r.start();
  assert.equal(r.host.dead, true);
  assert.equal(r.events.some(([kind]) => kind === 'preparationFailed' || kind === 'send'), false);
  r.host.dispose();
});

test('wait uses only slot zero and rechecks all resources at wake', async () => {
  let ready = true;
  const r = rig({ claim(bounds) { bounds[0] = 250; bounds[1] = Number.NaN; return 2; }, ready() { return ready; } });
  await r.start();
  assert.equal(r.events.some(([kind]) => kind === 'send'), false);
  assert.equal(r.timers.size, 1);
  ready = false; r.at(250);
  [...r.timers.values()][0].fn();
  assert.equal(r.events.filter(([kind]) => kind === 'preparationFailed').length, 1);
  assert.equal(r.events.some(([kind]) => kind === 'send'), false);
  r.host.dispose();
});

for (const clock of [Number.NaN, Number.NEGATIVE_INFINITY]) {
  test(`an invalid clock after first send refuses the sibling as post-release failure (${clock})`, async () => {
    let r;
    r = rig({ dispatch(_, id) { if (id === 7) r.at(clock); } });
    await r.start();
    assert.deepEqual(r.events.filter(([kind]) => ['send', 'httpFailed', 'preparationFailed'].includes(kind)), [['send', 7], ['httpFailed', 8]]);
    r.host.dispose();
  });
}


test('captured close defers the whole notification until both handoffs', async () => {
  let callbacks;
  const r = rig({ dispatch(host, id) {
    if (id !== 7) return;
    callbacks.onClose(1006);
    assert.equal(host.sockets.has(42), true);
    assert.equal(host.socketRecords.has(42), true);
    assert.equal(host.socketStats.public.open, 1);
    assert.equal(host.socketStats.public.closes, 0);
  } });
  r.openSocket(); callbacks = r.socket();
  await r.start();
  assert.deepEqual(r.events.filter(([kind]) => ['send', 'socketClosed'].includes(kind)), [['send', 7], ['send', 8], ['socketClosed', 1006]]);
  assert.equal(r.host.sockets.has(42), false);
  assert.equal(r.host.socketStats.public.closes, 1);
  assert.equal(r.host.socketStats.public.open, 0);
  r.host.dispose();
});
test('captured text preserves notification order without early health effects', async () => {
  let callbacks;
  const r = rig({ dispatch(host, id) {
    if (id !== 7) return;
    callbacks.onMessage('first'); callbacks.onMessage('second');
    assert.equal(host.socketStats.public.initialized, 0);
    assert.equal(host.socketStats.public.lastReceived, 0);
  } });
  r.openSocket(); callbacks = r.socket();
  await r.start();
  assert.deepEqual(r.events.filter(([kind]) => ['send', 'text'].includes(kind)), [['send', 7], ['send', 8], ['text', 'first'], ['text', 'second']]);
  assert.equal(r.host.socketStats.public.initialized, 1);
  r.host.dispose();
});
test('borrowed binary notification fails after all sends without retaining or decoding it', async () => {
  let callbacks;
  const bytes = new Uint8Array([1, 2]);
  const r = rig({ dispatch(host, id) {
    if (id !== 7) return;
    callbacks.onMessage('must not replay');
    callbacks.onBinary(bytes);
    bytes.fill(99);
    assert.equal(host.dead, false);
    assert.equal(host.socketStats.public.lastReceived, 0);
  } });
  r.openSocket(); callbacks = r.socket();
  await r.start();
  assert.deepEqual(r.events.filter(([kind]) => kind === 'send'), [['send', 7], ['send', 8]]);
  assert.equal(r.events.some(([kind]) => ['binary', 'text', 'preparationFailed'].includes(kind)), false);
  assert.equal(r.host.dead, true);
});
test('bounded control notification overflow fails after every handoff and before replay', async () => {
  let callbacks;
  const r = rig({ dispatch(host, id) {
    if (id !== 7) return;
    for (let i = 0; i < 257; i++) callbacks.onMessage('queued');
    assert.equal(host.dead, false);
    assert.equal(host.socketStats.public.lastReceived, 0);
  } });
  r.openSocket(); callbacks = r.socket();
  await r.start();
  assert.deepEqual(r.events.filter(([kind]) => kind === 'send'), [['send', 7], ['send', 8]]);
  assert.equal(r.events.some(([kind]) => ['text', 'preparationFailed'].includes(kind)), false);
  assert.equal(r.host.dead, true);
});
test('ordinary binary delivery outside a handoff stays immediate', () => {
  const r = rig(); r.openSocket();
  r.socket().onBinary(new Uint8Array([1, 2]));
  assert.deepEqual(r.events.filter(([kind]) => kind === 'binary'), [['binary', [1, 2]]]);
  assert.equal(r.host.dead, false);
  r.host.dispose();
});

for (const missing of ['httpGroupPending', 'httpClaimGroup']) {
  test(`a host without ${missing} refuses before any bind or claim`, async () => {
    const r = rig();
    r.host.engine[missing] = undefined;
    const prepare = r.host.transport.prepareHttp;
    r.host.transport.prepareHttp = async (...args) => {
      const resource = await prepare(...args);
      resource.bind = () => assert.fail('unsupported group cannot bind a resource');
      return resource;
    };
    await r.start();
    assert.equal(r.events.some(([kind]) => kind === 'claim' || kind === 'send'), false);
    assert.deepEqual(r.events.filter(([kind]) => kind === 'preparationFailed'), [['preparationFailed', tokens[0], 'unsupported']]);
    assert.equal(r.host.managedHttp.resources.size, 0);
    r.host.dispose();
  });
}
