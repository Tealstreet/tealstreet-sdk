import assert from 'node:assert/strict';
import { gzipSync, deflateSync } from 'node:zlib';
import { test } from 'node:test';
import { EngineHost, nativeTransport, socketMessages } from './host.mjs';

const delayedGzip = (text) => {
  let release;
  const ready = new Promise((resolve) => { release = resolve; });
  const data = new class extends Blob {
    async arrayBuffer() { await ready; return super.arrayBuffer(); }
  }([gzipSync(text)]);
  return { data, release };
};

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
  const engine = {
    wsMessages: (bytes) => {
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
