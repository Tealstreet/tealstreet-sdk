import assert from 'node:assert/strict';
import { gzipSync, deflateSync } from 'node:zlib';
import { test } from 'node:test';
import { EngineHost, nativeTransport, socketMessages } from './host.mjs';
import { workersTransport } from './workers.mjs';

const delayedGzip = (text) => {
  let release;
  const ready = new Promise((resolve) => { release = resolve; });
  const data = new class extends Blob {
    async arrayBuffer() { await ready; return super.arrayBuffer(); }
  }([gzipSync(text)]);
  return { data, release };
};

const ingressFixture = () => {
  let now = 17, timer;
  const events = [], handlers = new Map();
  const engine = {
    wsMessage: (id, text, at) => { events.push(['message', id, at, text]); return ''; },
    timer: (_id, at) => { events.push(['timer', at]); return ''; },
    command: (text, at) => { events.push(['command', at, JSON.parse(text).symbol]); return ''; },
    httpStart: (_id, _status, _headers, at) => { events.push(['httpStart', at]); return ''; },
    httpChunk: (_id, bytes, at) => { events.push(['httpChunk', at, [...bytes]]); return ''; },
    httpEnd: (_id, at) => { events.push(['httpEnd', at]); return ''; },
    wsClose: (id, code, at) => { events.push(['close', id, at, code]); return ''; },
    snapshotFrame: () => new Uint8Array([events.length]),
    accountId: () => String(events.length),
    takeFrame: () => { events.push(['flush']); },
  };
  engine.wsMessages = (bytes) => {
    const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    for (let at = 0; at < bytes.length;) {
      const size = view.getUint32(at + 4, true);
      engine.wsMessage(view.getUint32(at, true), new TextDecoder().decode(bytes.subarray(at + 16, at + 16 + size)), view.getFloat64(at + 8, true));
      at += 16 + size;
    }
    return '';
  };
  const host = new EngineHost(engine, {
    now: () => now,
    setTimeout: (fn) => { timer = fn; return fn; }, clearTimeout() {},
    connect: (url, callbacks) => { handlers.set(url, callbacks); return { send() {}, close() {} }; },
  });
  for (const id of [1, 2]) host.execute({ op: 'wsConnect', socket: id, url: String(id) });
  const receive = (id) => {
    const h = handlers.get(String(id));
    return socketMessages(h.onMessage, (_error, at) => h.onClose(1007, at), h.onClose, false, h.onIngress);
  };
  return { host, events, receive, at: (value) => { now = value; }, fire: () => timer() };
};

test('raw decode reservations fence timers, commands, HTTP and synchronous prefix reads', async () => {
  const f = ingressFixture(), delayed = delayedGzip('fill');
  const pending = f.receive(1)(delayed.data, 17);
  f.at(18);
  f.host.execute({ op: 'setTimer', timer: 9, at: 18 });
  f.fire();
  const command = { op: 'subscribeOrderBook', symbol: 'original' };
  const commanded = f.host.command(command);
  command.symbol = 'mutated';
  const bytes = new Uint8Array([7]);
  const response = f.host.receive(4, { status: 200, body: bytes });
  bytes[0] = 99;
  assert.deepEqual(f.events, []);
  assert.deepEqual([...new Uint8Array(f.host.snapshot())], [0]);
  assert.equal(f.host.accountId(), '0');
  f.host.flush();
  delayed.release();
  await pending;
  await commanded;
  await response;
  assert.deepEqual(f.events, [
    ['message', 1, 17, 'fill'], ['timer', 18], ['command', 18, 'original'],
    ['httpStart', 18], ['httpChunk', 18, [7]], ['httpEnd', 18], ['flush'],
  ]);
  f.host.dispose();
});

test('raw reservations order another socket and remote close before later controls', async () => {
  const f = ingressFixture(), delayed = delayedGzip('first'), first = f.receive(1);
  const pending = first(delayed.data, 17);
  f.at(18);
  f.receive(2)('second', 18);
  const closed = first.close(1000, 19);
  f.at(20);
  f.host.execute({ op: 'setTimer', timer: 9, at: 20 }); f.fire();
  assert.deepEqual(f.events, []);
  delayed.release(); await pending; await closed;
  assert.deepEqual(f.events, [['message', 1, 17, 'first'], ['message', 2, 18, 'second'], ['close', 1, 19, 1000], ['timer', 20]]);
  f.host.dispose();
});

test('an available whole HTTP body keeps its original callback position and owned bytes', async () => {
  const f = ingressFixture(), delayed = delayedGzip('first');
  const pending = f.receive(1)(delayed.data, 17);
  f.at(18);
  const bytes = new Uint8Array([7]);
  const response = f.host.receive(4, { status: 200, body: bytes });
  bytes[0] = 99;
  f.at(19);
  const command = f.host.command({ op: 'subscribeOrderBook', symbol: 'after response' });
  f.at(100); delayed.release();
  await pending; await response; await command;
  assert.deepEqual(f.events, [
    ['message', 1, 17, 'first'], ['httpStart', 18], ['httpChunk', 18, [7]], ['httpEnd', 18],
    ['command', 19, 'after response'],
  ]);
  f.host.dispose();
});

test('disposal settles HTTP waiting on raw decode without pulling its stream', async () => {
  const f = ingressFixture(), delayed = delayedGzip('disposed');
  const pending = f.receive(1)(delayed.data, 17);
  let pulled = false;
  const body = async function* () { pulled = true; yield new Uint8Array([7]); };
  const response = f.host.receive(4, { status: 200, body: body() });
  f.host.dispose(); await response;
  assert.equal(pulled, false);
  assert.deepEqual(f.events, []);
  delayed.release(); await pending;
});

test('an available stream chunk keeps owned bytes and its position before later commands', async () => {
  const f = ingressFixture(), delayed = delayedGzip('first');
  let releaseChunk;
  const ready = new Promise((resolve) => { releaseChunk = resolve; });
  const bytes = new Uint8Array([7]);
  const body = async function* () { await ready; yield bytes; };
  f.at(1);
  const response = f.host.receive(4, { status: 200, body: body() });
  f.at(17); const pending = f.receive(1)(delayed.data, 17);
  f.at(18); releaseChunk();
  await new Promise(setImmediate);
  bytes[0] = 99;
  f.at(19);
  const command = f.host.command({ op: 'subscribeOrderBook', symbol: 'after chunk' });
  f.at(100); delayed.release();
  await pending; await response; await command;
  assert.deepEqual(f.events, [
    ['httpStart', 1], ['message', 1, 17, 'first'], ['httpChunk', 18, [7]],
    ['command', 19, 'after chunk'], ['httpEnd', 100],
  ]);
  f.host.dispose();
});

test('cancellation and disposal settle raw reservations and reject deferred commands', async () => {
  const f = ingressFixture(), delayed = delayedGzip('cancelled'), receive = f.receive(1);
  const pending = receive(delayed.data, 17);
  f.at(18);
  const command = f.host.command({ op: 'subscribeOrderBook', symbol: 'after cancel' });
  receive.cancel(); await command;
  assert.deepEqual(f.events, [['command', 18, 'after cancel']]);
  delayed.release(); await pending;
  const second = delayedGzip('disposed');
  const unresolved = f.receive(2)(second.data, 19);
  const rejected = f.host.command({ op: 'fetchBalance' });
  f.host.dispose();
  await assert.rejects(rejected, /engine disposed/);
  second.release(); await unresolved;
  assert.deepEqual(f.events, [['command', 18, 'after cancel']]);
});

for (const kind of ['native', 'workers']) for (const malformed of [false, true]) {
  test(`${kind} transport reserves raw decode and close positions across sockets${malformed ? ' on failure' : ''}`, async (t) => {
    const physical = [];
    class Socket {
      listeners = new Map();
      constructor() { physical.push(this); }
      addEventListener(type, fn) { this.listeners.set(type, fn); }
      emit(type, data) { this.listeners.get(type)?.(data); }
      accept() {}
      send() {}
      close() { this.emit('close', { code: 1000 }); }
    }
    const original = { fetch: globalThis.fetch, socket: globalThis.WebSocket, builtin: process.getBuiltinModule, now: Date.now };
    t.after(() => {
      globalThis.fetch = original.fetch; globalThis.WebSocket = original.socket;
      process.getBuiltinModule = original.builtin; Date.now = original.now;
    });
    let transport;
    if (kind === 'native') {
      globalThis.WebSocket = Socket; process.getBuiltinModule = () => undefined;
      transport = nativeTransport();
    } else {
      globalThis.fetch = async () => ({ webSocket: new Socket() });
      transport = workersTransport();
    }
    let now = 1, fire, complete;
    const finished = new Promise((resolve) => { complete = resolve; });
    Date.now = () => now;
    transport.now = () => now;
    transport.setTimeout = (fn) => { fire = fn; return fn; };
    transport.clearTimeout = () => {};
    const events = [];
    const engine = {
      wsOpen: () => '',
      wsMessage: (id, text, at) => { events.push(['message', id, at, text]); return ''; },
      wsClose: (id, code, at) => { events.push(['close', id, at, code]); return ''; },
      timer: (_id, at) => { events.push(['timer', at]); complete(); return ''; },
    };
    const host = new EngineHost(engine, transport, {}, { batchMessages: false });
    t.after(() => host.dispose());
    for (const id of [1, 2]) host.execute({ op: 'wsConnect', socket: id, url: 'wss://fixture/' + id });
    await Promise.resolve(); await Promise.resolve();
    let release;
    const ready = new Promise((resolve) => { release = resolve; });
    const data = new class extends Blob { async arrayBuffer() { await ready; return super.arrayBuffer(); } }([
      malformed ? new Uint8Array([0x1f, 0x8b, 0]) : gzipSync('first'),
    ]);
    now = 17; physical[0].emit('message', { data });
    now = 18; physical[1].emit('message', { data: 'second' });
    now = 19; physical[0].emit('close', { code: 1000 });
    now = 20; host.execute({ op: 'setTimer', timer: 9, at: 20 }); fire();
    assert.deepEqual(events, []);
    release(); await finished;
    assert.deepEqual(events, malformed
      ? [['close', 1, 17, 1007], ['message', 2, 18, 'second'], ['timer', 20]]
      : [['message', 1, 17, 'first'], ['message', 2, 18, 'second'], ['close', 1, 19, 1000], ['timer', 20]]);
  });
}

test('text is synchronous and binary feeds retain message order', async () => {
  const got = [];
  const receive = socketMessages((text) => got.push(text), (error) => { throw error; });
  receive('Ping');
  assert.deepEqual(got, ['Ping']);
  receive(new Blob([gzipSync('{"n":1}')]));
  receive('second');
  receive(deflateSync('{"n":3}'));
  receive(new TextEncoder().encode('fourth').buffer);
  for (let i=0; i<50 && got.length<5; i++) await new Promise((r) => setTimeout(r, 5));
  assert.deepEqual(got, ['Ping', '{"n":1}', 'second', '{"n":3}', 'fourth']);
});

test('ready text does not allocate a raw reservation', () => {
  let reservations = 0;
  const received = [];
  const receive = socketMessages((text) => received.push(text), assert.fail, () => {}, false, () => {
    reservations++;
    return (deliver) => deliver?.();
  });
  receive('first'); receive('second');
  assert.equal(reservations, 0);
  assert.deepEqual(received, ['first', 'second']);
});

test('invalid compression reaches the transport error callback', async () => {
  const errors = [];
  const receive = socketMessages(() => assert.fail('unexpected frame'), (e) => errors.push(e));
  receive(new Uint8Array([0x1f, 0x8b, 0]));
  for (let i=0; i<50 && errors.length===0; i++) await new Promise((r) => setTimeout(r, 5));
  assert.equal(errors.length, 1);
});

test('remote close follows decoded messages and preserves raw arrival times', async () => {
  const got = [];
  const compressed = delayedGzip('last fill');
  const receive = socketMessages((text, now) => got.push([text, now]), assert.fail, (code, now) => got.push(['close', code, now]));
  receive(compressed.data, 17);
  receive('later text', 18);
  const closed = receive.close(1000, 19);
  receive('after close', 20);
  assert.deepEqual(got, []);
  compressed.release();
  await closed;
  assert.deepEqual(got, [['last fill', 17], ['later text', 18], ['close', 1000, 19]]);
});

test('bad compressed input fails closed without poisoning its close barrier', async () => {
  const got = [];
  const receive = socketMessages((text) => got.push(text), (_error, now) => got.push(['error', now]), (code) => got.push(['close', code]));
  receive(new Uint8Array([0x1f, 0x8b, 0]), 17);
  receive('must not pass after malformed data', 18);
  const closed = receive.close(1006, 19);
  receive.close(1000, 20);
  await closed;
  assert.deepEqual(got, [['error', 17], ['close', 1006]]);
});

test('local cancellation suppresses an unresolved binary delivery and queued close', async () => {
  const got = [];
  const compressed = delayedGzip('cancelled fill');
  const receive = socketMessages((text) => got.push(text), assert.fail, (code) => got.push(code));
  const pending = receive(compressed.data, 17);
  receive.close(1000, 18);
  receive.cancel();
  compressed.release();
  await pending;
  await Promise.resolve();
  assert.deepEqual(got, []);
});

test('native transport queues remote close behind gzip and stamps the raw callbacks', async () => {
  let socket;
  class FakeSocket {
    handlers = new Map();
    constructor() { socket = this; }
    addEventListener(name, fn) { this.handlers.set(name, fn); }
    emit(name, event) { this.handlers.get(name)?.(event); }
    send() {}
    close() { this.emit('close', { code: 1000 }); }
  }
  const originalSocket = globalThis.WebSocket;
  const originalBuiltin = process.getBuiltinModule;
  globalThis.WebSocket = FakeSocket;
  process.getBuiltinModule = () => undefined;
  let transport;
  try { transport = nativeTransport(); }
  finally { globalThis.WebSocket = originalSocket; process.getBuiltinModule = originalBuiltin; }
  const calls = [];
  let crossings = 0;
  const engine = {
    wsMessages: (bytes) => {
      crossings++;
      const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
      for (let at = 0; at < bytes.length;) {
        const size = view.getUint32(at + 4, true);
        calls.push(['message', view.getFloat64(at + 8, true), new TextDecoder().decode(bytes.subarray(at + 16, at + 16 + size))]);
        at += 16 + size;
      }
      return '';
    },
    wsClose: (_id, code, now) => { calls.push(['close', code, now]); return ''; },
  };
  const host = new EngineHost(engine, transport);
  host.execute({ op: 'wsConnect', socket: 1, url: 'wss://fixture' });
  const compressed = delayedGzip('last fill');
  const originalNow = Date.now;
  try {
    Date.now = () => 17;
    socket.emit('message', { data: compressed.data });
    Date.now = () => 18;
    socket.emit('message', { data: 'later text' });
    Date.now = () => 19;
    socket.emit('close', { code: 1000 });
  } finally { Date.now = originalNow; }
  assert.deepEqual(calls, []);
  compressed.release();
  for (let i = 0; i < 50 && calls.length < 3; i++) await new Promise((resolve) => setTimeout(resolve, 5));
  assert.deepEqual(calls, [['message', 17, 'last fill'], ['message', 18, 'later text'], ['close', 1000, 19]]);
  assert.equal(crossings, 1, 'ready decoded messages retain one crossing before their close barrier');
  host.dispose();
  const disposed = new EngineHost(engine, transport);
  disposed.execute({ op: 'wsConnect', socket: 2, url: 'wss://fixture' });
  const pending = delayedGzip('cancelled by disposal');
  socket.emit('message', { data: pending.data });
  disposed.dispose();
  pending.release();
  await new Promise((resolve) => setTimeout(resolve, 20));
  assert.equal(calls.length, 3, 'disposal cannot deliver a pending decode or close into the engine');
});
for (const [format, compress, field] of [['gzip', gzipSync, 'id'], ['deflate', deflateSync, 'symbol']]) {
  test(`${format} rejects malformed UTF-8 in JSON ${field} instead of delivering a changed value`, { timeout: 1000 }, async () => {
    const received = [];
    let failures = 0;
    let failed;
    const rejected = new Promise((resolve) => { failed = resolve; });
    const receive = socketMessages((text) => received.push(text), (error) => {
      failures++;
      failed(error);
    });
    const malformed = Buffer.concat([Buffer.from(`{"${field}":"`), Buffer.from([0xff]), Buffer.from('"}')]);
    receive(compress(malformed));
    const error = await rejected;
    assert.ok(error instanceof TypeError);
    assert.equal(failures, 1);
    assert.deepEqual(received, []);
  });
}

test('whole and streamed HTTP buffers cross in bounded chunks', async () => {
  for (const streamed of [false, true]) {
    const sizes=[];
    const engine={httpStart:()=>'',httpChunk:(_id,data)=>{sizes.push(data.length);return ''},httpEnd:()=>''};
    const host=new EngineHost(engine,{now:()=>0});
    const bytes=new Uint8Array(100_000);
    const chunks=async function*(){yield bytes};
    await host.receive(1,{status:200,body:streamed?chunks():bytes});
    assert.equal(sizes.reduce((a,b)=>a+b,0),100_000);
    assert.ok(Math.max(...sizes)<=16*1024);
  }
});

test('explicit binary protocols preserve bytes and control-message ordering', async () => {
  const messages = [];
  const delivered = new Promise((resolve, reject) => {
    const receive = socketMessages((data) => {
      messages.push(data);
      if (messages.length === 3) resolve();
    }, reject, true);
    // A binary application frame may begin with the gzip magic by coincidence.
    receive(new Blob([new Uint8Array([0x1f, 0x8b, 0xff, 0])]));
    receive('{"id":1,"result":{}}');
    receive(new Uint8Array([0x78, 0xff]));
  });
  await delivered;
  assert.deepEqual([...messages[0]], [0x1f, 0x8b, 0xff, 0]);
  assert.equal(messages[1], '{"id":1,"result":{}}');
  assert.deepEqual([...messages[2]], [0x78, 0xff]);
});

test('a binary engine socket crosses wasm once and ignores callbacks after disposal', () => {
  const received = [];
  let handlers;
  const host = new EngineHost({
    wsMessage: (...args) => { received.push(['text', ...args]); return ''; },
    wsBinary: (...args) => { received.push(['binary', ...args]); return ''; },
  }, {
    now: () => 42,
    connect: (_url, value) => { handlers = value; return { close() {} }; },
  }, {}, { batchMessages: false });
  host.execute({ op: 'wsConnect', socket: 9, url: 'wss://example.test', binary: true, channel: 'private' });
  assert.equal(handlers.binary, true);
  const bytes = new Uint8Array([1, 0, 0xff]);
  handlers.onBinary(bytes);
  handlers.onMessage('control');
  host.dispose();
  handlers.onBinary(bytes);
  assert.deepEqual(received, [['binary', 9, bytes, 42], ['text', 9, 'control', 42]]);
});

test('an older engine without binary ingress closes and releases the physical socket once', () => {
  let handlers;
  let closed = 0;
  const events = [];
  const host = new EngineHost({ wsClose: (...args) => { events.push(args); return ''; } }, {
    now: () => 10,
    connect: (_url, value) => {
      handlers = value;
      return { close() { closed++; handlers.onClose(1000); } };
    },
  });
  host.execute({ op: 'wsConnect', socket: 3, url: 'wss://example.test', binary: true });
  handlers.onBinary(new Uint8Array([0xff]));
  handlers.onBinary(new Uint8Array([0xff]));
  assert.equal(closed, 1);
  assert.equal(host.sockets.size, 0);
  assert.deepEqual(events, [[3, 1003, 10]]);
});

test('unsupported binary does not report another close after a queued text action closed the socket', () => {
  let handlers;
  let closed = 0;
  const events = [];
  const host = new EngineHost({
    wsMessage: () => JSON.stringify([{ op: 'wsClose', socket: 3 }]),
    wsClose: (...args) => { events.push(args); return ''; },
  }, {
    now: () => 10,
    setTimeout: () => 1,
    clearTimeout() {},
    connect: (_url, value) => {
      handlers = value;
      return { close() { closed++; handlers.onClose(1000); } };
    },
  });
  host.execute({ op: 'wsConnect', socket: 3, url: 'wss://example.test', binary: true });
  handlers.onMessage('queued close');
  handlers.onBinary(new Uint8Array([255]));
  assert.equal(closed, 1);
  assert.deepEqual(events, [[3, 1000, 10]]);
  assert.equal(host.sockets.size, 0);
  host.dispose();
});

test('existing Coinbase text sockets preserve their private role and opt out of binary ingress', () => {
  const received = [];
  let handlers;
  const host = new EngineHost({
    wsMessage: (...args) => { received.push(args); return ''; },
  }, {
    now: () => 42,
    connect: (_url, value) => { handlers = value; return { close() {} }; },
  }, {}, { batchMessages: false });
  host.execute({ op: 'wsConnect', socket: 10, url: 'wss://advanced-trade-ws-user.coinbase.com', channel: 'private' });
  assert.equal(handlers.binary, false);
  assert.equal(handlers.channel, 'private');
  handlers.onMessage('{"channel":"heartbeats","sequence_num":0,"events":[]}');
  assert.deepEqual(received, [[10, '{"channel":"heartbeats","sequence_num":0,"events":[]}', 42]]);
  host.dispose();
});
