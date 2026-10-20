import assert from 'node:assert/strict';
import { test } from 'node:test';
import { createServer } from 'node:net';
import { once } from 'node:events';
import { WebSocketServer } from 'ws';
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
const decodeEvents = (bytes) => {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const rows = [];
  for (let at = 0; at < bytes.length;) {
    const size = view.getUint32(at + 4, true);
    const payload = bytes.subarray(at + 20, at + 20 + size);
    const kind = view.getUint32(at + 16, true);
    assert.ok(kind === 0 || kind === 1);
    rows.push([view.getUint32(at, true), view.getFloat64(at + 8, true),
      kind ? Array.from(payload) : new TextDecoder().decode(payload)]);
    at += 20 + size;
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
  engine.wsMessage = (socket, text, now) => { calls.push(['single', [[socket, now, text]]]); return ''; };
  engine.wsBinary = (socket, data, now) => { calls.push(['binary', [[socket, now, Array.from(data)]]]); return ''; };
  engine.wsMessages = (data) => { calls.push(['batch', decode(data)]); return ''; };
  engine.wsEvents = (data) => { calls.push(['events', decodeEvents(data)]); return ''; };
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

test('one task batches text and owned binary snapshots in arrival order', () => {
  const f = fixture();
  f.host.execute({ op: 'wsConnect', socket: 7, url: 'binary', binary: true });
  const binary = f.arrivals.get('binary');
  assert.equal(binary.binary, true);
  const storage = new Uint8Array([9, 0, 255, 9]);
  f.arrivals.get('1').onMessage('first', 10);
  binary.onBinary(storage.subarray(1, 3), 11.5);
  storage.fill(42);
  f.arrivals.get('2').onMessage('雪😀', 12);
  assert.deepEqual(f.calls, []);
  assert.equal(f.tasks.size, 1);
  f.tick();
  assert.deepEqual(f.calls, [['events', [[1, 10, 'first'], [7, 11.5, [0, 255]], [2, 12, '雪😀']]]]);
  f.host.dispose();
});

test('binary singleton stays deferred and crosses directly before later event barriers', async () => {
  const f = fixture();
  const binary = f.arrivals.get('1');
  const payload = new Uint8Array([255, 0]);
  binary.onBinary(payload, 17);
  payload.fill(1);
  assert.equal(f.calls.length, 0);
  await f.host.command({ op: 'subscribeOrderBook', symbol: 'BTCUSDT' });
  assert.deepEqual(f.calls.slice(0, 2), [['binary', [[1, 17, [255, 0]]]], ['command', '{"op":"subscribeOrderBook","symbol":"BTCUSDT"}', 1]]);
  binary.onBinary(new Uint8Array(), 18);
  f.host.receive(2, { status: 200, body: new Uint8Array() });
  assert.deepEqual(f.calls.slice(-3).map(([name]) => name), ['binary', 'httpStart', 'httpEnd']);
  binary.onBinary(new Uint8Array([7]), 19);
  binary.onClose(1000, 20);
  assert.deepEqual(f.calls.slice(-2), [['binary', [[1, 19, [7]]]], ['wsClose', 1, 1000, 20]]);
  f.host.dispose();
});

test('mixed partitions deliver all captured messages before reentrant transport actions', () => {
  const f = fixture();
  const crossings = [];
  const events = [];
  f.engine.wsEvents = (data) => {
    crossings.push({ bytes: data.length, rows: decodeEvents(data) });
    events.push(`batch:${crossings.length}`);
    return JSON.stringify([{ op: 'log', level: 'info', message: String(crossings.length) }]);
  };
  f.host.handlers.onLog = (_, message) => {
    events.push(`log:${message}`);
    if (message === '1') f.arrivals.get('2').onBinary(new Uint8Array([3]), 30);
  };
  f.arrivals.get('1').onMessage('before', 10);
  const payload = new Uint8Array(70_000).fill(255);
  f.arrivals.get('1').onBinary(payload, 20);
  assert.deepEqual(events, ['batch:1', 'batch:2', 'log:1', 'log:2']);
  assert.deepEqual(crossings.map(({ bytes }) => bytes), [26, 70_020]);
  f.arrivals.get('2').onMessage('after', 40);
  f.tick();
  assert.deepEqual(crossings[0].rows, [[1, 10, 'before']]);
  assert.equal(crossings[1].rows[0][2].length, payload.length);
  assert.deepEqual(crossings[2].rows, [[2, 30, [3]], [2, 40, 'after']]);
  assert.ok(f.host.batchBuffer.length <= 16 * 1024);
  f.host.dispose();
});

test('mixed bursts share the count bound and disposal releases queued binary arrivals', () => {
  const f = fixture();
  for (let index = 0; index < 257; index++) {
    if (index % 2) f.arrivals.get('1').onBinary(new Uint8Array([index % 256]), index);
    else f.arrivals.get('2').onMessage(String(index), index);
  }
  assert.equal(f.calls.length, 1);
  assert.equal(f.calls[0][0], 'events');
  assert.equal(f.calls[0][1].length, 256);
  f.tick();
  assert.deepEqual(f.calls[1], ['single', [[2, 256, '256']]]);
  f.arrivals.get('1').onBinary(new Uint8Array([9]), 300);
  f.host.dispose();
  f.tick();
  assert.equal(f.calls.length, 2);
  assert.equal(f.host.messages.length, 0);
  assert.equal(f.host.messageBytes, 0);
});

test('disposal during mixed actions stops later resources and enclosing flush', () => {
  const f = fixture();
  f.engine.wsEvents = () => JSON.stringify([{ op: 'log', level: 'info', message: 'stop' },
    { op: 'wsConnect', socket: 7, url: 'leak' }]);
  f.host.handlers.onLog = () => f.host.dispose();
  f.send(1, 'text');
  f.arrivals.get('2').onBinary(new Uint8Array([255]), 17);
  f.host.flush();
  assert.equal(f.arrivals.has('leak'), false);
  assert.deepEqual(f.calls, []);
  assert.equal(f.tasks.size, 0);
});

test('older binary engines retain ordered deferred actions without a mixed ABI', () => {
  const f = fixture();
  f.engine.wsEvents = undefined;
  const seen = [];
  f.engine.wsMessageActions = (_socket, text) => { seen.push(text); return JSON.stringify([{ op: 'log', level: 'info', message: text }]); };
  f.engine.wsBinary = (_socket, bytes) => { seen.push([...bytes]); return ''; };
  f.host.handlers.onLog = () => assert.deepEqual(seen, ['first', [255], 'last']);
  f.send(1, 'first');
  f.arrivals.get('2').onBinary(new Uint8Array([255]), 2);
  f.send(1, 'last');
  f.tick();
  f.host.dispose();
});

test('scheduled text singletons prefer bytes and retain optional string fallbacks with raw clocks', () => {
  const encoder = new TextEncoder();
  for (const entry of ['bytes', 'actions', 'legacy']) {
    const f = fixture();
    const calls = [];
    f.engine.wsMessages = () => assert.fail('a singleton must not use the batch ABI');
    f.engine.wsMessage = function (socket, text, now) {
      assert.equal(entry, 'legacy');
      assert.equal(this, f.engine);
      calls.push({ socket, text, now });
      return '';
    };
    if (entry !== 'legacy') f.engine.wsMessageActions = function (socket, text, now) {
      assert.equal(entry, 'actions');
      assert.equal(this, f.engine);
      calls.push({ socket, text, now });
      return undefined;
    };
    if (entry === 'bytes') f.engine.wsMessageBytes = function (socket, bytes, now) {
      assert.equal(this, f.engine);
      calls.push({ socket, bytes: Uint8Array.from(bytes), now, buffer: bytes.buffer });
      return undefined;
    };
    const messages = [['ASCII', 17.25], ['雪😀\n', -7], ['\ud800', -0], ['\udc00', 19], ['', 20]];
    let scratch;
    for (const [text, now] of messages) {
      const before = calls.length;
      f.arrivals.get('2').onMessage(text, now);
      assert.equal(calls.length, before, 'one arrival still waits for its scheduled task');
      assert.equal(f.tasks.size, 1);
      f.tick();
      const row = calls.at(-1);
      assert.equal(row.socket, 2);
      assert.ok(Object.is(row.now, now), 'raw finite time, including negative zero, is preserved');
      if (entry === 'bytes') {
        assert.deepEqual(row.bytes, encoder.encode(text));
        scratch ??= row.buffer;
        assert.equal(row.buffer, scratch, 'small singleton encodings reuse the same scratch');
      } else assert.equal(row.text, text);
      assert.equal(f.host.batchBuffer.byteLength, 0);
      assert.equal(f.host.wsScratchBusy, false);
    }
    assert.equal(calls.length, messages.length);
    f.host.dispose();
    assert.equal(f.host.wsScratch, undefined);
  }
});

test('singleton arrivals stay deferred and preserve their text and raw finite clocks', () => {
  const f = fixture();
  f.engine.wsMessages = () => assert.fail('a singleton must not be encoded');
  const scratch = f.host.batchBuffer;
  for (const [text, now] of [['', 17.25], ['雪😀\n\u001e', -7], ['\ud800', -0]]) {
    f.arrivals.get('2').onMessage(text, now);
    assert.equal(f.calls.length, 0);
    assert.equal(f.tasks.size, 1);
    f.tick();
    assert.deepEqual(f.calls.pop(), ['single', [[2, now, text]]]);
    assert.equal(f.host.messages.length, 0);
    assert.equal(f.host.messageBytes, 0);
    assert.equal(f.host.batchTimer, undefined);
    assert.equal(f.host.batchBuffer, scratch);
  }
  f.host.dispose();
});

for (const entry of ['wsMessage', 'wsMessageBytes']) {
  test(`singleton actions enqueue reentrant arrivals in a fresh task (${entry})`, () => {
    const f = fixture();
    f.engine.wsMessages = () => assert.fail('each captured queue has one arrival');
    f.engine[entry] = (socket, data, now) => {
      const text = typeof data === 'string' ? data : new TextDecoder().decode(data);
      f.calls.push(['single', [[socket, now, text]]]);
      return text === 'first' ? JSON.stringify([{ op: 'log', level: 'info', message: 'reenter' }]) : '';
    };
    if (entry === 'wsMessageBytes') f.engine.wsMessage = () => assert.fail('byte entry must take precedence');
    f.host.handlers.onLog = () => {
      assert.equal(f.host.messages.length, 0);
      assert.equal(f.host.batchTimer, undefined);
      f.arrivals.get('2').onMessage('second', 18);
    };
    f.arrivals.get('1').onMessage('first', 17);
    f.tick();
    assert.deepEqual(f.calls, [['single', [[1, 17, 'first']]]]);
    assert.equal(f.tasks.size, 1);
    f.tick();
    assert.deepEqual(f.calls, [['single', [[1, 17, 'first']]], ['single', [[2, 18, 'second']]]]);
    f.host.dispose();
  });

}

for (const entry of ['wsMessage', 'wsMessageBytes']) {
  test(`singleton action disposal stops later resources and its flush (${entry})`, () => {
    const f = fixture();
    f.engine[entry] = () => {
      f.calls.push(['single']);
      return JSON.stringify([{ op: 'log', level: 'info', message: 'stop' },
        { op: 'wsConnect', socket: 3, url: 'leak' }, { op: 'setTimer', timer: 9, at: 0 }]);
    };
    if (entry === 'wsMessageBytes') f.engine.wsMessage = () => assert.fail('byte entry must take precedence');
    f.host.handlers.onLog = () => f.host.dispose();
    f.send(1, 'pending');
    f.host.flush();
    f.tick();
    assert.deepEqual(f.calls, [['single']]);
    assert.equal(f.arrivals.has('leak'), false);
    assert.equal(f.tasks.size, 0);
  });

}

test('an oversized singleton crosses directly without allocating batch scratch', () => {
  const f = fixture();
  f.engine.wsMessages = () => assert.fail('oversized singleton must not be encoded');
  const scratch = f.host.batchBuffer;
  const text = '雪😀'.repeat(100_000);
  f.arrivals.get('2').onMessage(text, 123.5);
  assert.deepEqual(f.calls, [['single', [[2, 123.5, text]]]]);
  assert.equal(f.host.batchBuffer, scratch);
  assert.equal(f.tasks.size, 0);
  f.host.dispose();
});

test('singleton undefined or empty actions are accepted and engine throws retain identity', () => {
  const f = fixture();
  for (const result of [undefined, '']) {
    f.engine.wsMessage = () => result;
    f.send(1, 'no actions');
    assert.doesNotThrow(f.tick);
  }
  const error = new Error('engine failed');
  f.engine.wsMessage = () => { throw error; };
  f.send(1, 'throws');
  assert.throws(f.tick, (actual) => actual === error);
  assert.equal(f.host.messages.length, 0);
  assert.equal(f.host.batchTimer, undefined);
  f.engine.wsMessage = (socket, text, now) => { f.calls.push(['single', [[socket, now, text]]]); return ''; };
  f.send(2, 'after failure');
  f.tick();
  assert.deepEqual(f.calls, [['single', [[2, 4, 'after failure']]]]);
  f.host.dispose();
});

test('nonfinite clocks reject before crossing and coercible clocks retain batch encoding', () => {
  const f = fixture();
  f.engine.wsMessage = () => assert.fail('invalid or coercible clocks must retain batch semantics');
  for (const now of [NaN, Infinity, -Infinity]) {
    f.arrivals.get('1').onMessage('payload', now);
    assert.throws(f.tick, /invalid websocket batch arrival time/);
    assert.deepEqual(f.calls, []);
  }
  for (const now of ['7', new Number(8)]) {
    f.arrivals.get('1').onMessage('payload', now);
    f.tick();
    const row = f.calls.pop();
    assert.equal(row[0], 'batch');
    assert.deepEqual(row[1], [[1, Number(now), 'payload']]);
  }
  const error = new Error('batch validation failed');
  f.engine.wsMessages = () => { throw error; };
  f.arrivals.get('1').onMessage('payload', 1);
  f.arrivals.get('2').onMessage('payload', 2);
  assert.throws(f.tick, (actual) => actual === error);
  f.host.dispose();
});

test('nonstring singleton payloads retain the encoder rejection path', () => {
  const f = fixture();
  f.engine.wsMessage = () => assert.fail('nonstring payload must not reach the single-message API');
  f.arrivals.get('1').onMessage(new String('payload'), 7);
  assert.throws(f.tick, TypeError);
  f.host.dispose();
});

test('every partition timestamp is validated before any batch reaches the engine', () => {
  const f = fixture();
  f.arrivals.get('1').onMessage('a'.repeat(12_000), 1);
  assert.throws(() => f.arrivals.get('2').onMessage('b'.repeat(10_000), NaN), /invalid websocket batch arrival time/);
  assert.deepEqual(f.calls, [], 'an invalid later partition must not partially apply the prefix');
  f.host.dispose();
});

test('raw ingress drains a finite prefix before reentrant arrivals', async () => {
  const f = fixture({ batchMessages: false });
  f.engine.wsMessage = (socket, text, now) => {
    f.calls.push(['single', [[socket, now, text]]]);
    return text === 'first' ? '[{"op":"log","level":"info","message":"reenter"}]' : '';
  };
  f.host.handlers.onLog = () => f.send(2, 'reentrant');
  const settle = f.arrivals.get('1').onIngress();
  const command = f.host.command({ op: 'subscribeOrderBook', symbol: 'before reentrant' });
  settle(() => f.arrivals.get('1').onMessage('first', 17));
  await command;
  assert.deepEqual(f.calls.map(([name]) => name), ['single', 'command']);
  f.tick();
  assert.deepEqual(f.calls.map(([name]) => name), ['single', 'command', 'single']);
  assert.equal(f.calls[2][1][0][2], 'reentrant');
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
  assert.deepEqual(f.calls.map(([name]) => name), ['single', 'command', 'single', 'snapshot', 'single', 'frame', 'single', 'wsClose', 'single', 'wsOpen']);
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
  assert.deepEqual(f.calls.map(([name]) => name), ['single', 'httpStart', 'single', 'httpChunk', 'single', 'httpEnd']);
  f.host.execute({ op: 'setTimer', timer: 1, at: 0 });
  f.send(1, 'before timer');
  f.tick();
  assert.deepEqual(f.calls.slice(-2).map(([name]) => name), ['single', 'timer']);
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
  f.send(2, 'y'.repeat(10_000));
  f.tick();
  assert.equal(f.host.batchBuffer.length, 0, 'a moderate burst does not become retained scratch');
  f.send(1, 'small message');
  f.send(2, 'another small message');
  f.tick();
  assert.ok(f.host.batchBuffer.length > 0 && f.host.batchBuffer.length <= 16 * 1024);
  f.host.dispose();
});

test('capacity flush cannot reorder an arrival behind a synchronous action callback', () => {
  const f = fixture();
  const batches = [];
  const receive = (rows) => {
    batches.push(rows);
    return JSON.stringify([{ op: 'log', level: 'info', message: 'reenter' }]);
  };
  f.engine.wsMessages = (data) => receive(decode(data));
  f.engine.wsMessage = (socket, text, now) => receive([[socket, now, text]]);
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
    f.send(2, 'also pending');
    if (enclosing === 'command') await assert.rejects(f.host.command({ op: 'fetchCandles' }), /engine disposed/);
    else f.host.flush();
    assert.deepEqual(f.calls, []);
    assert.equal(f.host.commands.size, 0);
    assert.throws(() => f.host.snapshot(), /engine disposed/);
  }
});

test('account identity is read from the live engine and unavailable after disposal', () => {
  let account;
  const host = new EngineHost({ accountId: () => account }, {});
  assert.equal(host.accountId(), undefined);
  account = 'selected-portfolio';
  assert.equal(host.accountId(), 'selected-portfolio');
  host.dispose();
  assert.throws(() => host.accountId(), /disposed/);
  assert.equal(new EngineHost({}, {}).accountId(), undefined);
});

test('account identity drains queued discovery before reading the live engine', () => {
  const f = fixture();
  let account;
  f.engine.wsMessage = () => { account = 'queued-portfolio'; return ''; };
  f.engine.accountId = () => account;
  f.send(1, 'discovery');
  assert.equal(account, undefined);
  assert.equal(f.host.accountId(), 'queued-portfolio');
  assert.equal(f.tasks.size, 0);
  f.host.dispose();
});

test('account identity stops after queued actions dispose the host', () => {
  const f = fixture();
  let reads = 0;
  f.engine.accountId = () => { reads++; return 'stale'; };
  f.engine.wsMessage = () => '[{"op":"log","level":"error","message":"stop"}]';
  f.host.handlers.onLog = () => f.host.dispose();
  f.send(1, 'invalidate');
  assert.throws(() => f.host.accountId(), /engine disposed/);
  assert.equal(reads, 0);
});

test('tick interval changes drain prior arrivals and resolve without a reply ID', async () => {
  const f = fixture();
  f.send(1, 'before interval');
  await f.host.command({ op: 'setTickInterval', interval: 250 });
  assert.deepEqual(f.calls, [
    ['single', [[1, 1, 'before interval']]],
    ['command', '{"op":"setTickInterval","interval":250}', 2],
  ]);
  assert.equal(f.host.commands.size, 0);
  assert.equal(f.host.nextCommand, 1);
  f.host.dispose();
});

for (const entry of ['legacy', 'actions', 'bytes']) {
  test(`WebSocket actions preserve ordering and empty results with ${entry}`, () => {
    const calls = [];
    const effects = [];
    const engine = {
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
    const host = new EngineHost(engine, {
      now: () => 123,
      connect: (_url, handlers) => {
        receive = handlers.onMessage;
        return { send: (text) => effects.push(text), close() {} };
      },
    }, { onLog: (_level, message) => effects.push(message) }, { batchMessages: false });
    host.execute({ op: 'wsConnect', socket: 7, url: 'wss://example.test' });
    receive('quiet');
    assert.deepEqual(effects, []);
    receive('payload');
    assert.deepEqual(effects, ['first', 'between', 'second']);
    assert.deepEqual(calls, [
      [entry, 7, 'quiet', 123],
      [entry, 7, 'payload', 123],
    ]);
    host.dispose();
    receive('late');
    assert.equal(calls.length, 2);
  });
}

for (const batchMessages of [false, true]) {
  test(`byte entry encodes complete Unicode messages with bounded scratch and reentry (scheduled=${batchMessages})`, () => {
    const encoder = new TextEncoder();
    const received = [];
    let arrival;
    const receive = (text) => {
      arrival(text);
      if (batchMessages) host.drainMessages();
    };
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
      setTimeout: () => 0,
      clearTimeout() {},
      connect: (_url, handlers) => { arrival = handlers.onMessage; return { close() {} }; },
    }, {}, { batchMessages });
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

  test(`byte scratch is released when a synchronous engine throws (scheduled=${batchMessages})`, () => {
    let arrival;
    const receive = (text) => {
      arrival(text);
      if (batchMessages) host.drainMessages();
    };
    let calls = 0;
    const failure = new Error('test engine failure');
    const host = new EngineHost({
      wsMessageBytes() { if (calls++ === 0) throw failure; },
    }, {
      now: () => 123,
      setTimeout: () => 0,
      clearTimeout() {},
      connect: (_url, handlers) => { arrival = handlers.onMessage; return { close() {} }; },
    }, {}, { batchMessages });
    host.execute({ op: 'wsConnect', socket: 7, url: 'wss://example.test' });
    assert.throws(() => receive('first'), (error) => error === failure);
    assert.equal(host.wsScratchBusy, false);
    receive('second');
    assert.equal(calls, 2);
    host.dispose();
  });
}

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
    clearTimeout() {},
    connect: (_, handlers) => { closeHandler = handlers.onClose; return { close: () => closeHandler(1000) }; },
  });
  host.execute({ op: 'wsConnect', socket: 1, url: 'wss://example.test' });
  host.dispose();
  closeHandler(1006);
  assert.equal(closedEvents, 0);
  assert.equal(host.sockets.size, 0);
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

test('request rejection cannot overtake queued websocket arrivals', async () => {
  const f = fixture();
  let reject;
  f.host.transport.request = () => new Promise((_, fail) => { reject = fail; });
  f.host.execute({ op: 'http', request: 1, method: 'GET', url: 'https://example.test/list', headers: [] });
  f.send(1, 'before failure');
  reject(new Error('transport failed'));
  await new Promise(setImmediate);
  assert.deepEqual(f.calls.map(([name]) => name), ['single', 'httpFailed']);
  assert.equal(f.host.requests.size, 0);
  f.host.dispose();
});

test('deactivation while draining an HTTP rejection aborts it and stops later resource actions', async () => {
  const f = fixture();
  let reject;
  let signal;
  let requests = 0;
  f.host.transport.request = (request) => {
    requests++;
    signal = request.signal;
    return new Promise((_, fail) => { reject = fail; });
  };
  f.host.execute({ op: 'http', request: 1, method: 'GET', url: 'https://example.test/list', headers: [] });
  f.engine.wsMessages = (data) => {
    f.calls.push(['batch', decode(data)]);
    return JSON.stringify([
      { op: 'deactivate' },
      { op: 'wsConnect', socket: 9, url: 'must not open' },
      { op: 'http', request: 2, method: 'POST', url: 'must not send', headers: [] },
    ]);
  };
  f.host.handlers.onDeactivate = () => f.host.dispose();
  f.send(1, 'credentials rejected');
  f.send(2, 'also before failure');
  reject(new Error('later failure'));
  await new Promise(setImmediate);
  assert.deepEqual(f.calls.map(([name]) => name), ['batch']);
  assert.equal(signal.aborted, true);
  assert.equal(f.host.requests.size, 0);
  assert.equal(requests, 1);
  assert.equal(f.arrivals.has('must not open'), false);
  assert.equal(f.host.disposed, true);
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

test('delegated signing crosses back as an event before HTTP and forwards the opaque answer', async () => {
  const calls = [];
  const engine = {
    payloadSigned(id, signature, headers, now) {
      calls.push(['signed', id, signature, headers, now]);
      return JSON.stringify([{ op: 'http', request: id, method: 'POST', url: 'https://venue.test/order', headers: [], body: signature }]);
    },
    httpStart: () => '', httpChunk: () => '', httpEnd: () => '',
  };
  const payload = { exchangeName: 'paradex', key: 'public', payload: '0x123', scheme: 'stark', encoding: 'felt-pair' };
  const host = new EngineHost(engine, {
    now: () => 100, setTimeout, clearTimeout,
    signPayload: async (request, { signal }) => {
      calls.push(['sign', request, signal.aborted]);
      return { signature: 'opaque answer', headers: { 'X-Proof': 'proof' } };
    },
    request: async (request) => { calls.push(['http', request]); return { status: 200, body: new Uint8Array() }; },
  });
  host.execute({ op: 'signPayload', request: 7, payload, timeoutMs: 1000 });
  assert.equal(calls.length, 0);
  await new Promise((resolve) => setImmediate(resolve));
  assert.deepEqual(calls[0], ['sign', payload, false]);
  assert.deepEqual(calls[1], ['signed', 7, 'opaque answer', 'X-Proof:proof', 100]);
  assert.equal(calls[2][0], 'http');
  assert.equal(calls[2][1].body, 'opaque answer');
  assert.equal(host.signings.size, 0);
  host.dispose();
});

test('delegated signing timeouts abort the signer and discard a late signature', async () => {
  let timeout;
  let resolveSign;
  let signal;
  const events = [];
  const host = new EngineHost({
    payloadSignFailed: (...args) => (events.push(args), ''),
    payloadSigned() { throw new Error('late signature must not enter engine'); },
  }, {
    now: () => 200,
    setTimeout: (fn) => (timeout = fn, 1), clearTimeout() {},
    signPayload: (_, context) => (signal = context.signal, new Promise((resolve) => { resolveSign = resolve; })),
  });
  host.execute({ op: 'signPayload', request: 8, payload: {}, timeoutMs: 20_000 });
  await Promise.resolve();
  timeout();
  assert.equal(signal.aborted, true);
  assert.deepEqual(events, [[8, 'Payload signing timed out', 200]]);
  assert.equal(host.signings.size, 0);
  resolveSign({ signature: 'late' });
  await new Promise((resolve) => setImmediate(resolve));
  assert.equal(events.length, 1);
  host.dispose();
});

test('host disposal cancels pending signatures and prevents subsequent engine calls', async () => {
  let resolveSign;
  let signal;
  let timers = 0;
  const host = new EngineHost({
    payloadSignFailed() { throw new Error('disposed'); }, payloadSigned() { throw new Error('disposed'); },
  }, {
    now: () => 0,
    setTimeout: () => (++timers), clearTimeout: () => --timers,
    signPayload: (_, context) => (signal = context.signal, new Promise((resolve) => { resolveSign = resolve; })),
  });
  host.execute({ op: 'signPayload', request: 9, payload: {}, timeoutMs: 20_000 });
  await Promise.resolve();
  host.dispose();
  assert.equal(signal.aborted, true);
  assert.equal(timers, 0);
  assert.equal(host.signings.size, 0);
  resolveSign({ signature: 'late' });
  await new Promise((resolve) => setImmediate(resolve));
});

test('missing, rejected and malformed delegated signers fail without making HTTP requests', async () => {
  for (const signPayload of [undefined, () => { throw new Error('denied'); }, () => ({ signature: 123 }), () => ({ signature: 'answer', headers: { 'X-Proof': 123 } })]) {
    const failures = [];
    const host = new EngineHost({ payloadSignFailed: (...args) => (failures.push(args), '') }, {
      now: () => 0, setTimeout, clearTimeout, signPayload,
      request() { throw new Error('must never issue HTTP'); },
    });
    host.execute({ op: 'signPayload', request: 10, payload: {}, timeoutMs: 1000 });
    await new Promise((resolve) => setImmediate(resolve));
    assert.equal(failures.length, 1);
    assert.match(failures[0][1], /not configured|denied|Invalid payload signer (result|header)/);
    assert.equal(host.signings.size, 0);
    host.dispose();
  }
});

test('delegated header lines preserve empty values, colons and surrounding whitespace', async () => {
  for (const [headers, expected] of [
    [undefined, ''],
    [{}, ''],
    [{ 'X-Empty': '', 'X-Exact': '  a:b:c \t' }, 'X-Empty:\nX-Exact:  a:b:c \t'],
  ]) {
    const received = [];
    const host = new EngineHost({ payloadSigned: (_id, _signature, lines) => (received.push(lines), '') }, {
      now: () => 0, setTimeout, clearTimeout,
      signPayload: () => ({ signature: 'opaque', headers }),
    });
    host.execute({ op: 'signPayload', request: 11, payload: {}, timeoutMs: 1000 });
    await new Promise((resolve) => setImmediate(resolve));
    assert.deepEqual(received, [expected]);
    host.dispose();
  }
});

test('delegated header names and CR/LF fail before encoding or making HTTP requests', async () => {
  for (const headers of [
    { 'X-Proof': 'valid\nInjected: value' },
    { 'X-Proof': 'valid\rInjected: value' },
    { 'X-Proof\nInjected': 'value' },
    { 'X-Proof\rInjected': 'value' },
    { 'Invalid Header': 'value' },
    { '': 'value' },
  ]) {
    const failures = [];
    const host = new EngineHost({
      payloadSigned() { assert.fail('invalid headers must never enter WASM as lines'); },
      payloadSignFailed: (_id, error) => (failures.push(error), ''),
    }, {
      now: () => 0, setTimeout, clearTimeout,
      signPayload: () => ({ signature: 'opaque', headers }),
      request() { assert.fail('invalid headers must never reach HTTP'); },
    });
    host.execute({ op: 'signPayload', request: 12, payload: {}, timeoutMs: 1000 });
    await new Promise((resolve) => setImmediate(resolve));
    assert.deepEqual(failures, ['Invalid payload signer header']);
    assert.equal(host.signings.size, 0);
    host.dispose();
  }
});

for (const outcome of ['success', 'rejection', 'timeout']) {
  test(`delegated signing ${outcome} waits behind raw input and preserves its completion clock`, async () => {
    const f = fixture();
    let now = 23;
    f.host.transport.now = () => now;
    f.host.transport.signPayload = () => outcome === 'timeout'
      ? new Promise(() => {})
      : outcome === 'rejection' ? Promise.reject(new Error('fixture denied'))
        : Promise.resolve({ signature: 'answer' });
    f.engine.payloadSigned = (...args) => (f.calls.push(['signed', ...args]), '');
    f.engine.payloadSignFailed = (...args) => (f.calls.push(['signFailed', ...args]), '');
    const release = f.host.reserveIngress();
    f.host.execute({ op: 'signPayload', request: 7, payload: {}, timeoutMs: 100 });
    if (outcome === 'timeout') {
      await Promise.resolve();
      f.tick();
    } else await new Promise((resolve) => setImmediate(resolve));
    assert.deepEqual(f.calls, []);
    assert.equal(f.host.signings.size, 0);
    now = 99;
    release(() => f.arrivals.get('1').onMessage('earlier', 17));
    f.tick();
    assert.deepEqual(f.calls[0], ['single', [[1, 17, 'earlier']]]);
    assert.deepEqual(f.calls[1], outcome === 'success'
      ? ['signed', 7, 'answer', '', 23]
      : ['signFailed', 7, outcome === 'timeout' ? 'Payload signing timed out' : 'fixture denied', 23]);
    f.host.dispose();
  });
}
