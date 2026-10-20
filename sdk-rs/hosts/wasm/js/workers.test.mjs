// The Workers host against fake Workers primitives (WebSocketPair, upgrade
// fetch) and the real Node build of the engine. Run after `yarn rust:build-wasm`.
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { test } from 'node:test';

import { createEngineObject, workersTransport, WorkersWebSocket } from './workers.mjs';
import { Frame } from './wire.mjs';

const require = createRequire(import.meta.url);
const loadEngine = async () => require('../pkg/node/binance/safe_cex_wasm.js');

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
    upgrades.push({ url, init, socket, headers: init.headers });
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
    headers: { 'ACCESS-SIGN': 'signature' },
    onOpen: () => events.push('open'),
    onMessage: (text) => events.push(text),
    onClose: (code) => events.push(code),
  });
  await until(() => upgrades.length > 0);
  const { url, socket, headers } = upgrades.at(-1);
  assert.equal(url, 'https://fstream.binance.com/public/ws');
  assert.equal(headers['ACCESS-SIGN'], 'signature');
  socket.dispatch('message', { data: 'hello' });
  socket.close(1001);
  assert.deepEqual(events, ['open', 'hello', 1001]);
});

test('the Workers transport delivers pending binary data before a single remote close', async () => {
  let release;
  const ready = new Promise((resolve) => { release = resolve; });
  const data = new class extends Blob {
    async arrayBuffer() { await ready; return super.arrayBuffer(); }
  }(['last fill']);
  const events = [];
  const before = upgrades.length;
  workersTransport().connect('wss://fixture/market', {
    onOpen() {},
    onMessage: (text, now) => events.push([text, now]),
    onClose: (code, now) => events.push(['close', code, now]),
  });
  await until(() => upgrades.length > before);
  const socket = upgrades.at(-1).socket;
  const originalNow = Date.now;
  try {
    Date.now = () => 17;
    socket.dispatch('message', { data });
    Date.now = () => 18;
    socket.dispatch('message', { data: 'later text' });
    Date.now = () => 19;
    socket.dispatch('close', { code: 1000 });
    socket.dispatch('error', {});
  } finally { Date.now = originalNow; }
  assert.deepEqual(events, []);
  release();
  await until(() => events.length >= 3);
  assert.deepEqual(events, [['last fill', 17], ['later text', 18], ['close', 1000, 19]]);
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

const deferred = () => {
  let resolve;
  let reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
};

const idleModule = (constructed) => ({
  WasmEngine: class {
    constructor(venue) { constructed.push(venue); }
    start() { return ''; }
    command(json) { return JSON.stringify([{ op: 'done', command: JSON.parse(json).command, result: '"done"' }]); }
    takeFrame() { return undefined; }
    snapshotFrame() { return new Uint8Array(); }
  },
});

const connectClient = async (object) => {
  const response = await object.fetch({ headers: new Map([['Upgrade', 'websocket']]) });
  return response.webSocket.peer;
};

test('concurrent starts share one load and reject a conflicting venue', async () => {
  const loading = deferred();
  const constructed = [];
  const loaded = [];
  const EngineObject = createEngineObject({ loadEngine: (venue) => { loaded.push(venue); return loading.promise; } });
  const object = new EngineObject({ storage: { setAlarm: async () => {} } });
  const first = await connectClient(object);
  const second = await connectClient(object);
  const start = object.receive(first, '{"op":"start","venue":"okx"}');
  const same = object.receive(second, '{"op":"start","venue":"okx"}');
  await assert.rejects(object.receive(second, '{"op":"start","venue":"bybit"}'), /different venue/);
  assert.deepEqual(loaded, ['okx']);
  loading.resolve(idleModule(constructed));
  await Promise.all([start, same]);
  assert.deepEqual(constructed, ['okx']);
  assert.equal(object.timers.length, 2);
  object.leave(first);
  assert.ok(object.host, 'another active client keeps the engine');
  object.leave(second);
  assert.equal(object.host, undefined);
  assert.equal(object.timers.length, 0);
});

test('leaving during load invalidates the attempt across a new connection', async () => {
  const oldLoad = deferred();
  const newLoad = deferred();
  const constructed = [];
  const EngineObject = createEngineObject({ loadEngine: (venue) => venue === 'okx' ? oldLoad.promise : newLoad.promise });
  const object = new EngineObject({ storage: { setAlarm: async () => {} } });
  const oldClient = await connectClient(object);
  const oldStart = object.receive(oldClient, '{"op":"start","venue":"okx"}');
  object.leave(oldClient);
  const newClient = await connectClient(object);
  const newStart = object.receive(newClient, '{"op":"start","venue":"bybit"}');
  oldLoad.resolve(idleModule(constructed));
  await oldStart;
  assert.deepEqual(constructed, [], 'the abandoned account never constructs an engine');
  assert.equal(object.host, undefined);
  assert.equal(object.starting.venue, 'bybit', 'the stale load cannot clear its replacement');
  newLoad.resolve(idleModule(constructed));
  await newStart;
  assert.deepEqual(constructed, ['bybit']);
  object.leave(newClient);
});

test('a failed venue download can retry without dropping active clients', async () => {
  const loading = deferred();
  const constructed = [];
  let attempts = 0;
  const EngineObject = createEngineObject({ loadEngine: () => ++attempts === 1 ? loading.promise : Promise.resolve(idleModule(constructed)) });
  const object = new EngineObject({ storage: { setAlarm: async () => {} } });
  const client = await connectClient(object);
  const start = object.receive(client, '{"op":"start","venue":"okx"}');
  const command = object.receive(client, '{"op":"command","id":11,"command":{"op":"fetchCandles"}}');
  loading.reject(new Error('download interrupted'));
  await assert.rejects(start, /download interrupted/);
  await command;
  assert.deepEqual(JSON.parse(client.sent.at(-1)), { op: 'done', id: 11, error: 'download interrupted' });
  assert.equal(object.starting, undefined);
  assert.equal(object.host, undefined);
  assert.equal(object.clients.size, 1);
  await object.receive(client, '{"op":"start","venue":"okx"}');
  assert.equal(attempts, 2);
  assert.deepEqual(constructed, ['okx']);
  object.leave(client);
});

test('commands await a pending start and report a missing engine explicitly', async () => {
  const loading = deferred();
  const EngineObject = createEngineObject({ loadEngine: () => loading.promise });
  const object = new EngineObject({ storage: { setAlarm: async () => {} } });
  const client = await connectClient(object);
  await object.receive(client, '{"op":"command","id":7,"command":{"op":"fetchCandles"}}');
  assert.deepEqual(JSON.parse(client.sent.pop()), { op: 'done', id: 7, error: 'the engine has not started' });
  const start = object.receive(client, '{"op":"start","venue":"okx"}');
  const command = object.receive(client, '{"op":"command","id":8,"command":{"op":"fetchCandles"}}');
  assert.deepEqual(client.sent, []);
  loading.resolve(idleModule([]));
  await Promise.all([start, command]);
  assert.deepEqual(JSON.parse(client.sent.at(-1)), { op: 'done', id: 8, result: 'done' });
  object.leave(client);
});

test('Workers derives private upgrade capability from its transport', async () => {
  let options;
  const EngineObject = createEngineObject({ loadEngine: async () => ({ WasmEngine: class {
    constructor(_, json) { options = JSON.parse(json); }
    start() { return ''; }
  } }) });
  const object = new EngineObject({ storage: { setAlarm() {} } });
  const client = { send() {} };
  object.clients.add(client);
  try {
    await object.receive(client, JSON.stringify({ op: 'start', venue: 'weex', options: { key: 'replay-key', privateWsHeaders: false } }));
    assert.equal(options.privateWsHeaders, true);
    assert.equal(options.key, 'replay-key');
  } finally { object.leave(client); }
});

test('Workers runs engine effects through its configured account transport', async () => {
  const destinations = [];
  const EngineObject = createEngineObject({
    loadEngine: async () => ({ WasmEngine: class {
      start() { return JSON.stringify([{ op: 'wsConnect', socket: 1, url: 'wss://venue.test/shared', channel: 'private' }]); }
    } }),
    createTransport: (venue, options, raw) => {
      assert.equal(venue, 'okx');
      assert.equal(options.tealstreetProxy, true);
      return { ...raw, connect: (url, handlers) => {
        destinations.push([url, handlers.channel]);
        return { send() {}, close() {} };
      } };
    },
  });
  const object = new EngineObject({ storage: { setAlarm() {} } });
  const client = { send() {} };
  object.clients.add(client);
  try {
    await object.receive(client, JSON.stringify({ op: 'start', venue: 'okx', options: { tealstreetProxy: true } }));
    assert.deepEqual(destinations, [['wss://venue.test/shared', 'private']]);
  } finally { object.leave(client); }
});

test('Workers refuses requested routing without an account transport factory', async () => {
  const messages = [];
  const EngineObject = createEngineObject({ loadEngine: async () => ({ WasmEngine: class {
    constructor() { throw new Error('must not construct an unrouted engine'); }
  } }) });
  const object = new EngineObject({ storage: { setAlarm() {} } });
  for (const options of [{ tealstreetProxy: true }, { tealstreetTunnel: true }, { tealstreetWsRoute: true },
    { overrideProxyUrl: 'https://proxy.test' }, { bybitSiteId: 'BRA_BTL' }]) {
    const client = { send: (text) => messages.push(JSON.parse(text)) };
    object.clients.add(client);
    await object.receive(client, JSON.stringify({ op: 'start', venue: 'bingx', options }));
    object.leave(client);
  }
  assert.equal(object.host, undefined);
  assert.equal(messages[0].level, 'error');
  assert.match(messages[0].message, /configured createTransport/);
  assert.equal(messages.length, 5);
});


test('the Workers raw constructor preserves upgrade headers, protocol and binary payloads', async () => {
  const before = upgrades.length;
  const socket = new WorkersWebSocket('wss://tunnel.tealstreet.io/tunnel?region=one&region=two', ['first', 'second'], {
    headers: { Origin: 'https://preview.tealstreet.io', 'User-Agent': 'worker-agent' },
  });
  const messages = [];
  socket.onmessage = (event) => messages.push(event.data);
  await until(() => socket.readyState === socket.OPEN);
  const upgrade = upgrades[before];
  assert.equal(upgrade.url, 'https://tunnel.tealstreet.io/tunnel?region=one&region=two');
  assert.deepEqual(upgrade.init.headers, {
    Origin: 'https://preview.tealstreet.io', 'User-Agent': 'worker-agent', Upgrade: 'websocket',
    'Sec-WebSocket-Protocol': 'first, second',
  });
  const binary = new Uint8Array([0x54, 0x53, 0x42, 0x31]).buffer;
  upgrade.socket.dispatch('message', { data: binary });
  assert.equal(messages[0], binary);
  socket.close();
});

test('closing a pending Workers upgrade aborts it and suppresses a late open', async () => {
  const original = globalThis.fetch;
  let finish;
  let signal;
  globalThis.fetch = (_url, init) => {
    signal = init.signal;
    return new Promise((resolve) => { finish = resolve; });
  };
  try {
    const events = [];
    const socket = new WorkersWebSocket('wss://tunnel.tealstreet.io/tunnel');
    socket.onopen = () => events.push('open');
    socket.onmessage = () => events.push('message');
    socket.onerror = () => events.push('error');
    socket.onclose = () => events.push('close');
    socket.close();
    assert.equal(signal.aborted, true);
    const raw = new FakeSocket();
    let closed = false;
    raw.addEventListener('close', () => { closed = true; });
    finish({ webSocket: raw });
    await new Promise((resolve) => setTimeout(resolve, 0));
    assert.equal(closed, true);
    assert.deepEqual(events, ['close']);
    assert.equal(socket.readyState, socket.CLOSED);
  } finally { globalThis.fetch = original; }
});

test('a Workers raw socket failure reports one abnormal close despite a following close event', async () => {
  const before = upgrades.length;
  const events = [];
  const socket = new WorkersWebSocket('wss://tunnel.tealstreet.io/tunnel');
  socket.onerror = () => events.push('error');
  socket.onclose = (event) => events.push(event.code);
  await until(() => socket.readyState === socket.OPEN);
  const raw = upgrades[before].socket;
  raw.dispatch('error', {});
  raw.close(1000);
  assert.deepEqual(events, ['error', 1006]);
});

test('credential deactivation reaches connected Workers clients once', async () => {
  const received = [];
  const EngineObject = createEngineObject({ loadEngine: async () => ({ WasmEngine: class {
    start() { return '[{"op":"deactivate"}]'; }
  } }) });
  const object = new EngineObject({ storage: { setAlarm() {} } });
  const client = { send: (text) => received.push(JSON.parse(text)) };
  object.clients.add(client);
  try {
    await object.receive(client, JSON.stringify({ op: 'start', venue: 'orderly', options: {} }));
    assert.deepEqual(received, [{ op: 'deactivate' }]);
  } finally { object.leave(client); }
});
