import { test } from 'node:test';
import assert from 'node:assert/strict';
import { gzipSync } from 'node:zlib';
import { nativeTransport } from './host.mjs';

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
