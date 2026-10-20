// The Workers host against fake Workers primitives (WebSocketPair, upgrade
// fetch) and the real Node build of the engine. Run after `yarn rust:build-wasm`.
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { test } from 'node:test';

import { createEngineObject, workersTransport, WorkersWebSocket } from './workers.mjs';
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
    upgrades.push({ url, init, socket });
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

test('the Durable Object runs an engine and streams frames to its clients', async () => {
  const EngineObject = createEngineObject({ accountToken: () => 'account-a-token', loadEngine, flush: { store: 20, books: 20 } });
  const alarms = [];
  const object = new EngineObject({ storage: { setAlarm: async (at) => alarms.push(at) } });
  const response = await object.fetch({ headers: new Map([['Upgrade', 'websocket'], ['Authorization', 'Bearer account-a-token']]) });
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

test('Workers derives private upgrade capability from its transport', async () => {
  let options;
  const EngineObject = createEngineObject({ accountToken: () => 'account-a-token', loadEngine: async () => ({ WasmEngine: class {
    constructor(_, json) { options = JSON.parse(json); }
    start() { return ''; }
  } }) });
  const object = new EngineObject({ storage: { setAlarm() {} } });
  const client = { send() {} };
  object.clients.add(client);
  try {
    await object.receive(client, JSON.stringify({ op: 'start', venue: 'weex', options: { key: 'replay-key' } }));
    assert.equal(options.privateWsHeaders, true);
    assert.equal(options.key, 'replay-key');
  } finally { object.leave(client); }
});

test('Workers runs engine effects through its configured account transport', async () => {
  const destinations = [];
  const EngineObject = createEngineObject({ accountToken: () => 'account-a-token',
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
  const EngineObject = createEngineObject({ accountToken: () => 'account-a-token', loadEngine: async () => ({ WasmEngine: class {
    constructor() { throw new Error('must not construct an unrouted engine'); }
  } }) });
  const object = new EngineObject({ storage: { setAlarm() {} } });
  for (const options of [{ tealstreetProxy: true }, { tealstreetTunnel: true }, { tealstreetWsRoute: true },
    { overrideProxyUrl: 'https://proxy.test' }, { bybitSiteId: 'BRA_BTL' }]) {
    const client = { send: (text) => messages.push(JSON.parse(text)) };
    object.clients.add(client);
    await object.receive(client, JSON.stringify({
      op: 'start', venue: 'bingx', options,
    }));
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

test('Workers applies factory protocol options after caller flags while retaining upgrade capability', async () => {
  for (const sessionProxy of [true, false]) {
    let received;
    const EngineObject = createEngineObject({ accountToken: () => 'account-a-token',
      loadEngine: async () => ({ WasmEngine: class {
        constructor(_venue, json) { received = JSON.parse(json); }
        start() { return ''; }
      } }),
      createTransport: (_venue, _options, raw) => ({
        ...raw,
        engineOptions: { grvtSessionProxy: sessionProxy },
      }),
    });
    const object = new EngineObject({ storage: { setAlarm() {} } });
    const client = { send() {} };
  object.clients.add(client);
    try {
      await object.receive(client, JSON.stringify({ op: 'start', venue: 'grvt', options: {
        key: 'fixture-key', grvtSessionProxy: !sessionProxy, privateWsHeaders: false,
      } }));
      assert.equal(received.grvtSessionProxy, sessionProxy);
      assert.equal(received.privateWsHeaders, true);
      assert.equal(received.key, 'fixture-key');
    } finally { object.leave(client); }
  }
});

test('Workers requests honor typed deadlines through response bodies and consume routing metadata', async (t) => {
  const deadlines = [];
  const requests = [];
  t.mock.method(AbortSignal, 'timeout', (milliseconds) => {
    const controller = new AbortController();
    deadlines.push({ milliseconds, controller });
    return controller.signal;
  });
  t.mock.method(globalThis, 'fetch', async (url, init) => {
    requests.push({ url, init });
    return {
      status: 200,
      headers: new Map(),
      body: new ReadableStream({
        start(controller) {
          init.signal.addEventListener('abort', () => controller.error(init.signal.reason), { once: true });
        },
      }),
    };
  });
  const transport = workersTransport();
  await transport.request({ method: 'GET', url: 'https://venue.test/default', headers: {} });
  const result = await transport.request({
    method: 'GET', url: 'https://venue.test/time', headers: {}, route: 'direct', timeoutMs: 5_000,
  });
  assert.deepEqual(deadlines.map(({ milliseconds }) => milliseconds), [20_000, 5_000]);
  assert.equal(requests[1].url, 'https://venue.test/time');
  for (const { init } of requests) {
    assert.equal(Object.hasOwn(init, 'route'), false);
    assert.equal(Object.hasOwn(init, 'timeoutMs'), false);
  }
  const read = result.body[Symbol.asyncIterator]().next();
  const rejected = assert.rejects(read, /clock deadline/);
  deadlines[1].controller.abort(new Error('clock deadline'));
  await rejected;
  assert.equal(requests[1].init.signal.aborted, true);
  deadlines[0].controller.abort();
});

test('standalone Workers reject unconfigured CORS before fetching or allocating a deadline', async (t) => {
  const fetch = t.mock.method(globalThis, 'fetch');
  const timeout = t.mock.method(AbortSignal, 'timeout');
  await assert.rejects(workersTransport().request({
    method: 'GET', url: 'https://venue.test/time', headers: {}, route: 'publicCors', timeoutMs: 5_000,
  }), /Public CORS request routing is not configured/);
  assert.equal(fetch.mock.callCount(), 0);
  assert.equal(timeout.mock.callCount(), 0);
});

test('Workers upgrades fail closed without deployment auth and cannot select another account token', async () => {
  const request = (token) => ({ headers: new Map([['Upgrade', 'websocket'], ['Authorization', `Bearer ${token}`]]) });
  for (const accountToken of [undefined, () => '', () => { throw Error('secret'); }, () => 'account-b-token']) {
    const Object = createEngineObject({ loadEngine, accountToken });
    const object = new Object({ storage: { setAlarm() {} } });
    assert.equal((await object.fetch(request('account-a-token'))).status, 401);
    assert.equal(object.clients.size, 0);
  }
  const Object = createEngineObject({ loadEngine, accountToken: (_state, env) => env.token });
  const object = new Object({ storage: { setAlarm() {} } }, { token: 'account-a-token' });
  const response = await object.fetch(request('account-a-token'));
  assert.equal(response.status, 101);
  response.webSocket.close();
});

test('Workers permits one pending start and rejects reconfiguration of a running account', async () => {
  let finish;
  let constructed = 0;
  const messages = [];
  const Object = createEngineObject({
    loadEngine: () => new Promise((resolve) => { finish = resolve; }),
    accountToken: () => 'fixture-token',
  });
  const object = new Object({ storage: { setAlarm() {} } });
  const client = { send: (text) => messages.push(JSON.parse(text)) };
  object.clients.add(client);
  const start = JSON.stringify({ op: 'start', venue: 'binance', options: { key: 'first-account' } });
  const pending = object.receive(client, start);
  await object.receive(client, JSON.stringify({ op: 'start', venue: 'kraken', options: { key: 'other-account' } }));
  finish({ WasmEngine: class { constructor() { constructed++; } start() { return ''; } } });
  await pending;
  await object.receive(client, start);
  assert.equal(constructed, 1);
  assert.equal(messages.length, 2);
  assert.ok(messages.every((message) => /already started or starting/.test(message.message)));
  object.leave(client);
});

test('Workers late start resolution never revives a disconnected client or a superseded lifetime', async () => {
  const loads = [];
  let constructed = 0;
  const Object = createEngineObject({ accountToken: () => 'fixture-token', loadEngine: () => new Promise((resolve) => loads.push(resolve)) });
  const object = new Object({ storage: { setAlarm() {} } });
  const first = { send: assert.fail };
  object.clients.add(first);
  const old = object.receive(first, '{"op":"start","venue":"binance"}');
  object.leave(first);
  const next = { send: assert.fail };
  object.clients.add(next);
  const current = object.receive(next, '{"op":"start","venue":"binance"}');
  const module = { WasmEngine: class { constructor() { constructed++; } start() { return ''; } } };
  loads[0](module);
  await old;
  assert.equal(constructed, 0);
  assert.equal(object.host, undefined);
  loads[1](module);
  await current;
  assert.equal(constructed, 1);
  object.leave(next);
});

test('Workers ignores foreign clients, malformed JSON and null; failed starts do not echo options', async () => {
  const messages = [];
  let loads = 0;
  const Object = createEngineObject({ accountToken: () => 'fixture-token', loadEngine: async () => {
    loads++;
    throw Error('private-secret');
  } });
  const object = new Object({ storage: { setAlarm() {} } });
  const client = { send: (text) => messages.push(text) };
  await object.receive(client, '{"op":"start","venue":"binance"}');
  assert.equal(loads, 0);
  object.clients.add(client);
  for (const message of ['{', 'null', '[]', '42', '{"op":"start","venue":"binance","options":"private-secret"}',
    '{"op":"start","venue":"binance","options":{"secret":"private-secret"}}']) await object.receive(client, message);
  assert.equal(loads, 1);
  assert.equal(messages.length, 3);
  assert.ok(messages.every((text) => !text.includes('private-secret')));
  object.leave(client);
});

test('Workers discards command completions after client departure or host replacement', async () => {
  for (const reject of [false, true]) for (const replace of [false, true]) {
    let complete;
    const Object = createEngineObject({ accountToken: () => 'fixture-token', loadEngine });
    const object = new Object({ storage: { setAlarm() {} } });
    const client = { send: assert.fail };
    const other = { send() {} };
    object.clients.add(client);
    object.clients.add(other);
    object.host = { command: () => new Promise((resolve, fail) => { complete = reject ? fail : resolve; }), dispose() {} };
    const pending = object.receive(client, '{"op":"command","id":1,"command":{}}');
    if (replace) object.host = { dispose() {} };
    else object.leave(client);
    complete('private result');
    await pending;
    object.leave(client);
    object.leave(other);
  }
});

test('Workers HTTP requests and upgrade requests forbid redirects with private headers', async (t) => {
  const requests = [];
  t.mock.method(globalThis, 'fetch', async (_url, init) => {
    requests.push(init);
    if (init.headers.Upgrade) return { webSocket: new FakeSocket() };
    return { status: 200, headers: new Map(), body: null };
  });
  const socket = new WorkersWebSocket('wss://venue.test/private?token=sensitive', [], { headers: { 'X-Api-Key': 'sensitive' } });
  await workersTransport().request({ method: 'GET', url: 'https://venue.test/private', headers: { 'X-Api-Key': 'sensitive' } });
  assert.deepEqual(requests.map((init) => init.redirect), ['error', 'error']);
  socket.close();
});
