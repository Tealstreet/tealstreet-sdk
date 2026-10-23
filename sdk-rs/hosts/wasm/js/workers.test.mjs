// The Workers host against fake Workers primitives (WebSocketPair, upgrade
// fetch) and the real Node build of the engine. Run after `yarn rust:build-wasm`.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
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

test('the Durable Object sends accountAuth evidence only to clients that opted in', async () => {
  const EngineObject = createEngineObject({ accountToken: () => 'account-a-token', loadEngine, flush: { store: 20, books: 20 } });
  const object = new EngineObject({ storage: { setAlarm: async () => {} } });
  const connect = async () => {
    const client = (await object.fetch({ headers: new Map([['Upgrade', 'websocket'], ['Authorization', 'Bearer account-a-token']]) })).webSocket;
    const received = [];
    client.addEventListener('message', (event) => received.push(event.data));
    return { client, received };
  };
  const optedIn = await connect();
  const legacy = await connect();
  optedIn.client.send(JSON.stringify({ op: 'accountAuth' }));
  optedIn.client.send(JSON.stringify({ op: 'start', venue: 'binance', options: { publicDataOnly: true } }));
  await until(() => object.host);
  object.host.handlers.onAccountAuth({ credential: 'primary', scope: 'orders', outcome: 'rejected' });
  const auth = (received) => received.filter((m) => typeof m === 'string' && m.includes('accountAuth'));
  assert.deepEqual(auth(optedIn.received).map((m) => JSON.parse(m)), [
    { op: 'accountAuth', credential: 'primary', scope: 'orders', outcome: 'rejected' },
  ]);
  assert.deepEqual(auth(legacy.received), []);
  optedIn.client.close();
  legacy.client.close();
});

test('Workers derives private upgrade capability from its transport', async () => {
  let options;
  const loaded = [];
  const EngineObject = createEngineObject({ accountToken: () => 'account-a-token', loadEngine: async (venue) => (loaded.push(venue), { WasmEngine: class {
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
    assert.deepEqual(loaded, ['weex'], 'the loader receives the venue to select its family');
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
  t.mock.method(Date, 'now', () => 1_700_000_000_000);
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

test('Durable Object notices reach clients, and Warn+ reaches the console with none attached', (t) => {
  const EngineObject = createEngineObject({ loadEngine });
  const object = new EngineObject({ storage: { setAlarm() {} } });
  const warned = t.mock.method(console, 'warn', () => {});
  object.notify(false, { op: 'event', level: 'info', message: 'quiet' });
  object.notify(true, { op: 'event', level: 'warn', code: 'ws.reconnect', message: 'reconnecting' });
  assert.deepEqual(warned.mock.calls.map((call) => call.arguments), [['safe-cex warn ws.reconnect: reconnecting']]);
  const sent = [];
  object.clients.add({ send: (text) => sent.push(JSON.parse(text)) });
  object.notify(true, { op: 'event', level: 'error', code: 'auth.rejected', message: 'rejected', fields: {} });
  assert.deepEqual(sent, [{ op: 'event', level: 'error', code: 'auth.rejected', message: 'rejected', fields: {} }]);
  assert.equal(warned.mock.callCount(), 1);
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


test('Workers retain actionable venue command errors for connected clients', async () => {
  const messages = [];
  const Object = createEngineObject({ accountToken: () => 'fixture-token', loadEngine });
  const object = new Object({ storage: { setAlarm() {} } });
  const client = { send: (text) => messages.push(JSON.parse(text)) };
  object.clients.add(client);
  object.host = { command: async () => { throw new Error('Insufficient balance for this order'); }, dispose() {} };
  await object.receive(client, '{"op":"command","id":22,"command":{}}');
  assert.deepEqual(messages, [{ op: 'done', id: 22, error: 'Insufficient balance for this order' }]);
  object.leave(client);
});

test('Workers rejects provider controls from authenticated sockets while preserving ordinary commands', async () => {
  const EngineObject = createEngineObject({ loadEngine, accountToken: () => 'account-a-token' });
  const object = new EngineObject({ storage: { setAlarm() {} } });
  const response = await object.fetch({ headers: new Map([
    ['Upgrade', 'websocket'], ['Authorization', 'Bearer account-a-token'],
  ]) });
  assert.equal(response.status, 101);
  const client = response.webSocket;
  const messages = [];
  const delivered = [];
  const state = { bid: 50_000 };
  object.host = {
    async command(command) {
      delivered.push(command);
      if (command.op === 'publicFeedUpdate') state.bid = command.tickers[0].bid;
      return { bid: state.bid };
    },
    dispose() {},
  };
  client.addEventListener('message', (event) => messages.push(JSON.parse(event.data)));
  try {
    client.send(JSON.stringify({ op: 'command', id: 31,
      command: { op: 'publicFeedUpdate', tickers: [{ symbol: 'BTCUSDT', bid: 1 }] },
    }));
    await until(() => messages.some((message) => message.id === 31));
    assert.deepEqual(messages[0], { op: 'done', id: 31, error: 'public feed updates require a trusted host provider' });
    assert.deepEqual(delivered, []);
    assert.equal(state.bid, 50_000);
    client.send(JSON.stringify({ op: 'command', id: 32, command: { op: 'fetchAccount' } }));
    await until(() => messages.some((message) => message.id === 32));
    assert.deepEqual(messages[1], { op: 'done', id: 32, result: { bid: 50_000 } });
    assert.deepEqual(delivered, [{ op: 'fetchAccount' }]);
  } finally { client.close(); }
});

test('Workers forgets a startup failure once the engine stops or restarts', async () => {
  let starts = 0;
  const WasmEngine = class {
    start() {
      starts++;
      return starts === 1 ? JSON.stringify([{ op: 'startupFailed', detail: { message: 'key rejected' } }]) : '';
    }
    snapshotFrame() { return Uint8Array.of(1); }
    free() {}
  };
  const EngineObject = createEngineObject({
    accountToken: () => 'account-a-token',
    loadEngine: async () => ({ WasmEngine }),
    flush: { store: 1e9, books: 1e9 },
  });
  const object = new EngineObject({ storage: { setAlarm: async () => {} } });
  const join = async () => {
    const response = await object.fetch({ headers: new Map([['Upgrade', 'websocket'], ['Authorization', 'Bearer account-a-token']]) });
    // The server side records what it sent, including replays sent while joining.
    const server = [...object.clients].at(-1);
    return { socket: response.webSocket, received: server.sent };
  };
  const failed = (received) => received.some((m) => typeof m === 'string' && m.includes('"startupFailed"'));
  const first = await join();
  first.socket.send(JSON.stringify({ op: 'start', venue: 'binance' }));
  await until(() => failed(first.received));
  first.socket.close();
  const second = await join();
  assert.ok(!failed(second.received), 'a stopped engine no longer reports its failed start');
  second.socket.send(JSON.stringify({ op: 'start', venue: 'binance' }));
  await until(() => object.host !== undefined);
  const third = await join();
  assert.ok(!failed(second.received) && !failed(third.received), 'a successful restart reports no failure');
  third.socket.close();
  second.socket.close();
});


async function eventObject(synchronous = false, emitEvents = true) {
  const EngineObject = createEngineObject({
    accountToken: () => 'fixture-token',
    flush: { store: 1e9, books: 1e9 },
    loadEngine: async () => ({ WasmEngine: class {
      start() { return '[]'; }
      command(json) {
        if (!emitEvents) return '[]';
        const { command } = JSON.parse(json);
        const event = { op: 'event', level: 'warn', code: 'history.incomplete', message: `partial-${command}`, fields: { command } };
        return JSON.stringify(synchronous ? [event, { op: 'done', command, result: '[]' }] : [event]);
      }
      free() {}
    } }),
  });
  const object = new EngineObject({ storage: { setAlarm() {} } });
  const messages = [[], []];
  const clients = messages.map((messages) => ({ send(text) { messages.push(JSON.parse(text)); } }));
  for (const client of clients) object.clients.add(client);
  await object.receive(clients[0], '{"op":"start","venue":"fixture"}');
  return { object, clients, messages };
}

const historyNotice = (command) => ({
  op: 'event', level: 'warn', code: 'history.incomplete', message: 'private coverage', fields: { command },
});

test('Workers routes the native WEEX incomplete fixture only to its translated owner', async () => {
  const event = JSON.parse(readFileSync(new URL('../../../venues/weex/tests/fixtures/incomplete-query-event.json', import.meta.url), 'utf8'));
  const { object, clients, messages } = await eventObject(false, false);
  try {
    const first = object.receive(clients[0], '{"op":"command","id":77,"command":{"op":"fetchIncome"}}');
    const second = object.receive(clients[1], '{"op":"command","id":77,"command":{"op":"fetchIncome"}}');
    assert.deepEqual(messages, [[], []], 'only the actual native fixture supplies a marker');
    object.host.run(JSON.stringify([
      event,
      { op: 'done', command: 1, result: '[{"tranId":"kept"}]' },
      { op: 'done', command: 2, result: '[]' },
    ]));
    await Promise.all([first, second]);
    assert.deepEqual(messages[0].map((row) => row.op), ['event', 'done']);
    assert.deepEqual(messages[0][0], { ...event, fields: { command: 77 } });
    assert.equal(messages[0][1].id, 77);
    assert.deepEqual(messages[0][1].result, [{ tranId: 'kept' }]);
    assert.equal(messages[0][1].error, undefined);
    assert.deepEqual(messages[1].map((row) => row.op), ['done']);
    assert.equal(messages[1][0].id, 77);
  } finally { for (const client of clients) object.leave(client); }
});

test('Workers translates overlapping external IDs and delivers each marker before its own done', async () => {
  const { object, clients, messages } = await eventObject();
  try {
    const first = object.receive(clients[0], '{"op":"command","id":77,"command":{"op":"fetchFills"}}');
    const second = object.receive(clients[1], '{"op":"command","id":77,"command":{"op":"fetchFills"}}');
    assert.equal(messages[0].length, 1, 'synchronous marker reaches only its owner');
    assert.equal(messages[1].length, 1);
    assert.equal(messages[0][0].fields.command, 77);
    assert.equal(messages[1][0].fields.command, 77);
    assert.equal(messages[0][0].message, 'partial-1');
    assert.equal(messages[1][0].message, 'partial-2');
    object.host.run(JSON.stringify([
      { op: 'done', command: 2, result: '[]' },
      { op: 'done', command: 1, result: '[]' },
    ]));
    await Promise.all([first, second]);
    for (const rows of messages) assert.deepEqual(rows.map((row) => row.op), ['event', 'done']);
    assert.equal(messages[0][1].id, 77);
    assert.equal(messages[1][1].id, 77);
    object.host.run(JSON.stringify([historyNotice(1), historyNotice(2), historyNotice(999)]));
    assert.deepEqual(messages.map((rows) => rows.length), [2, 2], 'retired or unknown IDs never broadcast');
  } finally { for (const client of clients) object.leave(client); }
});

test('Workers correlates a marker and done emitted in one synchronous native dispatch', async () => {
  const { object, clients, messages } = await eventObject(true);
  try {
    await object.receive(clients[1], '{"op":"command","id":92,"command":{"op":"fetchFills"}}');
    assert.deepEqual(messages[1].map((row) => row.op), ['event', 'done']);
    assert.equal(messages[1][0].fields.command, 92);
    assert.equal(messages[1][1].id, 92);
    assert.equal(messages[0].length, 0);
  } finally { for (const client of clients) object.leave(client); }
});

test('Workers suppresses departed owners, replaced hosts and malformed command event IDs', async () => {
  for (const replace of [false, true]) {
    const { object, clients, messages } = await eventObject();
    const host = object.host;
    try {
      const pending = object.receive(clients[0], '{"op":"command","id":77,"command":{"op":"fetchFills"}}');
      messages[0].length = 0;
      if (replace) object.host = { dispose() {} };
      else object.leave(clients[0]);
      host.run(JSON.stringify([
        historyNotice(1), historyNotice(999),
        { ...historyNotice(999), code: 'row.skipped' },
        ...['1', null, true, 1.5, -1, 0x1_0000_0000].map(historyNotice),
        { op: 'event', level: 'warn', code: 'history.incomplete', message: 'owner absent', fields: {} },
        { op: 'done', command: 1, result: '[]' },
      ]));
      await pending;
      assert.deepEqual(messages, [[], []]);
      // Background notices retain their existing broadcast contract.
      if (!replace) {
        host.run(JSON.stringify([{ op: 'event', level: 'warn', code: 'row.skipped', message: 'background', fields: {} }]));
        assert.equal(messages[1][0].code, 'row.skipped');
      }
    } finally {
      host.dispose();
      for (const client of clients) object.leave(client);
    }
  }
});

test('Workers routes the native WEEX empty-fill fixture before retained rows only to its translated owner', async () => {
  const event = JSON.parse(readFileSync(new URL('../../../venues/weex/tests/fixtures/empty-fill-incomplete-query-event.json', import.meta.url), 'utf8'));
  const { object, clients, messages } = await eventObject(false, false);
  try {
    const first = object.receive(clients[0], '{"op":"command","id":77,"command":{"op":"fetchFills"}}');
    const second = object.receive(clients[1], '{"op":"command","id":77,"command":{"op":"fetchFills"}}');
    object.host.run(JSON.stringify([event, { op: 'done', command: 1, result: '[{"id":"good","price":69000,"qty":0.01}]' }, { op: 'done', command: 2, result: '[]' }]));
    await Promise.all([first, second]);
    assert.deepEqual(messages[0].map((row) => row.op), ['event', 'done']);
    assert.deepEqual(messages[0][0], { ...event, fields: { command: 77 } });
    assert.deepEqual(messages[0][1], { op: 'done', id: 77, result: [{ id: 'good', price: 69000, qty: 0.01 }] });
    assert.deepEqual(messages[1], [{ op: 'done', id: 77, result: [] }]);
  } finally { for (const client of clients) object.leave(client); }
});

test('Workers obtains opaque signer refs and provider from deployment setup only', async () => {
  const identities = { primary: { credential: 'account/key', signing: 'account/sign' } };
  const calls = [];
  const EngineObject = createEngineObject({
    accountToken: () => 'fixture-token',
    flush: { store: 1e9, books: 1e9 },
    createSigner(state, env, venue) {
      assert.equal(venue, 'binance');
      assert.equal(env, environment);
      calls.push('provision');
      return { identities, signer: (request) => ({ values: [[1]], receipt: request.receipt }) };
    },
    loadEngine: async () => ({ WasmEngine: class {
      constructor(venue, options) { calls.push(JSON.parse(options)); }
      requiresExternalSigner() { return true; }
      useExternalSigner() { calls.push('installed'); }
      start() { return JSON.stringify([{ op: 'sign', attempt: 7, timeoutMs: 1000, work: {
        operations: [{ kind: 'present', key: 'account/key' }], context: {}, receipt: { expiryMs: 2000 },
      } }]); }
      signed(id, result) { calls.push({ signed: id, result: JSON.parse(result) }); return '[]'; }
      free() {}
    } }),
  });
  const environment = {};
  const object = new EngineObject({ storage: { setAlarm() {} } }, environment);
  const messages = [];
  const client = { send(message) { messages.push(message); } };
  object.clients.add(client);
  await object.receive(client, JSON.stringify({ op: 'start', venue: 'binance', options: {
    signing: { primary: { credential: 'untrusted', signing: 'untrusted' } },
  } }));
  try {
    assert.ok(object.host, messages.join('\n'));
    assert.equal(calls[0], 'provision');
    assert.deepEqual(calls[1].signing, identities);
    assert.equal(calls[2], 'installed');
    await until(() => calls.some((call) => call?.signed === 7));
    assert.deepEqual(calls.find((call) => call?.signed === 7), {
      signed: 7, result: { values: [[1]], receipt: { expiryMs: 2000 } },
    });
  } finally { object.leave(client); }
});

test('Workers releases managed local signer allocation after failed host construction', async () => {
  let frees = 0;
  const Object = createEngineObject({
    accountToken: () => 'fixture-token',
    loadEngine: async () => ({ WasmEngine: class {
      requiresExternalSigner() { return true; }
      free() { frees++; }
    } }),
  });
  const object = new Object({ storage: { setAlarm() {} } });
  const messages = [];
  const client = { send(message) { messages.push(message); } };
  object.clients.add(client);
  await object.receive(client, '{"op":"start","venue":"binance"}');
  assert.equal(frees, 1);
  assert.equal(object.host, undefined);
  assert.ok(messages.some((message) => message.includes('Engine start failed')));
  object.leave(client);
  assert.equal(frees, 1);
});

// Real EngineHost ownership with a deterministic native engine double. No WASM
// build or exhaustion loop is needed to reach the sticky terminal state.
function terminalWorkers(t, dieAt) {
  const intervals = new Map();
  let next = 0;
  t.mock.method(globalThis, 'setInterval', (callback) => { intervals.set(++next, callback); return next; });
  t.mock.method(globalThis, 'clearInterval', (id) => intervals.delete(id));
  const engines = [];
  class WasmEngine {
    constructor() { this.fatal = false; this.freed = 0; this.frames = 0; engines.push(this); }
    failed() { return this.fatal; }
    start() { if (dieAt === 'start') this.fatal = true; return '[]'; }
    command(json) { if (JSON.parse(json).op === 'placeOrders') this.fatal = true; return '[]'; }
    takeFrame() { this.frames++; if (dieAt === 'flush') this.fatal = true; return undefined; }
    snapshotFrame() { if (dieAt === 'snapshot') this.fatal = true; return new Uint8Array(8); }
    free() { this.freed++; }
  }
  const EngineObject = createEngineObject({
    accountToken: () => 'terminal-fixture', loadEngine: async () => ({ WasmEngine }),
    createTransport: () => ({ now: () => 0, setTimeout, clearTimeout }),
    flush: { store: 250, books: 60 },
  });
  const object = new EngineObject({ storage: { setAlarm() {} } });
  const messages = [[], []];
  const clients = messages.map((received) => ({ send(text) { received.push(JSON.parse(text)); } }));
  for (const client of clients) object.clients.add(client);
  return { object, clients, messages, engines, intervals,
    start: () => object.receive(clients[0], '{"op":"start","venue":"fixture"}') };
}

for (const phase of ['start', 'flush']) {
  test(`Workers terminal ${phase} retires its host and intervals before notifying clients`, async (t) => {
    const r = terminalWorkers(t, phase);
    try {
      await r.start();
      if (phase === 'flush') [...r.intervals.values()][0]();
      assert.equal(r.object.host, undefined);
      assert.equal(r.intervals.size, 0);
      assert.equal(r.engines[0].freed, 1);
      for (const received of r.messages) {
        assert.equal(received.length, 1);
        assert.deepEqual(received[0], { op: 'startupFailed', detail: {
          message: 'Exchange engine stopped unexpectedly; recreate the engine', kind: 'internal', outcome: 'unknown',
        } });
      }
      const failure = r.object.startupFailure;
      assert.equal(typeof failure, 'string');
    } finally { for (const client of r.clients) r.object.leave(client); }
  });
}

test('Workers terminal commands keep their original owners after host replacement', async (t) => {
  const r = terminalWorkers(t);
  try {
    await r.start();
    const old = r.object.host;
    const first = r.object.receive(r.clients[0], '{"op":"command","id":77,"command":{"op":"fetchFills"}}');
    const second = r.object.receive(r.clients[1], '{"op":"command","id":77,"command":{"op":"fetchFills"}}');
    const fatal = r.object.receive(r.clients[0], '{"op":"command","id":88,"command":{"op":"placeOrders","orders":[]}}');
    assert.equal(r.object.host, undefined);
    const replacement = { dispose() {}, flush() { throw new Error('old interval reached replacement'); } };
    r.object.host = replacement;
    await Promise.all([first, second, fatal]);
    assert.equal(r.object.host, replacement);
    assert.equal(old.dead, true);
    for (const [index, received] of r.messages.entries()) {
      const done = received.filter((message) => message.op === 'done');
      assert.deepEqual(done.map((message) => message.id).sort(), index === 0 ? [77, 88] : [77]);
      assert.ok(done.every((message) => message.error.includes('engine stopped')));
      if (index === 0) assert.match(done.find((message) => message.id === 88).error,
        /outcome is unknown.*Reconcile before retrying/);
      assert.equal(received.filter((message) => message.op === 'startupFailed').length, 1);
    }
  } finally { for (const client of r.clients) r.object.leave(client); }
});

test('Workers queued old intervals and late death cannot touch a replacement', async (t) => {
  const r = terminalWorkers(t, 'flush');
  try {
    await r.start();
    const old = r.object.host;
    const oldTicks = [...r.intervals.values()];
    oldTicks[0]();
    assert.equal(r.object.host, undefined);
    let flushes = 0;
    const replacement = { dispose() {}, flush() { flushes++; } };
    r.object.host = replacement;
    r.object.startupFailure = 'replacement state';
    old.handlers.onDead(old.deadError);
    for (const tick of oldTicks) tick();
    assert.equal(r.object.host, replacement);
    assert.equal(r.object.startupFailure, 'replacement state');
    assert.equal(flushes, 0);
    assert.equal(r.engines[0].freed, 1);
  } finally { for (const client of r.clients) r.object.leave(client); }
});

test('Workers join snapshot death returns one terminal notice without leaking the accepted client', async (t) => {
  const r = terminalWorkers(t, 'snapshot');
  try {
    await r.start();
    const response = await r.object.fetch({ headers: new Map([['Upgrade', 'websocket'], ['Authorization', 'Bearer terminal-fixture']]) });
    assert.equal(response.status, 101);
    assert.equal(r.object.host, undefined);
    const joined = [...r.object.clients].at(-1);
    assert.deepEqual(joined.sent.map((text) => JSON.parse(text).op), ['startupFailed']);
    response.webSocket.close();
    assert.equal(r.object.clients.has(joined), false);
    assert.equal(r.engines[0].freed, 1);
    assert.equal(r.intervals.size, 0);
  } finally { for (const client of [...r.object.clients]) r.object.leave(client); }
});


test('Workers terminal errors stay silent after their original client departs', async (t) => {
  const r = terminalWorkers(t);
  try {
    await r.start();
    const pending = r.object.receive(r.clients[0], '{"op":"command","id":77,"command":{"op":"fetchFills"}}');
    const fatal = r.object.receive(r.clients[1], '{"op":"command","id":88,"command":{"op":"placeOrders","orders":[]}}');
    r.object.leave(r.clients[0]);
    await Promise.all([pending, fatal]);
    assert.equal(r.messages[0].filter((message) => message.op === 'done').length, 0);
    const done = r.messages[1].filter((message) => message.op === 'done');
    assert.deepEqual(done.map((message) => message.id), [88]);
    assert.match(done[0].error, /outcome is unknown.*Reconcile before retrying/);
  } finally { for (const client of [...r.object.clients]) r.object.leave(client); }
});
