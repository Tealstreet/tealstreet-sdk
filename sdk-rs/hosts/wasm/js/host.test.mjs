import assert from 'node:assert/strict';
import { test } from 'node:test';
import { EngineHost } from './host.mjs';

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
