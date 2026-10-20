import assert from 'node:assert/strict';
import test from 'node:test';
import { createServer } from 'node:net';
import { EngineHost, nativeTransport } from './host.mjs';

test('required upgrade headers are forwarded only to capable transports', () => {
  for (const capable of [false, true]) {
    const connected = [];
    const closed = [];
    const engine = { wsClose: (...args) => (closed.push(args), '') };
    const transport = {
      webSocketHeaders: capable,
      now: () => 100,
      connect: (...args) => (connected.push(args), { close() {}, send() {} }),
      setTimeout: (fn) => fn(),
    };
    const host = new EngineHost(engine, transport);
    host.execute({ op: 'wsConnect', socket: 7, url: 'wss://example.test/private', headers: [['ACCESS-SIGN', 'signature']] });
    if (capable) {
      assert.equal(connected.length, 1);
      assert.deepEqual(connected[0][1].headers, { 'ACCESS-SIGN': 'signature' });
      assert.equal(closed.length, 0);
    } else {
      assert.equal(connected.length, 0);
      assert.deepEqual(closed, [[7, 1006, 100]]);
    }
  }
});

for (const closeBeforeOpen of [false, true]) {
  test(`Node socket reports ${closeBeforeOpen ? 'close before open' : 'a refused upgrade'} once without an unhandled error`, async () => {
    const server = createServer();
    await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
    const port = server.address().port;
    await new Promise((resolve) => server.close(resolve));
    let closes = 0;
    let timer;
    try {
      await new Promise((resolve, reject) => {
        timer = setTimeout(() => reject(new Error('socket close was not reported')), 3000);
        const socket = nativeTransport().connect(`ws://127.0.0.1:${port}`, {
          onOpen: () => reject(new Error('unexpected open')),
          onMessage() {},
          onClose: () => { closes++; resolve(); },
        });
        if (closeBeforeOpen) socket.close();
      });
      await new Promise(setImmediate);
      assert.equal(closes, 1);
    } finally { clearTimeout(timer); }
  });
}

test('disposing a host cannot schedule a reconnect through close callbacks', () => {
  let closeHandler;
  let closedEvents = 0;
  const host = new EngineHost({ wsClose: () => { closedEvents++; return ''; } }, {
    now: () => 100,
    clearTimeout() {},
    connect: (_, handlers) => { closeHandler = handlers.onClose; return { close: () => closeHandler(1000) }; },
  });
  host.execute({ op: 'wsConnect', socket: 1, url: 'wss://example.test' });
  host.dispose();
  closeHandler(1006);
  assert.equal(closedEvents, 0);
  assert.equal(host.sockets.size, 0);
});

test('unsupported-header fallback cancels on disposal and ignores a late task', () => {
  let callback;
  const tasks = new Set();
  let closed = 0;
  const host = new EngineHost({ wsClose: () => { closed++; return ''; } }, {
    now: () => 100,
    setTimeout(fn) { callback = fn; tasks.add(fn); return fn; },
    clearTimeout(handle) { tasks.delete(handle); },
  });
  host.execute({ op: 'wsConnect', socket: 7, url: 'wss://example.test/private', headers: [['ACCESS-SIGN', 'signature']] });
  assert.equal(tasks.size, 1);
  host.dispose();
  assert.equal(tasks.size, 0);
  callback();
  assert.equal(closed, 0);
});

test('unsupported-header fallback drains queued messages before reporting close', () => {
  let callback;
  const calls = [];
  const host = new EngineHost({
    wsMessage: () => { calls.push('messages'); return ''; },
    wsMessages: () => { calls.push('messages'); return ''; },
    wsClose: () => { calls.push('close'); return ''; },
  }, {
    now: () => 100,
    setTimeout(fn) { callback = fn; return fn; },
    clearTimeout() {},
  });
  host.message(1, 'earlier');
  host.execute({ op: 'wsConnect', socket: 7, url: 'wss://example.test/private', headers: [['ACCESS-SIGN', 'signature']] });
  callback();
  assert.deepEqual(calls, ['messages', 'close']);
  host.dispose();
});

test('native HTTP requests retain their lifetime timeout signal', async () => {
  const fetch = globalThis.fetch;
  const timeout = AbortSignal.timeout;
  const controller = new AbortController();
  let milliseconds;
  let request;
  try {
    AbortSignal.timeout = (ms) => { milliseconds = ms; return controller.signal; };
    globalThis.fetch = async (_url, init) => { request = init; return { status: 200, headers: new Map(), body: undefined }; };
    await nativeTransport().request({ method: 'GET', url: 'https://example.test', headers: {} });
    assert.equal(milliseconds, 20_000);
    assert.equal(request.signal, controller.signal);
  } finally {
    globalThis.fetch = fetch;
    AbortSignal.timeout = timeout;
  }
});
