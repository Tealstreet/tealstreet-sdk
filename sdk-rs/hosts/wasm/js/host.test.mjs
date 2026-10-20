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

import { once } from 'node:events';
import { WebSocketServer } from 'ws';

for (const entry of ['legacy', 'actions', 'bytes']) {
  test(`WebSocket actions preserve ordering and empty results with ${entry}`, () => {
    const calls = [];
    const effects = [];
    const engine = {
      wsClose() { return ''; },
      wsMessage(socket, text, now) {
        assert.equal(this, engine);
        calls.push(['legacy', socket, text, now]);
        return text === 'quiet' ? '' : actions;
      },
    };
    const actions = JSON.stringify([
      { op: 'wsSend', socket: 7, text: 'first' },
      { op: 'log', level: 'info', message: 'between' },
      { op: 'wsSend', socket: 7, text: 'second' },
    ]);
    if (entry !== 'legacy') engine.wsMessageActions = function (socket, text, now) {
      assert.equal(this, engine);
      calls.push(['actions', socket, text, now]);
      return text === 'quiet' ? undefined : actions;
    };
    if (entry === 'bytes') engine.wsMessageBytes = function (socket, bytes, now) {
      assert.equal(this, engine);
      assert.ok(bytes instanceof Uint8Array);
      const text = new TextDecoder('utf-8', { fatal: true }).decode(bytes);
      calls.push(['bytes', socket, text, now]);
      return text === 'quiet' ? undefined : actions;
    };
    let receive;
    let remoteClose;
    const host = new EngineHost(engine, {
      now: () => 123,
      connect: (_url, handlers) => {
        receive = handlers.onMessage;
        remoteClose = handlers.onClose;
        return { send: (text) => effects.push(text), close() {} };
      },
    }, { onLog: (_level, message) => effects.push(message) });
    host.execute({ op: 'wsConnect', socket: 7, url: 'wss://example.test' });
    receive('quiet');
    assert.deepEqual(effects, []);
    receive('payload');
    assert.deepEqual(effects, ['first', 'between', 'second']);
    assert.deepEqual(calls, [
      [entry, 7, 'quiet', 123],
      [entry, 7, 'payload', 123],
    ]);
    remoteClose(1001);
    receive('stale after remote close');
    assert.equal(calls.length, 2);
    host.dispose();
    receive('late');
    assert.equal(calls.length, 2);
  });
}

test('byte entry encodes complete Unicode messages with bounded reusable scratch and reentrant ownership', () => {
  const encoder = new TextEncoder();
  const received = [];
  let receive;
  const engine = {
    wsMessage() { assert.fail('byte entry must take precedence'); },
    wsMessageBytes(socket, bytes, now) {
      assert.equal(this, engine);
      assert.equal(socket, 7);
      assert.equal(now, 123);
      const before = Uint8Array.from(bytes);
      received.push({ bytes: before, buffer: bytes.buffer });
      if (new TextDecoder().decode(bytes) === 'outer') {
        receive('nested é🦀');
        assert.deepEqual(bytes, before, 'nested message must not overwrite borrowed outer bytes');
      }
      return undefined;
    },
  };
  const host = new EngineHost(engine, {
    now: () => 123,
    connect: (_url, handlers) => { receive = handlers.onMessage; return { close() {} }; },
  });
  host.execute({ op: 'wsConnect', socket: 7, url: 'wss://example.test' });
  // Exact 1365-unit boundary, lone surrogates, paired surrogates, and large frames.
  const messages = ['', 'ASCII', 'é漢🦀', '\ud800', '\udc00', '\ud800x\udc00', '漢'.repeat(1365), '\ud800'.repeat(1365), 'a'.repeat(1364) + '\ud800', '🦀'.repeat(683), '漢'.repeat(1366), 'small again'];
  for (const text of messages) {
    receive(text);
    assert.deepEqual(received.at(-1).bytes, encoder.encode(text));
  }
  const scratch = received[0].buffer;
  assert.equal(scratch.byteLength, 4096);
  for (let i = 1; i < 9; i++) assert.equal(received[i].buffer, scratch);
  assert.notEqual(received[9].buffer, scratch);
  assert.notEqual(received[10].buffer, scratch);
  assert.equal(received[11].buffer, scratch);
  receive('outer');
  assert.notEqual(received.at(-1).buffer, scratch);
  assert.equal(host.wsScratchBusy, false);
  // Minimal TextEncoder implementations may expose only encode().
  host.wsEncoder = { encode: (text) => encoder.encode(text) };
  receive('encode-only \ud800🦀');
  assert.deepEqual(received.at(-1).bytes, encoder.encode('encode-only \ud800🦀'));
  host.wsEncoder = { encode: (text) => encoder.encode(text), encodeInto: () => ({ read: 0, written: 0 }) };
  receive('complete fallback 漢');
  assert.deepEqual(received.at(-1).bytes, encoder.encode('complete fallback 漢'));
  host.dispose();
  const count = received.length;
  receive('late');
  assert.equal(received.length, count);
});

test('byte scratch is released when a custom synchronous engine throws', () => {
  let receive;
  let calls = 0;
  const host = new EngineHost({
    wsMessageBytes() { if (calls++ === 0) throw new Error('test engine failure'); },
  }, {
    now: () => 123,
    connect: (_url, handlers) => { receive = handlers.onMessage; return { close() {} }; },
  });
  host.execute({ op: 'wsConnect', socket: 7, url: 'wss://example.test' });
  assert.throws(() => receive('first'), /test engine failure/);
  assert.equal(host.wsScratchBusy, false);
  receive('second');
  assert.equal(calls, 2);
  host.dispose();
});

test('the ESM native raw constructor carries HOW and relay upgrade headers', { timeout: 3000 }, async () => {
  const server = new WebSocketServer({ port: 0, host: '127.0.0.1' });
  await once(server, 'listening');
  const connected = once(server, 'connection');
  const Socket = nativeTransport().WebSocket;
  const headers = { Origin: 'https://preview.tealstreet.io', 'User-Agent': 'fixture-browser' };
  const client = new Socket(`ws://127.0.0.1:${server.address().port}/tunnel?v=2&t=1`, [], { headers });
  client.on('error', () => {});
  let peer;
  try {
    const opened = once(client, 'open');
    const connection = await connected;
    peer = connection[0];
    assert.equal(connection[1].url, '/tunnel?v=2&t=1');
    assert.equal(connection[1].headers.origin, headers.Origin);
    assert.equal(connection[1].headers['user-agent'], headers['User-Agent']);
    await opened;
  } finally {
    client.terminate();
    peer?.terminate();
    await new Promise((resolve) => server.close(resolve));
  }
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

test('feed roles survive the engine action boundary for identical upstream URLs', () => {
  const roles = [];
  const host = new EngineHost({}, {
    now: () => 0,
    connect: (url, handlers) => { roles.push([url, handlers.channel]); return { close() {}, send() {} }; },
  });
  for (const [socket, channel] of ['public', 'private', 'business'].entries()) {
    host.execute({ op: 'wsConnect', socket, url: 'wss://venue.test/stream', channel });
  }
  assert.deepEqual(roles, ['public', 'private', 'business'].map((role) => ['wss://venue.test/stream', role]));
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

test('disposing an engine releases account-owned transport resources', () => {
  const order = [];
  const host = new EngineHost({}, {
    now: () => 0,
    connect: () => ({ send() {}, close: () => order.push('socket') }),
    dispose: () => order.push('transport'),
  });
  host.execute({ op: 'wsConnect', socket: 1, url: 'wss://venue.test/stream' });
  host.dispose();
  host.dispose();
  assert.deepEqual(order, ['socket', 'transport']);
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

test('disposal aborts deferred authorization before a signed write can dispatch', async () => {
  let authorize;
  const authorization = new Promise((resolve) => { authorize = resolve; });
  let signal;
  let writes = 0;
  let failures = 0;
  const host = new EngineHost({ httpFailed: () => { failures++; return ''; } }, {
    now: () => 100,
    async request(request) {
      signal = request.signal;
      await authorization;
      request.signal.throwIfAborted();
      writes++;
      return { status: 200, body: new Uint8Array() };
    },
  });
  host.execute({ op: 'http', request: 1, method: 'POST', url: 'https://example.test/order', headers: [['Signature', 'signed-body']], body: '{}' });
  assert.equal(host.requests.size, 1);
  assert.equal(signal.aborted, false);
  host.dispose();
  assert.equal(signal.aborted, true);
  assert.equal(host.requests.size, 0);
  authorize();
  await new Promise(setImmediate);
  assert.equal(writes, 0);
  assert.equal(failures, 0);
});

test('request controllers cover streaming consumption and release after completion', async () => {
  let resume;
  const pending = new Promise((resolve) => { resume = resolve; });
  let firstChunk;
  const first = new Promise((resolve) => { firstChunk = resolve; });
  let signal;
  let completed;
  const done = new Promise((resolve) => { completed = resolve; });
  const host = new EngineHost({
    httpStart: () => '',
    httpChunk: () => { firstChunk(); return ''; },
    httpEnd: () => { completed(); return ''; },
    httpFailed: assert.fail,
  }, {
    now: () => 100,
    async request(request) {
      signal = request.signal;
      return { status: 200, body: (async function* () { yield Uint8Array.of(1); await pending; yield Uint8Array.of(2); })() };
    },
  });
  host.execute({ op: 'http', request: 1, method: 'GET', url: 'https://example.test/list', headers: [] });
  await first;
  assert.equal(host.requests.size, 1, 'headers alone do not complete a streamed request');
  resume();
  await done;
  await new Promise(setImmediate);
  assert.equal(host.requests.size, 0);
  host.dispose();
  assert.equal(signal.aborted, false, 'completed requests are no longer retained by the host');
});

test('synchronous transport errors release their request controller', async () => {
  const failures = [];
  const host = new EngineHost({ httpFailed: (_, error) => { failures.push(error); return ''; } }, {
    now: () => 100,
    request() { throw new Error('transport unavailable'); },
  });
  host.execute({ op: 'http', request: 1, method: 'POST', url: 'https://example.test/order', headers: [] });
  await new Promise(setImmediate);
  assert.deepEqual(failures, ['transport unavailable']);
  assert.equal(host.requests.size, 0);
  host.dispose();
});

for (const runtime of ['native', 'worker']) {
  test(`${runtime} fetch abort reaches an in-flight streamed body and releases its reader`, async () => {
    const transport = runtime === 'native' ? nativeTransport() : (await import('./workers.mjs')).workersTransport();
    const originalFetch = globalThis.fetch;
    let signal;
    let stream;
    let delivered;
    const first = new Promise((resolve) => { delivered = resolve; });
    let failures = 0;
    globalThis.fetch = async (_url, options) => {
      signal = options.signal;
      stream = new ReadableStream({
        start(controller) {
          controller.enqueue(Uint8Array.of(1));
          signal.addEventListener('abort', () => controller.error(signal.reason), { once: true });
        },
      });
      return { status: 200, headers: new Headers(), body: stream };
    };
    const host = new EngineHost({
      httpStart: () => '', httpChunk: () => { delivered(); return ''; },
      httpEnd: assert.fail, httpFailed: () => { failures++; return ''; },
    }, transport);
    try {
      host.execute({ op: 'http', request: 1, method: 'GET', url: 'https://example.test/list', headers: [] });
      await first;
      assert.equal(stream.locked, true);
      assert.equal(host.requests.size, 1);
      host.dispose();
      assert.equal(signal.aborted, true);
      await new Promise(setImmediate);
      assert.equal(stream.locked, false);
      assert.equal(host.requests.size, 0);
      assert.equal(failures, 0);
    } finally {
      host.dispose();
      globalThis.fetch = originalFetch;
    }
  });
}

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

test('credential deactivation is an explicit host event', () => {
  let deactivated = 0;
  const host = new EngineHost({ start() { return '[{"op":"deactivate"}]'; } }, { now: () => 1 }, {
    onDeactivate() { deactivated++; },
  });
  host.start();
  assert.equal(deactivated, 1);
});

test('socket feed identity and protocol recovery reach the account transport', () => {
  let received;
  const host = new EngineHost({}, {
    connect: (url, handlers) => {
      received = { url, channel: handlers.channel, context: handlers.context };
      return { send() {}, close() {} };
    },
  });
  const context = { feed: 'book-2', recover: true };
  host.execute({ op: 'wsConnect', socket: 1, url: 'wss://venue.test/shared', channel: 'public', context });
  assert.deepEqual(received, { url: 'wss://venue.test/shared', channel: 'public', context });
  host.dispose();
});

test('public HTTP routing metadata survives actions without changing ordinary requests', async () => {
  const captured=[];
  const host=new EngineHost({httpFailed:()=>''},{now:()=>0,request:(request)=>{captured.push(request);return Promise.reject(new Error('fixture stop'));}});
  for(const route of [undefined,'direct','publicCors']) host.execute({op:'http',request:1,method:'GET',url:'https://venue.test/time',headers:[],...(route?{route,timeoutMs:5000}:{})});
  assert.equal(Object.hasOwn(captured[0],'route'),false);assert.equal(Object.hasOwn(captured[0],'timeoutMs'),false);assert.deepEqual(captured.slice(1).map((request)=>request.timeoutMs),[5000,5000]);
  assert.deepEqual(captured.slice(1).map((request)=>request.route),['direct','publicCors']);
  assert.ok(captured.every((request)=>Object.keys(request.headers).length===0));
  await Promise.resolve();
});

test('the raw host rejects unconfigured CORS routing before touching the network', async () => {
  await assert.rejects(nativeTransport().request({method:'GET',route:'publicCors',url:'https://venue.test/time',headers:{}}),/not configured/);
});

test('raw binary frames, following text and closure retain receive order', async () => {
  const socket = new MessageSocket();
  const events = [];
  const closed = deferred();
  bindSocketMessages(socket, {
    binary: true,
    onBinary: (data) => events.push([...data]),
    onMessage: (text) => events.push(text),
    onClose: (code) => { events.push(code); closed.resolve(); },
  });
  socket.dispatch('message', { data: new Blob([Uint8Array.of(0x1f, 0x8b, 0xff)]) });
  socket.dispatch('message', { data: 'control' });
  socket.dispatch('close', { code: 1001 });
  await closed.promise;
  assert.deepEqual(events, [[0x1f, 0x8b, 0xff], 'control', 1001]);
});

test('raw binary frames remain bounded and reject queued messages after failure', async () => {
  const socket = new MessageSocket();
  const closed = deferred();
  const messages = [];
  bindSocketMessages(socket, {
    binary: true,
    onBinary: (data) => messages.push(data),
    onMessage: (text) => messages.push(text),
    onClose: () => closed.resolve(),
  });
  socket.dispatch('message', { data: new Uint8Array(16 * 1024 * 1024 + 1) });
  socket.dispatch('message', { data: 'must not follow oversize' });
  await closed.promise;
  assert.deepEqual(messages, []);
  assert.equal(socket.closes, 1);
});

test('disposing while raw binary delivery waits suppresses data, control and close callbacks', async () => {
  const socket = new MessageSocket();
  const started = deferred();
  const release = deferred();
  const events = [];
  class DelayedBlob extends Blob {
    async arrayBuffer() { started.resolve(); await release.promise; return super.arrayBuffer(); }
  }
  const dispose = bindSocketMessages(socket, {
    binary: true,
    onBinary: (data) => events.push(data),
    onMessage: (text) => events.push(text),
    onClose: (code) => events.push(code),
  });
  socket.dispatch('message', { data: new DelayedBlob([Uint8Array.of(0xff)]) });
  socket.dispatch('message', { data: 'late control' });
  socket.dispatch('close', { code: 1000 });
  await started.promise;
  dispose();
  release.resolve();
  await new Promise(setImmediate);
  assert.deepEqual(events, []);
});

test('a transport that closes during connect reaches the engine once and leaves no socket', () => {
  const events = [];
  let released = 0;
  const host = new EngineHost({ wsClose: (...args) => { events.push(args); return ''; } }, {
    now: () => 7,
    connect: (_url, handlers) => {
      handlers.onClose(1006);
      return { close: () => { released++; handlers.onClose(1006); } };
    },
  });
  host.execute({ op: 'wsConnect', socket: 4, url: 'wss://example.test' });
  assert.deepEqual(events, [[4, 1006, 7]]);
  assert.equal(host.sockets.size, 0);
  assert.equal(released, 1);
});
