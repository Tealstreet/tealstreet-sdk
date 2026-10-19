// The Workers host against fake Workers primitives (WebSocketPair, upgrade
// fetch) and the real Node build of the engine. Run after `yarn rust:build-wasm`.
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { test } from 'node:test';
import { deflateSync, gzipSync } from 'node:zlib';

import { createEngineObject, workersTransport } from './workers.mjs';
import { Frame } from './wire.mjs';

const require = createRequire(import.meta.url);
const loadEngine = async () => require('../pkg/node/safe_cex_wasm.js');

const RESPONSES = {
  exchangeInfo: JSON.stringify({
    symbols: [
      {
        symbol: 'BTCUSDT',
        status: 'TRADING',
        baseAsset: 'BTC',
        quoteAsset: 'USDT',
        marginAsset: 'USDT',
        contractType: 'PERPETUAL',
        filters: [
          { filterType: 'PRICE_FILTER', tickSize: '0.10' },
          { filterType: 'LOT_SIZE', stepSize: '0.001', minQty: '0.001', maxQty: '1000' },
          { filterType: 'MARKET_LOT_SIZE', stepSize: '0.001', minQty: '0.001', maxQty: '120' },
        ],
      },
    ],
  }),
  '24hr': '[{"symbol":"BTCUSDT","lastPrice":"100","priceChangePercent":"1","volume":"2","quoteVolume":"3"}]',
  bookTicker: '[{"symbol":"BTCUSDT","bidPrice":"99.9","askPrice":"100.1"}]',
  premiumIndex:
    '[{"symbol":"BTCUSDT","markPrice":"100","indexPrice":"100","lastFundingRate":"0.0001","nextFundingTime":1700000000000}]',
  klines: '[[1700000000000,"1","2","0.5","1.5","10",1700000059999,"15",5,"5","7","0"]]',
};

/** A socket with Workers' accept/addEventListener surface. */
class FakeSocket {
  constructor() {
    this.listeners = {};
    this.sent = [];
    this.peer = undefined;
  }
  accept() {}
  addEventListener(type, fn) {
    (this.listeners[type] ??= []).push(fn);
  }
  dispatch(type, event) {
    for (const fn of this.listeners[type] ?? []) fn(event);
  }
  send(data) {
    this.sent.push(data);
    this.peer?.dispatch('message', { data });
  }
  close(code = 1000) {
    this.dispatch('close', { code });
    this.peer?.dispatch('close', { code });
  }
}

const upgrades = [];
globalThis.WebSocketPair = class {
  constructor() {
    const [a, b] = [new FakeSocket(), new FakeSocket()];
    a.peer = b;
    b.peer = a;
    return { 0: a, 1: b };
  }
};
// Node's Response refuses status 101; Workers allow it with a webSocket.
globalThis.Response = class {
  constructor(body, init = {}) {
    this.status = init.status;
    this.webSocket = init.webSocket;
  }
};
globalThis.fetch = async (url, init = {}) => {
  if (init.headers?.Upgrade === 'websocket') {
    const socket = new FakeSocket();
    upgrades.push({ url, socket });
    return { webSocket: socket };
  }
  const name = new URL(url).pathname.split('/').pop();
  const body = RESPONSES[name];
  const bytes = new TextEncoder().encode(body ?? '{}');
  return {
    status: body ? 200 : 404,
    headers: new Map(),
    body: new ReadableStream({
      start(controller) {
        controller.enqueue(bytes);
        controller.close();
      },
    }),
  };
};

const until = async (check) => {
  for (let i = 0; i < 300 && !check(); i++) await new Promise((resolve) => setTimeout(resolve, 10));
  assert.ok(check());
};

test('the transport opens client sockets with an https upgrade', async () => {
  const events = [];
  workersTransport().connect('wss://fstream.binance.com/public/ws', {
    onOpen: () => events.push('open'),
    onMessage: (text) => events.push(text),
    onClose: (code) => events.push(code),
  });
  await until(() => upgrades.length > 0);
  const { url, socket } = upgrades.at(-1);
  assert.equal(url, 'https://fstream.binance.com/public/ws');
  socket.dispatch('message', { data: 'hello' });
  socket.close(1001);
  assert.deepEqual(events, ['open', 'hello', 1001]);
});

test('Worker binary gzip, zlib, ArrayBuffer and text messages precede closure', async () => {
  const events = [];
  workersTransport().connect('wss://example.test/public/', {
    onOpen: () => events.push('open'),
    onMessage: (text) => events.push(text),
    onClose: (code) => events.push(code),
  });
  await until(() => events[0] === 'open');
  const { socket } = upgrades.at(-1);
  socket.dispatch('message', { data: new Blob([gzipSync('{"book":1}')]) });
  socket.dispatch('message', { data: deflateSync('{"book":2}') });
  socket.dispatch('message', { data: new TextEncoder().encode('{"book":3}').buffer });
  socket.dispatch('message', { data: '{"book":4}' });
  socket.close(1001);
  socket.dispatch('error', {});
  await until(() => events.at(-1) === 1001);
  assert.deepEqual(events, ['open', '{"book":1}', '{"book":2}', '{"book":3}', '{"book":4}', 1001]);
});

test('disposing a Worker socket while binary decode waits suppresses pending callbacks', async () => {
  let release;
  let started;
  const gate = new Promise((resolve) => { release = resolve; });
  const decoding = new Promise((resolve) => { started = resolve; });
  class DelayedBlob extends Blob {
    async arrayBuffer() { started(); await gate; return super.arrayBuffer(); }
  }
  const events = [];
  const connection = workersTransport().connect('wss://example.test/public/', {
    onOpen: () => events.push('open'),
    onMessage: (text) => events.push(text),
    onClose: (code) => events.push(code),
  });
  await until(() => events[0] === 'open');
  const { socket } = upgrades.at(-1);
  socket.dispatch('message', { data: new DelayedBlob([gzipSync('stale book')]) });
  socket.dispatch('message', { data: 'stale ticker' });
  await decoding;
  connection.close();
  release();
  await new Promise((resolve) => setImmediate(resolve));
  assert.deepEqual(events, ['open']);
});

test('closing before a Worker upgrade resolves closes the acquired socket without callbacks', async (t) => {
  let resolveUpgrade;
  const upgrading = new Promise((resolve) => { resolveUpgrade = resolve; });
  t.mock.method(globalThis, 'fetch', () => upgrading);
  const events = [];
  const connection = workersTransport().connect('wss://example.test/public/', {
    onOpen: () => events.push('open'),
    onMessage: (text) => events.push(text),
    onClose: (code) => events.push(code),
  });
  connection.close();
  const socket = new FakeSocket();
  let closes = 0;
  socket.addEventListener('close', () => closes++);
  resolveUpgrade({ webSocket: socket });
  await until(() => closes === 1);
  socket.dispatch('message', { data: 'too late' });
  assert.deepEqual(events, []);
});

test('a failed Worker upgrade cannot notify a disposed connection', async (t) => {
  let rejectUpgrade;
  const upgrading = new Promise((_, reject) => { rejectUpgrade = reject; });
  t.mock.method(globalThis, 'fetch', () => upgrading);
  const events = [];
  const connection = workersTransport().connect('wss://example.test/public/', {
    onOpen: () => events.push('open'),
    onMessage: (text) => events.push(text),
    onClose: (code) => events.push(code),
  });
  connection.close();
  rejectUpgrade(new Error('upgrade failed'));
  await new Promise((resolve) => setImmediate(resolve));
  assert.deepEqual(events, []);
});

test('the Durable Object runs an engine and streams frames to its clients', async () => {
  const EngineObject = createEngineObject({ loadEngine, flush: { store: 20, books: 20 } });
  const alarms = [];
  const object = new EngineObject({ storage: { setAlarm: async (at) => alarms.push(at) } });
  const response = await object.fetch({ headers: new Map([['Upgrade', 'websocket']]) });
  assert.equal(response.status, 101);
  const client = response.webSocket;
  const received = [];
  client.addEventListener('message', (event) => received.push(event.data));
  assert.equal(alarms.length, 1);

  client.send(JSON.stringify({ op: 'start', venue: 'binance', options: { publicDataOnly: true } }));
  await until(() => received.some((m) => m instanceof ArrayBuffer && new Frame(m).markets()));
  const frame = new Frame(received.find((m) => m instanceof ArrayBuffer && new Frame(m).markets()));
  assert.equal(frame.markets().symbol(0), 'BTCUSDT');

  client.send(JSON.stringify({ op: 'command', id: 7, command: { op: 'fetchCandles', symbol: 'BTCUSDT', interval: '1m' } }));
  await until(() => received.some((m) => typeof m === 'string' && m.includes('"id":7')));
  const done = JSON.parse(received.find((m) => typeof m === 'string' && m.includes('"id":7')));
  assert.deepEqual(done.result, [{ timestamp: 1700000000, open: 1, high: 2, low: 0.5, close: 1.5, volume: 10 }]);

  client.close();
  assert.equal(object.host, undefined, 'the last client leaving stops the engine');
});
