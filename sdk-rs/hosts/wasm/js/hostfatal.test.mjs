import assert from 'node:assert/strict';
import test from 'node:test';
import { EngineDeadError, EngineHost } from './host.mjs';

const settle = () => new Promise(setImmediate);
const socketAction = { op: 'wsConnect', socket: 7, url: 'wss://example.test' };
const methods = ['start', 'command', 'takeFrame', 'snapshotFrame', 'capabilities', 'wsOpen', 'wsMessage',
  'wsMessageActions', 'wsMessageBytes', 'wsBinary', 'wsClose', 'wsRouteExhausted', 'timer', 'httpStart', 'httpChunk',
  'httpEnd', 'httpFailed'];

function rig(overrides = {}, transportOverrides = {}, handlers = {}) {
  const calls = [];
  const sockets = [];
  const timers = new Map();
  const deaths = [];
  const engine = Object.fromEntries(methods.map((name) => [name, function () {
    assert.equal(this, engine);
    calls.push(name);
    return name === 'snapshotFrame' ? new Uint8Array(8) : name === 'takeFrame' ? undefined : name === 'capabilities' ? '{}' : '';
  }]));
  Object.assign(engine, overrides);
  let nextTimer = 0;
  const transport = {
    now: () => 123,
    connect: (_url, callbacks) => {
      const socket = { callbacks, closed: 0, send() {}, close() { this.closed++; callbacks.onClose(); } };
      sockets.push(socket);
      return socket;
    },
    setTimeout: (fn) => { timers.set(++nextTimer, fn); return nextTimer; },
    clearTimeout: (handle) => timers.delete(handle),
    request: async () => ({ status: 200, body: new Uint8Array([1]) }),
    ...transportOverrides,
  };
  const host = new EngineHost(engine, transport, { onDead: (error) => deaths.push(error), ...handlers });
  return { host, engine, transport, calls, sockets, timers, deaths };
}

for (const method of methods) {
  test(`a native ${method} trap terminates the host and suppresses late entry`, async () => {
    const trap = new WebAssembly.RuntimeError('unreachable');
    let entries = 0;
    const r = rig({ [method]() { entries++; throw trap; } });
    // Exercise all supported WS ingress variants rather than only the preferred byte entry.
    if (method === 'wsMessage') { delete r.engine.wsMessageActions; delete r.engine.wsMessageBytes; }
    if (method === 'wsMessageActions') delete r.engine.wsMessageBytes;
    r.host.execute(socketAction);
    const callbacks = r.sockets[0].callbacks;
    r.host.execute({ op: 'setTimer', timer: 3, at: 124 });
    const timer = r.timers.values().next().value;
    if (method === 'start') r.host.start();
    else if (method === 'command') await assert.rejects(r.host.command({ op: 'getAccount' }), EngineDeadError);
    else if (method === 'takeFrame') r.host.flush();
    else if (method === 'snapshotFrame') assert.throws(() => r.host.snapshot(), EngineDeadError);
    else if (method === 'capabilities') assert.throws(() => r.host.capabilities(), EngineDeadError);
    else if (method === 'wsOpen') callbacks.onOpen();
    else if (method.startsWith('wsMessage')) callbacks.onMessage('payload');
    else if (method === 'wsBinary') callbacks.onBinary(new Uint8Array([1]));
    else if (method === 'wsClose') callbacks.onClose(1006);
    else if (method === 'wsRouteExhausted') callbacks.onRouteExhausted();
    else if (method === 'timer') timer();
    else if (method === 'httpFailed') await r.host.request({ request: 1, headers: [], url: 'https://example.test',
      get method() { throw new Error('request setup failed'); } });
    else if (method.startsWith('http')) await r.host.receive(1, { status: 200, body: new Uint8Array([1]) });
    assert.equal(entries, 1);
    assert.equal(r.host.dead, true);
    assert.equal(r.host.disposed, true);
    assert.equal(r.deaths.length, 1);
    assert.equal(r.deaths[0], r.host.deadError);
    assert.equal(r.host.deadError.cause, trap);
    assert.equal(r.host.deadError.kind, 'internal');
    assert.equal(r.host.wsScratchBusy, false);
    const count = r.calls.length;
    callbacks.onOpen(); callbacks.onMessage('late'); callbacks.onBinary(new Uint8Array([2])); callbacks.onClose();
    callbacks.onRouteExhausted();
    timer(); r.host.start(); r.host.flush(); r.host.dispose();
    await r.host.receive(2, { status: 200, body: new Uint8Array([2]) });
    await assert.rejects(r.host.command({ op: 'placeOrders', orders: [] }), EngineDeadError);
    assert.throws(() => r.host.snapshot(), EngineDeadError);
    assert.equal(r.calls.length, count);
    assert.equal(entries, 1);
    assert.equal(r.deaths.length, 1);
    assert.equal(r.host.sockets.size, 0);
    assert.equal(r.host.timers.size, 0);
  });
}

test('a native stack overflow (V8 RangeError) is contained like a trap', async () => {
  // wasm stack overflow surfaces as RangeError, not RuntimeError; it unwinds the
  // engine mid-mutation with its wasm-bindgen borrow held, so it cannot be reused.
  const overflow = new RangeError('Maximum call stack size exceeded');
  const r = rig({ command() { throw overflow; } });
  await assert.rejects(r.host.command({ op: 'placeOrders', orders: [{ clientOrderId: 'c-1' }] }), (error) =>
    error instanceof EngineDeadError && error.kind === 'internal' && error.outcome === 'unknown');
  assert.equal(r.host.dead, true);
  assert.equal(r.host.deadError.cause, overflow);
  assert.equal(r.deaths.length, 1);
  await assert.rejects(r.host.command({ op: 'getAccount' }), EngineDeadError);
});

test('real WASM unreachable is contained without poisoning a sibling host', async () => {
  // (module (func (export "trap") unreachable)); no compiler or production panic API.
  const bytes = new Uint8Array([0, 97, 115, 109, 1, 0, 0, 0, 1, 4, 1, 96, 0, 0,
    3, 2, 1, 0, 7, 8, 1, 4, 116, 114, 97, 112, 0, 0, 10, 5, 1, 3, 0, 0, 11]);
  const { instance } = await WebAssembly.instantiate(bytes);
  const failed = rig({ wsMessageBytes: instance.exports.trap });
  const sibling = rig({ command: (json) => JSON.stringify([{ op: 'done', command: JSON.parse(json).command, result: '"live"' }]) });
  failed.host.execute(socketAction);
  failed.sockets[0].callbacks.onMessage('trap');
  assert.ok(failed.host.deadError.cause instanceof WebAssembly.RuntimeError);
  assert.equal(failed.deaths.length, 1);
  assert.equal(await sibling.host.command({ op: 'getAccount' }), 'live');
  sibling.host.execute(socketAction);
  sibling.sockets[0].callbacks.onMessage('still live');
  assert.equal(sibling.host.dead, false);
  assert.equal(sibling.deaths.length, 0);
  sibling.host.dispose();
});

test('fatal cleanup settles all pending operations with unknown write outcomes despite broken cleanup', async () => {
  let httpSignal;
  let finishHttp;
  let closes = 0;
  let cleared = 0;
  let disposed = 0;
  const r = rig({ start() { throw new WebAssembly.RuntimeError('unreachable'); } }, {
    request: ({ signal }) => { httpSignal = signal; return new Promise((resolve) => { finishHttp = resolve; }); },
    connect: () => ({ close() { closes++; throw new Error('close failed'); } }),
    clearTimeout: () => { cleared++; throw new Error('clear failed'); },
    dispose: () => { disposed++; throw new Error('dispose failed'); },
  });
  r.host.execute(socketAction);
  r.host.execute({ ...socketAction, socket: 8 });
  r.host.execute({ op: 'setTimer', timer: 4, at: 124 });
  const receive = r.host.request({ request: 1, method: 'POST', headers: [], url: 'https://example.test' });
  await settle();
  const pending = Promise.allSettled([
    r.host.command({ op: 'placeOrders', orders: [{ clientOrderId: 'caller-1' }] }),
    r.host.command({ op: 'placeOrders', orders: [{ clientOrderId: 'caller-2' }, { clientOrderId: 'caller-3' }] }),
    r.host.command({ op: 'cancelOrders', ids: ['order-1'] }),
    r.host.command({ op: 'getAccount' }),
  ]);
  r.host.start();
  const results = await pending;
  assert.ok(results.every((result) => result.status === 'rejected' && result.reason instanceof EngineDeadError));
  // A trap is the engine's own fault, never the venue's: callers branch on `kind`.
  assert.ok(results.every((result) => result.reason.kind === 'internal'));
  assert.equal(results[0].reason.outcome, 'unknown');
  assert.equal(results[0].reason.clientOrderId, 'caller-1');
  assert.deepEqual(results[1].reason.clientOrderIds, ['caller-2', 'caller-3']);
  assert.equal(results[2].reason.outcome, 'unknown');
  assert.equal(results[3].reason.outcome, undefined);
  assert.equal(r.host.commands.size, 0);
  assert.equal(httpSignal.aborted, true);
  assert.equal(closes, 2);
  // The native timer and all four pending command deadlines are cleared.
  assert.equal(cleared, 5);
  assert.equal(disposed, 1);
  assert.equal(r.deaths.length, 1);
  const count = r.calls.length;
  finishHttp({ status: 200, body: new Uint8Array([1]) });
  await receive;
  await settle();
  assert.equal(r.calls.length, count);
  assert.equal(r.deaths.length, 1);
});

test('validation, transport and user callback errors do not kill a healthy engine', async () => {
  const r = rig({ command() { throw new Error('invalid caller id'); }, takeFrame: () => new Uint8Array(1) }, {
    request: async () => { throw new WebAssembly.RuntimeError('a transport error is not a native trap'); },
  }, { onFrame() { throw new WebAssembly.RuntimeError('a consumer callback is not a native trap'); } });
  await assert.rejects(r.host.command({ op: 'placeOrders', orders: [] }), /invalid caller id/);
  await r.host.request({ request: 1, method: 'GET', url: 'https://example.test', headers: [] });
  assert.ok(r.calls.includes('httpFailed'));
  assert.throws(() => r.host.flush(), /consumer callback/);
  r.host.start();
  assert.equal(r.host.dead, false);
  assert.equal(r.deaths.length, 0);
  r.host.dispose();
});

test('a synchronous open trap closes the socket returned after fatal cleanup', () => {
  let closed = 0;
  const r = rig({ wsOpen() { throw new WebAssembly.RuntimeError('unreachable'); } }, {
    connect: (_url, callbacks) => { callbacks.onOpen(); return { close() { closed++; } }; },
  });
  r.host.execute(socketAction);
  assert.equal(closed, 1);
  assert.equal(r.host.sockets.size, 0);
  assert.equal(r.deaths.length, 1);
});

test('a sticky failed engine is retired before native re-entry', () => {
  let entries = 0;
  const r = rig({ failed: () => true, start() { entries++; return JSON.stringify([socketAction]); } });
  r.host.start();
  assert.equal(entries, 0);
  assert.equal(r.host.dead, true);
  assert.equal(r.deaths.length, 1);
  assert.equal(r.sockets.length, 0);
  r.host.start();
  assert.equal(r.deaths.length, 1);
});

test('sticky failure on normal native return suppresses that turn of actions', () => {
  let failed = false;
  let entries = 0;
  let armed = 0;
  const r = rig({
    failed: () => failed,
    start() { entries++; failed = true; return JSON.stringify([socketAction, { op: 'setTimer', timer: 9, at: 124 }]); },
  }, { setTimeout() { armed++; return 1; } });
  r.host.start();
  assert.equal(entries, 1);
  assert.equal(failed, true);
  assert.equal(armed, 0);
  assert.equal(r.host.dead, true);
  assert.equal(r.deaths.length, 1);
  assert.equal(r.sockets.length, 0);
  assert.equal(r.timers.size, 0);
});
