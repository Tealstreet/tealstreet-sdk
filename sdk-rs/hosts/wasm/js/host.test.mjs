import assert from 'node:assert/strict';
import { test } from 'node:test';
import { createServer } from 'node:net';
import { EngineHost, nativeTransport, socketMessages } from './host.mjs';

const decode = (bytes) => {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const rows = [];
  for (let at = 0; at < bytes.length;) {
    const size = view.getUint32(at + 4, true);
    rows.push([view.getUint32(at, true), view.getFloat64(at + 8, true),
      new TextDecoder().decode(bytes.subarray(at + 16, at + 16 + size))]);
    at += 16 + size;
  }
  return rows;
};
const fixture = (options) => {
  const calls = [];
  const arrivals = new Map();
  const tasks = new Map();
  let clock = 1;
  let next = 1;
  const engine = Object.fromEntries(['start', 'command', 'wsOpen', 'wsClose', 'httpStart', 'httpChunk', 'httpEnd', 'httpFailed', 'timer'].map((name) =>
    [name, (...args) => { calls.push([name, ...args]); return ''; }]));
  engine.wsMessages = (data) => { calls.push(['batch', decode(data)]); return ''; };
  engine.takeFrame = () => { calls.push(['frame']); };
  engine.snapshotFrame = () => { calls.push(['snapshot']); return new Uint8Array(); };
  const transport = {
    now: () => clock++,
    setTimeout: (fn) => { const id = next++; tasks.set(id, fn); return id; },
    clearTimeout: (id) => tasks.delete(id),
    connect: (url, handlers) => { arrivals.set(url, handlers); return { send() {}, close() {} }; },
  };
  const host = new EngineHost(engine, transport, {}, options);
  for (const socket of [1, 2]) host.execute({ op: 'wsConnect', socket, url: String(socket) });
  const send = (socket, text) => arrivals.get(String(socket)).onMessage(text);
  const tick = () => { for (const [id, fn] of [...tasks]) { tasks.delete(id); fn(); } };
  return { host, send, tick, calls, tasks, arrivals, engine };
};

test('one task batches mixed sockets, empty text, newlines and Unicode with arrival clocks', () => {
  const f = fixture();
  f.send(2, '雪😀\n\u001e');
  f.send(1, '');
  f.send(2, 'pong');
  assert.equal(f.calls.length, 0);
  assert.equal(f.tasks.size, 1);
  f.tick();
  assert.deepEqual(f.calls, [['batch', [[2, 1, '雪😀\n\u001e'], [1, 2, ''], [2, 3, 'pong']]]]);
  f.host.dispose();
});

test('commands, snapshots, flushes and socket lifecycle preserve queued-message order', async () => {
  const f = fixture();
  f.send(1, 'a');
  await f.host.command({ op: 'subscribeOrderBook', symbol: 'BTCUSDT' });
  f.send(1, 'b');
  f.host.snapshot();
  f.send(1, 'c');
  f.host.flush();
  f.send(1, 'd');
  f.arrivals.get('1').onClose(1001);
  f.send(2, 'e');
  f.arrivals.get('2').onOpen();
  assert.deepEqual(f.calls.map(([name]) => name), ['batch', 'command', 'batch', 'snapshot', 'batch', 'frame', 'batch', 'wsClose', 'batch', 'wsOpen']);
  assert.equal(f.tasks.size, 0);
  f.host.dispose();
});

test('HTTP and timer events cannot overtake socket arrivals', async () => {
  const f = fixture();
  f.send(1, 'before headers');
  const body = async function* () {
    f.send(2, 'before chunk');
    yield new Uint8Array([1]);
    f.send(1, 'before end');
  };
  await f.host.receive(1, { status: 200, body: body() });
  assert.deepEqual(f.calls.map(([name]) => name), ['batch', 'httpStart', 'batch', 'httpChunk', 'batch', 'httpEnd']);
  f.host.execute({ op: 'setTimer', timer: 1, at: 0 });
  f.send(1, 'before timer');
  f.tick();
  assert.deepEqual(f.calls.slice(-2).map(([name]) => name), ['batch', 'timer']);
  f.host.dispose();
});

test('bounded batches do not drop messages and oversize scratch is released', () => {
  const f = fixture();
  for (let i = 0; i < 600; i++) f.send(1, String(i));
  f.tick();
  assert.deepEqual(f.calls.map(([, rows]) => rows.length), [256, 256, 88]);
  assert.deepEqual(f.calls.flatMap(([, rows]) => rows.map((r) => r[2])), Array.from({ length: 600 }, (_, i) => String(i)));
  f.send(2, 'x'.repeat(1024 * 1024));
  assert.equal(f.calls.at(-1)[1][0][2].length, 1024 * 1024);
  assert.ok(f.host.batchBuffer.length <= 1024 * 1024);
  f.send(1, 'x'.repeat(10_000));
  f.tick();
  assert.equal(f.host.batchBuffer.length, 0, 'a moderate burst does not become retained scratch');
  f.send(1, 'small message');
  f.tick();
  assert.ok(f.host.batchBuffer.length > 0 && f.host.batchBuffer.length <= 16 * 1024);
  f.host.dispose();
});

test('capacity flush cannot reorder an arrival behind a synchronous action callback', () => {
  const f = fixture();
  const batches = [];
  f.engine.wsMessages = (data) => {
    batches.push(decode(data));
    return JSON.stringify([{ op: 'log', level: 'info', message: 'reenter' }]);
  };
  let reentered = false;
  f.host.handlers.onLog = () => {
    if (!reentered) {
      reentered = true;
      f.send(2, 'later');
    }
  };
  f.send(1, 'x'.repeat(350_000));
  f.tick();
  assert.deepEqual(batches.flatMap((rows) => rows.map((row) => [row[0], row[2].length])), [[1, 350_000], [2, 5]]);
  f.host.dispose();
});

test('oversized arrivals cross alone and all captured arrivals precede reentrant actions', () => {
  const f = fixture();
  const crossings = [];
  const events = [];
  f.engine.wsMessages = (data) => {
    const rows = decode(data);
    crossings.push({ bytes: data.length, rows });
    events.push(`batch:${crossings.length}`);
    return JSON.stringify([{ op: 'log', level: 'info', message: String(crossings.length) }]);
  };
  f.host.handlers.onLog = (_, message) => {
    events.push(`log:${message}`);
    if (message === '1') f.send(2, 'callback');
  };
  f.send(1, 'before');
  f.send(1, 'x'.repeat(70_000));
  assert.deepEqual(events, ['batch:1', 'batch:2', 'log:1', 'log:2']);
  f.send(2, 'after');
  f.tick();
  assert.deepEqual(crossings.map(({ bytes }) => bytes), [22, 70_016, 45]);
  assert.deepEqual(crossings.flatMap(({ rows }) => rows.map(([socket, now, text]) => [socket, now, text.length])),
    [[1, 1, 6], [1, 2, 70_000], [2, 3, 8], [2, 4, 5]]);
  assert.ok(f.host.batchBuffer.length <= 16 * 1024);
  f.host.dispose();
});

test('disposal during partitioned batch actions stops later resource actions', () => {
  const f = fixture();
  let batches = 0;
  let logs = 0;
  f.engine.wsMessages = () => {
    batches++;
    return JSON.stringify([{ op: 'log', level: 'info', message: 'stop' },
      { op: 'wsConnect', socket: 3, url: 'leak' }]);
  };
  f.host.handlers.onLog = () => { logs++; f.host.dispose(); };
  f.send(1, 'before');
  f.send(1, 'x'.repeat(70_000));
  assert.equal(batches, 2, 'captured input reaches the engine before actions execute');
  assert.equal(logs, 1);
  assert.equal(f.arrivals.has('leak'), false);
  assert.equal(f.tasks.size, 0);
});

test('disposal cancels queued deliveries and ignores late transport callbacks', async () => {
  const f = fixture();
  f.send(1, 'pending');
  f.host.dispose();
  f.send(2, 'late');
  f.arrivals.get('1').onClose();
  f.arrivals.get('2').onOpen();
  await f.host.receive(2, { status: 200, body: new Uint8Array([1]) });
  f.tick();
  assert.equal(f.calls.length, 0);
  assert.equal(f.tasks.size, 0);
});

test('disposal rejects pending commands and ignores their later HTTP responses', async () => {
  const f = fixture();
  const command = f.host.command({ op: 'fetchCandles', symbol: 'BTCUSDT', interval: '1m' });
  const rejected = assert.rejects(command, /engine disposed/);
  assert.equal(f.host.commands.size, 1);
  f.host.dispose();
  await rejected;
  assert.equal(f.host.commands.size, 0);
  await f.host.receive(1, { status: 200, body: new Uint8Array([1]) });
  assert.deepEqual(f.calls.map(([name]) => name), ['command']);
  await assert.rejects(f.host.command({ op: 'fetchCandles' }), /engine disposed/);
  assert.deepEqual(f.calls.map(([name]) => name), ['command']);
});

test('synchronous command validation failure leaves no hidden pending promise', async () => {
  const f = fixture();
  f.engine.command = () => { throw new Error('invalid command'); };
  await assert.rejects(Promise.resolve().then(() => f.host.command({ op: 'invalid' })), /invalid command/);
  assert.equal(f.host.commands.size, 0);
  f.host.dispose();
});

test('outbound action failures reject and remove their pending command', async () => {
  const f = fixture();
  f.engine.command = () => JSON.stringify([{ op: 'wsSend', socket: 1, text: 'request' }]);
  f.host.sockets.get(1).send = () => { throw new Error('send failed'); };
  await assert.rejects(f.host.command({ op: 'fetchCandles' }), /send failed/);
  assert.equal(f.host.commands.size, 0);
  f.host.dispose();
});

test('malformed command results reject their promise and preserve subsequent actions', async () => {
  const f = fixture();
  let logged = false;
  f.host.handlers.onLog = () => { logged = true; };
  f.engine.command = (json) => JSON.stringify([
    { op: 'done', command: JSON.parse(json).command, result: '{' },
    { op: 'log', level: 'info', message: 'after malformed result' },
  ]);
  await assert.rejects(f.host.command({ op: 'fetchCandles' }), SyntaxError);
  assert.equal(f.host.commands.size, 0);
  assert.equal(logged, true);
  f.host.dispose();
});

test('a throwing disposal callback returns the command cancellation without a hidden rejection', async () => {
  const f = fixture();
  f.engine.command = () => JSON.stringify([{ op: 'log', level: 'error', message: 'stop' }]);
  f.host.handlers.onLog = () => {
    f.host.dispose();
    throw new Error('callback failed');
  };
  await assert.rejects(f.host.command({ op: 'fetchCandles' }), /engine disposed/);
  assert.equal(f.host.commands.size, 0);
});

test('disposal during an action callback stops the remaining resource actions', () => {
  const f = fixture();
  f.host.handlers.onLog = () => f.host.dispose();
  f.host.run(JSON.stringify([
    { op: 'log', level: 'error', message: 'stop' },
    { op: 'wsConnect', socket: 7, url: 'after-dispose' },
    { op: 'setTimer', timer: 1, at: 0 },
  ]));
  assert.equal(f.arrivals.has('after-dispose'), false);
  assert.equal(f.tasks.size, 0);
  assert.equal(f.host.sockets.size, 0);
  f.host.execute({ op: 'wsConnect', socket: 8, url: 'direct-after-dispose' });
  assert.equal(f.arrivals.has('direct-after-dispose'), false);
});

test('disposal while draining a batch prevents its enclosing flush or command', async () => {
  for (const enclosing of ['flush', 'command']) {
    const f = fixture();
    f.engine.wsMessages = () => JSON.stringify([{ op: 'log', level: 'error', message: 'stop' }]);
    f.host.handlers.onLog = () => f.host.dispose();
    f.send(1, 'pending');
    if (enclosing === 'command') await assert.rejects(f.host.command({ op: 'fetchCandles' }), /engine disposed/);
    else f.host.flush();
    assert.deepEqual(f.calls, []);
    assert.equal(f.host.commands.size, 0);
    assert.throws(() => f.host.snapshot(), /engine disposed/);
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
    const host = new EngineHost(engine, { now: () => 100, clearTimeout() {} });
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
      clearTimeout() {},
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
    clearTimeout() {},
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
