import { test, mock } from 'node:test';
import assert from 'node:assert/strict';
import { deflateSync, gzipSync } from 'node:zlib';
import { createServer } from 'node:net';
import { EngineHost, bindSocketMessages, nativeTransport, socketMessages } from './host.mjs';

class MessageSocket {
  listeners = {};
  closes = 0;
  addEventListener(name, fn) {
    const previous = this.listeners[name];
    this.listeners[name] = previous ? (event) => { previous(event); fn(event); } : fn;
  }
  dispatch(name, event) { this.listeners[name]?.(event); }
  close() { this.closes++; this.dispatch('close', { code: 1000 }); }
}

const deferred = () => {
  let resolve;
  const promise = new Promise((done) => { resolve = done; });
  return { promise, resolve };
};

test('binary gzip messages and close reach engine in receive order', async () => {
  const original = globalThis.WebSocket;
  const builtin = process.getBuiltinModule ? mock.method(process, 'getBuiltinModule', () => undefined) : undefined;
  let ws;
  class FakeSocket extends MessageSocket {
    constructor() { super(); ws = this; }
    send() {}
    close() {}
  }
  globalThis.WebSocket = FakeSocket;
  try {
    const events = [];
    let finish;
    const closed = new Promise((resolve) => { finish = resolve; });
    nativeTransport().connect('wss://example.test/', {
      onOpen() {}, onMessage: (text) => events.push(text),
      onClose: (code) => { events.push(code); finish(); },
    });
    ws.listeners.message({ data: new Blob([gzipSync('{"book":1}')]) });
    ws.listeners.message({ data: '{"book":2}' });
    ws.listeners.close({ code: 1000 });
    await closed;
    assert.deepEqual(events, ['{"book":1}', '{"book":2}', 1000]);
  } finally { globalThis.WebSocket = original; builtin?.mock.restore(); }
});

test('binary ArrayBuffers, zlib and text preserve receive order', async () => {
  const socket = new MessageSocket();
  const events = [];
  const closed = deferred();
  bindSocketMessages(socket, {
    onMessage: (text) => events.push(text),
    onClose: (code) => { events.push(code); closed.resolve(); },
  });
  socket.dispatch('message', { data: 'synchronous' });
  assert.deepEqual(events, ['synchronous']);
  socket.dispatch('message', { data: new TextEncoder().encode('plain binary').buffer });
  socket.dispatch('message', { data: deflateSync('compressed €') });
  socket.dispatch('message', { data: 'last' });
  socket.dispatch('error', {});
  socket.dispatch('close', { code: 1000 });
  await closed.promise;
  assert.deepEqual(events, ['synchronous', 'plain binary', 'compressed €', 'last', 1006]);
});

test('disposing a native connection suppresses a late open event', () => {
  const original = globalThis.WebSocket;
  const builtin = process.getBuiltinModule ? mock.method(process, 'getBuiltinModule', () => undefined) : undefined;
  let socket;
  globalThis.WebSocket = class extends MessageSocket {
    constructor() { super(); socket = this; }
  };
  try {
    const events = [];
    const connection = nativeTransport().connect('wss://example.test/', {
      onOpen: () => events.push('open'),
      onMessage: (text) => events.push(text),
      onClose: (code) => events.push(code),
    });
    connection.close();
    socket.dispatch('open', {});
    socket.dispatch('message', { data: 'stale' });
    assert.deepEqual(events, [1000]);
  } finally { globalThis.WebSocket = original; builtin?.mock.restore(); }
});

test('failed binary decoding discards later queued frames and closes once', async () => {
  for (const data of [new Uint8Array([0xff]), new Uint8Array([0x1f, 0x8b, 0])]) {
    const socket = new MessageSocket();
    const events = [];
    const closed = deferred();
    bindSocketMessages(socket, {
      onMessage: (text) => events.push(text),
      onClose: () => closed.resolve(),
    });
    socket.dispatch('message', { data });
    socket.dispatch('message', { data: 'must not follow corruption' });
    await closed.promise;
    assert.deepEqual(events, []);
    assert.equal(socket.closes, 1);
  }
});

test('compressed messages exceeding the decoded size bound are rejected whole', async () => {
  const socket = new MessageSocket();
  const closed = deferred();
  const messages = [];
  bindSocketMessages(socket, {
    onMessage: (text) => messages.push(text),
    onClose: () => closed.resolve(),
  });
  socket.dispatch('message', { data: gzipSync(Buffer.alloc(16 * 1024 * 1024 + 1, 97)) });
  await closed.promise;
  assert.deepEqual(messages, []);
  assert.equal(socket.closes, 1);
});

test('disposing during binary decoding suppresses queued messages and closure', async () => {
  const socket = new MessageSocket();
  const started = deferred();
  const release = deferred();
  const events = [];
  class DelayedBlob extends Blob {
    async arrayBuffer() { started.resolve(); await release.promise; return super.arrayBuffer(); }
  }
  const dispose = bindSocketMessages(socket, {
    onMessage: (text) => events.push(text),
    onClose: (code) => events.push(code),
  });
  socket.dispatch('message', { data: new DelayedBlob([gzipSync('discard me')]) });
  socket.dispatch('message', { data: 'discard this too' });
  socket.dispatch('close', { code: 1000 });
  await started.promise;
  dispose();
  release.resolve();
  await new Promise((resolve) => setImmediate(resolve));
  assert.deepEqual(events, []);
});


test('command failures preserve partial placement metadata', async () => {
  const details = { message: 'second order rejected', partialOrderIds: ['accepted'], partialOrderCount: 1, totalOrderCount: 2, failedOrderIndex: 1 };
  const engine = {
    command(json) { const { command } = JSON.parse(json); return JSON.stringify([{ op: 'done', command, error: JSON.stringify(details) }]); },
  };
  const host = new EngineHost(engine, { now: () => 1 });
  await assert.rejects(host.command({ op: 'placeOrders', orders: [] }), (error) => {
    assert.equal(error.message, details.message);
    assert.deepEqual(error.partialOrderIds, details.partialOrderIds);
    assert.equal(error.failedOrderIndex, 1);
    assert.equal(error.totalOrderCount, 2);
    return true;
  });
});

test('invalid command parsing rejects and releases its pending completion', async () => {
  const host = new EngineHost({ command() { throw new Error('invalid caller client order id'); } }, { now: () => 1 });
  await assert.rejects(host.command({ op: 'placeOrders', orders: [] }), /invalid caller/);
  assert.equal(host.commands.size, 0);
});

test('settings preserve independent hedge leverage and decode older frames', async () => {
  const { Frame } = await import('./wire.mjs');
  const settings = (independent) => {
    const buffer = new ArrayBuffer(independent ? 64 : 48);
    new Uint8Array(buffer, 0, 4).set(new TextEncoder().encode('SCXF'));
    const view = new DataView(buffer);
    view.setUint16(4, 1, true); view.setUint16(6, 1, true);
    view.setUint16(16, 12, true); view.setUint32(20, independent ? 40 : 24, true);
    view.setUint32(24, 1, true); view.setUint32(28, 0, true); view.setUint32(32, 2, true);
    view.setFloat64(40, 20, true);
    if (independent) { view.setFloat64(48, 10, true); view.setFloat64(56, 15, true); }
    return new Frame(buffer).settings();
  };
  assert.equal(settings(true).buyLeverage[0], 10);
  assert.equal(settings(true).sellLeverage[0], 15);
  assert.equal(settings(false).buyLeverage[0], 20);
  assert.equal(settings(false).sellLeverage[0], 20);
});

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
    connect: (_, handlers) => { closeHandler = handlers.onClose; return { close: () => closeHandler(1000) }; },
  });
  host.execute({ op: 'wsConnect', socket: 1, url: 'wss://example.test' });
  host.dispose();
  closeHandler(1006);
  assert.equal(closedEvents, 0);
  assert.equal(host.sockets.size, 0);
});

test('disposing rejects pending commands and does not enter the engine again', async () => {
  let commands = 0;
  const host = new EngineHost({ command: () => { commands++; return ''; } }, { now: () => 100 });
  const result = assert.rejects(host.command({ op: 'placeOrders', orders: [] }), /engine disposed/);
  host.dispose();
  await result;
  await assert.rejects(host.command({ op: 'placeOrders', orders: [] }), /engine disposed/);
  assert.equal(commands, 1);
  assert.equal(host.commands.size, 0);
});

test('disposing during a streamed response suppresses later engine calls', async () => {
  const started = deferred();
  const release = deferred();
  const calls = [];
  const engine = Object.fromEntries(['httpStart', 'httpChunk', 'httpEnd', 'httpFailed']
    .map((name) => [name, () => { calls.push(name); return ''; }]));
  const host = new EngineHost(engine, { now: () => 100 });
  async function* body() {
    yield new Uint8Array([1]);
    started.resolve();
    await release.promise;
    yield new Uint8Array([2]);
  }
  const receiving = host.receive(1, { status: 200, body: body() });
  await started.promise;
  host.dispose();
  release.resolve();
  await receiving;
  assert.deepEqual(calls, ['httpStart', 'httpChunk']);
});

test('late timers and HTTP rejection do not call a disposed engine', async () => {
  const response = deferred();
  const timers = [];
  const calls = [];
  const engine = Object.fromEntries(['timer', 'wsClose', 'httpFailed']
    .map((name) => [name, () => { calls.push(name); return ''; }]));
  const host = new EngineHost(engine, {
    now: () => 100,
    request: () => response.promise,
    setTimeout: (callback) => { timers.push(callback); return timers.length; },
    clearTimeout() {},
  });
  host.execute({ op: 'setTimer', timer: 1, at: 100 });
  host.execute({ op: 'wsConnect', socket: 1, url: 'wss://example.test', headers: [['ACCESS-SIGN', 'signature']] });
  host.execute({ op: 'http', request: 1, method: 'GET', url: 'https://example.test', headers: [] });
  host.dispose();
  for (const timer of timers) timer();
  // Reject through a separate deferred promise to observe the request's handler.
  response.resolve(Promise.reject(new Error('request failed')));
  await new Promise(setImmediate);
  assert.deepEqual(calls, []);
});

for (const continuation of ['chunk', 'end', 'error']) {
  test(`a body awaiting ${continuation} cannot call a freed engine after disposal`, async () => {
    let resume;
    const waiting = new Promise((resolve) => { resume = resolve; });
    let delivered;
    const firstChunk = new Promise((resolve) => { delivered = resolve; });
    let freed = false;
    let closed = false;
    const calls = [];
    const record = (name) => {
      assert.equal(freed, false, `${name} called a freed engine`);
      calls.push(name);
      return '';
    };
    const engine = {
      httpStart: () => record('start'),
      httpChunk: () => { delivered(); return record('chunk'); },
      httpEnd: () => record('end'),
      httpFailed: () => record('error'),
    };
    const host = new EngineHost(engine, { now: () => 100 });
    const body = async function* () {
      try {
        yield Uint8Array.of(1);
        await waiting;
        if (continuation === 'chunk') yield Uint8Array.of(2);
        if (continuation === 'error') throw new Error('late body failure');
      } finally { closed = true; }
    };
    const receiving = host.receive(1, { status: 200, body: body() });
    await firstChunk;
    host.dispose();
    freed = true;
    resume();
    await receiving;
    assert.deepEqual(calls, ['start', 'chunk']);
    assert.equal(closed, true);
  });
}

test('late read and write request failures cannot reach a disposed engine or retry', async () => {
  for (const method of ['GET', 'POST']) {
    let reject;
    let requests = 0;
    let failures = 0;
    const host = new EngineHost({ httpFailed: () => { failures++; return ''; } }, {
      now: () => 100,
      request: () => { requests++; return new Promise((_, fail) => { reject = fail; }); },
    });
    host.execute({ op: 'http', request: 1, method, url: 'https://example.test', headers: [] });
    host.dispose();
    reject(new Error('late request failure'));
    await new Promise(setImmediate);
    assert.equal(failures, 0);
    assert.equal(requests, 1);
  }
});

test('already queued timers cannot enter the engine after disposal', () => {
  const callbacks = [];
  let calls = 0;
  const host = new EngineHost({
    timer: () => { calls++; return ''; },
    wsClose: () => { calls++; return ''; },
    start: () => { calls++; return ''; },
    takeFrame: () => { calls++; },
    snapshotFrame: () => { calls++; },
  }, {
    now: () => 100,
    setTimeout: (fn) => { callbacks.push(fn); return callbacks.length; },
    clearTimeout() {},
  });
  host.execute({ op: 'setTimer', timer: 1, at: 101 });
  host.execute({ op: 'wsConnect', socket: 1, url: 'wss://example.test', headers: [['Auth', 'secret']] });
  host.dispose();
  for (const callback of callbacks) callback();
  host.start();
  host.flush();
  assert.throws(() => host.snapshot(), /disposed/);
  assert.equal(calls, 0);
});

test('an asynchronous binary message cannot enter a disposed engine', async () => {
  let receive;
  let delivered;
  const decoded = new Promise((resolve) => { delivered = resolve; });
  let calls = 0;
  const host = new EngineHost({ wsMessage: () => { calls++; return ''; } }, {
    now: () => 100,
    connect: (_, handlers) => {
      receive = socketMessages((text) => { handlers.onMessage(text); delivered(); }, assert.fail);
      return { close() {}, send() {} };
    },
  });
  host.execute({ op: 'wsConnect', socket: 1, url: 'wss://example.test' });
  receive(new Blob(['late binary message']));
  host.dispose();
  await decoded;
  assert.equal(calls, 0);
});
