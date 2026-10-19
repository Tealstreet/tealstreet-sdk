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
