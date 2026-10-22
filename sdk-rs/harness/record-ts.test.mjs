import assert from 'node:assert/strict';
import test from 'node:test';
import { EventEmitter } from 'node:events';
import { createRequire } from 'node:module';
import { spawnSync } from 'node:child_process';
import * as fs from 'node:fs';
import { mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { atomicTsTape, createTsCapture, recordTsPublic, validateTsRecording } from './record-ts-runtime.mjs';
import { venueOf } from './venues.mjs';

const require = createRequire(new URL('../../packages/safe-cex/package.json', import.meta.url));
const axios = require('axios').default;
const flush = () => new Promise(setImmediate);
const response = (config, data = '{"id":9007199254740993}') => ({ config, data: typeof data === 'string' ? Buffer.from(data) : data, status: 200,
  headers: { 'content-type': 'application/json', 'set-cookie': 'discard-this-cookie' }, statusText: 'OK' });
const fixture = (adapter, extra = {}) => {
  const records = [], http = adapter ?? (async (config) => response(config));
  const client = axios.create({ adapter: http, baseURL: 'https://public.example', transformResponse: [(text) => ({ wire: text })], ...extra });
  // Only tests substitute the standard HTTP adapter. Production uses Axios's
  // actual getAdapter('http'), and refuses arbitrary adapters at installation.
  const resolver = { getAdapter(value) { if (value === http || value === 'http') return http; return axios.getAdapter(value); } };
  const capture = createTsCapture({ axios: resolver, now: () => 123, write: (record) => records.push(record) });
  return { client, capture, records };
};
class NodeSocket extends EventEmitter {
  static instances = [];
  constructor(url) { super(); this.url = url; this.sent = []; NodeSocket.instances.push(this); queueMicrotask(() => this.emit('open')); }
  send(data, ...args) { this.sent.push(data); args.find((arg) => typeof arg === 'function')?.(); }
  close() { queueMicrotask(() => this.emit('close', 1000)); }
}
class BrowserSocket extends EventTarget {
  constructor(url) { super(); this.url = url; this.sent = []; queueMicrotask(() => this.dispatchEvent(new Event('open'))); }
  send(data) { this.sent.push(data); }
  close() { const event = new Event('close'); event.code = 1000; queueMicrotask(() => this.dispatchEvent(event)); }
}

test('public configuration and catalog checks reject unsupported scope before loading', async () => {
  const valid = { name: 'binance', books: ['BTCUSDT'], seconds: 1, env: {} };
  validateTsRecording(valid);
  for (const options of [{ key: 'supplied' }, { applicationId: 'account' }, { vault: 'account' }, { remoteSigner: {} },
    { externalPublicData: true }, { transport: {} }, { endpoints: {} }, { testnet: 'yes' }])
    assert.throws(() => validateTsRecording({ ...valid, options }));
  for (const change of [{ name: 'bybit' }, { books: ['BTCUSDT', 'BTCUSDT'] }, { seconds: Infinity },
    { env: { HTTPS_PROXY: 'https://proxy.example' } }, { venue: { ...venueOf('binance'), ts: { module: '__tests__/reference.ts' } } }])
    assert.throws(() => validateTsRecording({ ...valid, ...change }));
  let loaded = false;
  await assert.rejects(recordTsPublic({ name: 'orderly', books: ['BTCUSDT'], seconds: 1, env: {},
    dependencies: { loadTransports() { loaded = true; } } }));
  assert.equal(loaded, false);
});

test('Axios delegates serialization/transforms and captures raw large IDs before parsing', async () => {
  let dispatched;
  const { client, capture, records } = fixture(async (config) => { dispatched = config; return response(config); },
    { paramsSerializer: { serialize: () => 'symbol=BTC%2FUSDT&symbol=ETH%2FUSDT' } });
  const original = client.defaults.adapter; capture.wrapAxios(client);
  const result = await client.get('/ticker', { params: { symbol: ['BTC/USDT', 'ETH/USDT'] } });
  await capture.drain();
  assert.equal(result.data.wire, '{"id":9007199254740993}');
  assert.equal(records[0].body, result.data.wire);
  assert.equal(records[0].url, 'https://public.example/ticker?symbol=BTC%2FUSDT&symbol=ETH%2FUSDT');
  assert.equal(dispatched.proxy, false); assert.equal(dispatched.maxContentLength, 8 * 1024 * 1024);
  assert.equal(dispatched.responseType, 'arraybuffer'); assert.equal(result.config.responseType, undefined);
  capture.restore(); assert.equal(client.defaults.adapter, original);
});

test('Axios byte capture preserves BOM while restoring original caller text/buffer semantics', async () => {
  for (const [options, bom] of [[{}, false], [{ responseEncoding: 'utf-8' }, true], [{ responseType: 'arraybuffer' }, true]]) {
    const bytes = Buffer.from('\uFEFF{"id":9007199254740993}');
    const { client, capture, records } = fixture(async (config) => response(config, bytes), options);
    capture.wrapAxios(client); const result = await client.get('/ticker'); await capture.drain();
    assert.equal(records[0].body, bytes.toString('utf8'));
    if (options.responseType) assert.deepEqual(result.data.wire, bytes);
    else assert.equal(result.data.wire.startsWith('\uFEFF'), bom);
    capture.restore();
  }
  const { client, capture } = fixture(async (config) => response(config, new Uint8Array([255])));
  capture.wrapAxios(client); await assert.rejects(client.get('/ticker'), /encoded data/);
  await assert.rejects(capture.drain()); capture.restore();
});

test('HTTP dispatch ordinals survive reversed response completion', async () => {
  const finish = new Map();
  const { client, capture, records } = fixture((config) => new Promise((resolve) => finish.set(config.url, () => resolve(response(config)))));
  capture.wrapAxios(client); const first = client.get('/first'), second = client.get('/second');
  await flush(); finish.get('/second')(); await second; finish.get('/first')(); await first; await capture.drain();
  assert.deepEqual(records.map((r) => r.requestOrdinal), [1, 0]); capture.restore();
});

test('recorded HTTP error replies retain raw bytes and permit normal retry semantics', async () => {
  let count = 0;
  const { client, capture, records } = fixture(async (config) => {
    if (count++) return response(config);
    throw Object.assign(new Error('HTTP rejection'), { response: { ...response(config, '{"code":429}'), status: 429 } });
  });
  capture.wrapAxios(client); await assert.rejects(client.get('/ticker'), /HTTP rejection/);
  await client.get('/ticker'); await capture.drain();
  assert.deepEqual(records.map((r) => r.status), [429, 200]); capture.restore();
});

for (const [label, config] of [['adapter', { adapter: async () => { throw new Error('bypassed'); } }], ['stream', { responseType: 'stream' }],
  ['authenticated', { headers: { Authorization: 'supplied' } }], ['write', { method: 'post', data: '{}' }]]) {
  test(`unsupported Axios ${label} fails before adapter dispatch`, async () => {
    let calls = 0; const { client, capture } = fixture(async (c) => { calls++; return response(c); });
    capture.wrapAxios(client); await assert.rejects(client.request({ url: '/ticker', ...config }));
    assert.equal(calls, 0); await assert.rejects(capture.drain()); capture.restore();
  });
}
test('custom and Axios-fetch adapters are explicitly unsupported', () => {
  const { capture } = fixture();
  assert.throws(() => capture.wrapAxios(axios.create({ adapter: async () => {} })), /standard HTTP/);
  assert.throws(() => capture.wrapAxios(axios.create({ adapter: 'fetch' })), /standard HTTP/);
  capture.restore();
});

test('actual production Binance constructor has no I/O and owns only standard HTTP adapters', () => {
  const code = `
    import {createRequire, syncBuiltinESMExports} from 'node:module';
    import http from 'node:http'; import https from 'node:https';
    import assert from 'node:assert/strict';
    import {axiosClients, venueOf} from './sdk-rs/harness/venues.mjs';
    import {createTsCapture} from './sdk-rs/harness/record-ts-runtime.mjs';
    let attempts=0;const refuse=()=>{attempts++;throw Error('forbidden test network')};
    http.request=refuse;https.request=refuse;globalThis.fetch=refuse;globalThis.WebSocket=class{constructor(){refuse()}};
    syncBuiltinESMExports();process.on('unhandledRejection',()=>{attempts++});
    const venue=venueOf('binance');
    const [{BinanceExchange},{DefaultStore}]=await Promise.all([import(new URL(venue.ts.module,new URL('./sdk-rs/harness/venues.mjs',import.meta.url))),import('./packages/safe-cex/src/store/store.base.ts')]);
    const require=createRequire(new URL('./packages/safe-cex/package.json',import.meta.url));
    const capture=createTsCapture({axios:require('axios').default,write(){}});
    const exchange=new BinanceExchange('binance',{publicDataOnly:true,tealstreetTunnel:false,tealstreetProxy:false},new DefaultStore());
    const clients=axiosClients(exchange);assert(clients.length>=3);
    for(const client of clients)capture.wrapAxios(client);
    capture.restore();exchange.dispose();await new Promise(setImmediate);assert.equal(attempts,0);
    console.log('PRODUCTION_CONSTRUCTOR_OFFLINE');process.exit(0);
  `;
  const result = spawnSync(process.execPath, ['--import', 'tsx', '--input-type=module', '-e', code], { encoding: 'utf8', timeout: 30000 });
  assert.equal(result.status, 0, result.stdout + result.stderr); assert.match(result.stdout, /PRODUCTION_CONSTRUCTOR_OFFLINE/);
});

test('Node sockets retain startup sends, callbacks, text Buffers, binary and reconnect ordinals', async () => {
  const { capture, records } = fixture(); const Socket = capture.wrapWebSocket(NodeSocket);
  assert.equal(Socket.WebSocket, Socket);
  const socket = new Socket('wss://public.example/ws'); await flush();
  let called = false;
  socket.send(Buffer.from('ping'), { binary: false }, () => { called = true; });
  socket.emit('message', Buffer.from('text-buffer'), false); socket.emit('message', Buffer.from([1, 2]), true);
  const second = new Socket('wss://public.example/ws'); await flush();
  second.send('{"method":"SUBSCRIBE","params":["book"],"id":1}'); await capture.closeSockets(); await capture.drain();
  assert(called); assert.equal(records.find((r) => r.type === 'ws-send').text, 'ping');
  assert.deepEqual(records.filter((r) => r.type === 'ws').map((r) => [r.text, r.binary]), [['text-buffer', undefined], [undefined, 'AQI=']]);
  assert.deepEqual(records.filter((r) => r.type === 'open').map((r) => r.socket), [0, 1]); capture.restore();
});

test('mutable incoming and outgoing socket bytes are snapshotted before consumers reuse them', async () => {
  const { capture, records } = fixture(), Socket = capture.wrapWebSocket(NodeSocket), socket = new Socket('wss://public.example/ws');
  await flush();
  const text = Buffer.from('wire-text'), binary = new Uint8Array([1, 2]), outgoing = Buffer.from('{"method":"SUBSCRIBE","params":["book"]}');
  socket.emit('message', text, false); text.fill(120);
  socket.emit('message', binary, true); binary.fill(9);
  socket.send(outgoing, { binary: false }); outgoing.fill(120);
  await capture.closeSockets(); await capture.drain();
  assert.deepEqual(records.filter((r) => r.type === 'ws').map((r) => r.text ?? r.binary), ['wire-text', 'AQI=']);
  assert.equal(records.find((r) => r.type === 'ws-send').text, '{"method":"SUBSCRIBE","params":["book"]}'); capture.restore();
});

test('socket auth checks cover typed buffers; outgoing Blob forms refuse delegation', async () => {
  for (const data of [Buffer.from('{"method":"AUTH"}'), new TextEncoder().encode('{"method":"AUTH"}').buffer,
    new Blob(['{"method":"AUTH"}'])]) {
    const { capture } = fixture(), Socket = capture.wrapWebSocket(NodeSocket), socket = new Socket('wss://public.example/ws');
    await flush(); assert.throws(() => socket.send(data)); assert.deepEqual(socket.sent, []);
    await capture.closeSockets(); capture.restore();
  }
});

test('asynchronous Blob conversion cannot reorder browser messages and close events', async () => {
  const { capture, records } = fixture(); let release;
  class SlowBlob extends Blob { async arrayBuffer() { await new Promise((resolve) => { release = resolve; }); return super.arrayBuffer(); } }
  const Socket = capture.wrapWebSocket(BrowserSocket), socket = new Socket('wss://public.example/ws'); await flush();
  socket.dispatchEvent(new MessageEvent('message', { data: new SlowBlob(['binary']) })); socket.close(); await flush();
  release(); await capture.drain();
  assert.deepEqual(records.map((r) => r.type), ['open', 'ws', 'close']); assert.equal(records[1].binary, Buffer.from('binary').toString('base64'));
  capture.restore();
});

test('bounded socket queues and authentication sends fail without complete capture', async () => {
  const records = [], capture = createTsCapture({ axios, maxBytes: 256, write: (r) => records.push(r) });
  const Socket = capture.wrapWebSocket(NodeSocket), socket = new Socket('wss://public.example/ws'); await flush();
  socket.emit('message', Buffer.alloc(257), true); await assert.rejects(capture.drain(), /byte limit/);
  await capture.closeSockets(); capture.restore();
  const second = fixture(), PublicSocket = second.capture.wrapWebSocket(NodeSocket), other = new PublicSocket('wss://public.example/ws');
  await flush(); assert.throws(() => other.send('{"method":"AUTH"}'), /authentication/);
  assert.deepEqual(other.sent, []); await second.capture.closeSockets(); second.capture.restore();
});

test('direct fetch captures finite raw chunks without consuming the caller body', async () => {
  const { capture, records } = fixture();
  const chunks = [Buffer.from('{"id":'), Buffer.from('9007199254740993}')];
  const fetch = capture.wrapFetch(async () => new Response(new ReadableStream({ start(c) { for (const x of chunks) c.enqueue(x); c.close(); } }),
    { headers: { 'content-length': String(chunks.reduce((n, x) => n + x.length, 0)) } }));
  const actual = await fetch('https://public.example/ticker');
  assert.equal(await actual.text(), '{"id":9007199254740993}'); await capture.drain();
  assert.equal(records[0].body, '{"id":9007199254740993}'); capture.restore();
});
for (const [label, body, headers] of [['unknown length', 'ok', {}], ['truncated', 'x', { 'content-length': '2' }],
  ['invalid UTF-8', new Uint8Array([255]), { 'content-length': '1' }], ['oversize', 'x', { 'content-length': '99999999' }]]) {
  test(`unsupported fetch ${label} fails closed`, async () => {
    const { capture } = fixture(), fetch = capture.wrapFetch(async () => new Response(body, { headers }));
    try { await fetch('https://public.example/ticker'); } catch { /* Header failures precede the returned response. */ }
    await assert.rejects(capture.drain()); capture.restore();
  });
}
test('fetch caller cancellation reaches the actual transport and invalidates the capture', async () => {
  const { capture } = fixture(), controller = new AbortController();
  const fetch = capture.wrapFetch((_url, { signal }) => new Promise((_, reject) => signal.addEventListener('abort', () => reject(new Error('canceled')), { once: true })));
  const request = fetch('https://public.example/ticker', { signal: controller.signal }); controller.abort();
  await assert.rejects(request, /canceled/); await assert.rejects(capture.drain()); capture.restore();
});

test('broken finite fetch streams refuse completion', async () => {
  const { capture } = fixture();
  const fetch = capture.wrapFetch(async () => new Response(new ReadableStream({ start(controller) { controller.error(new Error('truncated stream')); } }),
    { headers: { 'content-length': '10' } }));
  await fetch('https://public.example/ticker'); await assert.rejects(capture.drain(), /truncated stream/); capture.restore();
});

test('atomic tape refuses overwrite and failed/partial writes never publish', () => {
  const dir = mkdtempSync(join(tmpdir(), 'ts-record-output-'));
  try {
    const out = join(dir, 'tape.jsonl'), sink = atomicTsTape(out); sink.write({ type: 'meta' }); sink.publish(); sink.abort();
    assert.equal(readFileSync(out, 'utf8'), '{"type":"meta"}\n'); assert.throws(() => atomicTsTape(out), /already exists/);
    const limited = atomicTsTape(join(dir, 'too-large'), { maxBytes: 1 }); assert.throws(() => limited.write({ data: 1 })); limited.abort();
    const race = atomicTsTape(join(dir, 'race')); race.write({ complete: true }); writeFileSync(join(dir, 'race'), 'other owner');
    assert.throws(() => race.publish()); race.abort(); assert.equal(readFileSync(join(dir, 'race'), 'utf8'), 'other owner');
    assert.deepEqual(readdirSync(dir).sort(), ['race', 'tape.jsonl']);
    const partial = atomicTsTape(join(dir, 'failed-write'), { io: { ...fs, writeSync: () => 0 } });
    assert.throws(() => partial.write({ data: 'test' }), /did not progress/); partial.abort();
    assert.deepEqual(readdirSync(dir).sort(), ['race', 'tape.jsonl']);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

function clock() {
  let time = 0; const timers = new Set();
  return { now: () => time, get pending() { return timers.size; },
    delay(ms, signal) { return new Promise((resolve, reject) => {
      const timer = { at: time + ms, resolve: () => { timers.delete(timer); signal?.removeEventListener('abort', abort); resolve(); } };
      const abort = () => { timers.delete(timer); reject(new Error('aborted')); };
      if (signal?.aborted) return abort(); timers.add(timer); signal?.addEventListener('abort', abort, { once: true });
    }); },
    advance(ms) { time += ms; for (const timer of [...timers]) if (timer.at <= time) timer.resolve(); },
  };
}
function scenario(t, { missing = false, failSink = false, failLoader = false, hangLoader = false, unsubscribeError = false, unsubscribeSends = false } = {}) {
  const prior = Object.getOwnPropertyDescriptor(globalThis, 'WebSocket');
  Object.defineProperty(globalThis, 'WebSocket', { configurable: true, writable: true, value: BrowserSocket });
  t.after(() => { if (prior) Object.defineProperty(globalThis, 'WebSocket', prior); else delete globalThis.WebSocket; });
  const time = clock(), records = []; let published = false, aborted = false, disposed = false, restored = false;
  class Store {
    markets = []; loaded = { markets: false, tickers: false };
    subscribe(callback) { this.callback = callback; return () => { this.callback = undefined; }; }
  }
  class Exchange {
    constructor(_name, options, store) { assert(options.publicDataOnly); this.store = store; }
    async start() {
      this.store.markets = [{ symbol: 'BTCUSDT' }]; this.store.loaded = { markets: true, tickers: true }; this.store.callback(this.store);
      this.socket = new globalThis.WebSocket('wss://public.example/ws');
      this.socket.addEventListener('open', () => {
        this.socket.send('{"method":"SUBSCRIBE","params":["TS_ONLY_STARTUP"],"id":1}');
        this.socket.dispatchEvent(new MessageEvent('message', { data: '{"e":"24hrTicker","s":"BTCUSDT"}' }));
      });
    }
    listenOrderBook(symbol, callback) {
      this.socket.send(JSON.stringify({ method: 'SUBSCRIBE', params: [`TS_ONLY_BOOK.${symbol}`], id: 2 }));
      if (!missing || symbol === 'BTCUSDT') { callback({ symbol }); callback({ symbol }); }
      return () => {
        if (unsubscribeSends) this.socket.send('{"method":"UNSUBSCRIBE","params":["TS_ONLY_BOOK"],"id":3}');
        if (unsubscribeError) throw new Error('unsubscribe failure');
      };
    }
    dispose() { disposed = true; }
  }
  const dependencies = { now: time.now, delay: time.delay, startupMs: 50, shutdownMs: 50,
    sink: { write(record) { if (failSink) throw new Error('sink failed'); records.push(record); },
      publish() { published = true; }, abort() { aborted = true; } },
    loadTransports() { return { axios, NodeWebSocket: NodeSocket, installNodeSocket() { return () => { restored = true; }; } }; },
    async loadExchange() {
      if (failLoader) throw new Error('loader failed');
      if (hangLoader) return new Promise(() => {});
      return { Exchange, DefaultStore: Store };
    },
  };
  const task = recordTsPublic({ books: missing ? ['BTCUSDT', 'ETHUSDT'] : ['BTCUSDT'], seconds: 1, env: {}, dependencies });
  task.catch(() => {});
  return { task, time, records, dependencies, state: () => ({ published, aborted, disposed, restored }) };
}
test('unresolved loading and sink failure during loading stay bounded and restore hooks', async (t) => {
  for (const failSink of [false, true]) await t.test(`sink failure: ${failSink}`, async (child) => {
    const s = scenario(child, { hangLoader: true, failSink }); await flush(); s.time.advance(50); await flush();
    await assert.rejects(s.task, failSink ? /sink failed/ : /loading\/readiness timed out/);
    assert.deepEqual(s.state(), { published: false, aborted: true, disposed: false, restored: true });
    assert.equal(s.time.pending, 0); assert.equal(globalThis.WebSocket, BrowserSocket);
  });
});
test('offline TS runner captures actual extra subscriptions and publishes only after cleanup', async (t) => {
  const s = scenario(t, { unsubscribeSends: true }); await flush(); s.time.advance(10); await flush(); s.time.advance(1000); await s.task;
  const sent = s.records.filter((r) => r.type === 'ws-send').map((r) => JSON.parse(r.text).params[0]);
  assert.deepEqual(sent, ['TS_ONLY_STARTUP', 'TS_ONLY_BOOK.BTCUSDT', 'TS_ONLY_BOOK']);
  const fakeRustTraffic = ['RUST_ONLY']; assert(sent.some((channel) => !fakeRustTraffic.includes(channel)));
  assert.equal(s.records[0].source, 'live-ts-public');
  assert.deepEqual(s.state(), { published: true, aborted: true, disposed: true, restored: true });
  assert.equal(s.time.pending, 0); assert.equal(globalThis.WebSocket, BrowserSocket);
});
for (const [label, settings, expected] of [['missing book', { missing: true }, /timed out/], ['sink', { failSink: true }, /sink failed/],
  ['loader', { failLoader: true }, /loader failed/], ['unsubscribe', { unsubscribeError: true }, /unsubscribe failure/]]) {
  test(`runner ${label} failure refuses publication and restores resources`, async (t) => {
    const s = scenario(t, settings); await flush(); s.time.advance(10); await flush(); s.time.advance(1000); await flush(); s.time.advance(100);
    await assert.rejects(s.task, expected);
    assert.equal(s.state().published, false); assert(s.state().aborted); assert(s.state().restored);
    if (!settings.failLoader) assert(s.state().disposed); assert.equal(s.time.pending, 0); assert.equal(globalThis.WebSocket, BrowserSocket);
  });
}
