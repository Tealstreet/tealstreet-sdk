import { test } from 'node:test';
import assert from 'node:assert/strict';
import { deflateSync, gzipSync } from 'node:zlib';
import { bindSocketMessages, nativeTransport } from './host.mjs';

class MessageSocket {
  listeners = {};
  closes = 0;
  addEventListener(name, fn) { this.listeners[name] = fn; }
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
  let ws;
  class FakeSocket {
    listeners = {};
    constructor() { ws = this; }
    addEventListener(name, fn) { this.listeners[name] = fn; }
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
  } finally { globalThis.WebSocket = original; }
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
    assert.deepEqual(events, []);
  } finally { globalThis.WebSocket = original; }
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
