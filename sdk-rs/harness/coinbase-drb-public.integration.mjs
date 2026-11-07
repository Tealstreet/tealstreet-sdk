// Offline WASM/EngineHost gate using the same LIVE tape as the native Rust gate.
// Build first: CARGO_BUILD_JOBS=2 bash sdk-rs/harness/build.sh
// A --venue coinbase build or an explicit module path is supported too.
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { performance } from 'node:perf_hooks';
import { setTimeout as delay } from 'node:timers/promises';
import { EngineHost } from '../hosts/wasm/js/host.mjs';
import { Frame, LOADED_FLAG, TickerState } from '../hosts/wasm/js/wire.mjs';
import { loadEngine } from './engine.mjs';
import { prepareRecordedHttp } from './recorded-http.mjs';

const records = (await readFile(new URL('../venues/coinbase/tests/fixtures/drb-public.jsonl', import.meta.url), 'utf8')).trim().split('\n').map(JSON.parse);
const meta = records[0];
const { WasmEngine } = await loadEngine(process.argv[2]);
let now = meta.epoch, status, markets, socket;
const tickers = new TickerState(), books = new Map(), calls = [];
const transportErrors = [];
const trace = [];
const traceStarted = performance.now();
const recordTrace = (event, detail = {}) => trace.push({ elapsedMs: Math.round(performance.now() - traceStarted), event, ...detail });
const transport = {
  now: () => now,
  setTimeout, clearTimeout,
  prepareHttp(request) {
    recordTrace('prepare', { method: request.method, url: request.url });
    return prepareRecordedHttp.call(this, request);
  },
  connect(url, callbacks) {
    recordTrace('connect', { url });
    assert.equal(url, 'wss://streams.drb.coinbase.com/ws/api/v2');
    socket = callbacks;
    queueMicrotask(callbacks.onOpen);
    return {
      send(text) {
        const request = JSON.parse(text);
        recordTrace('subscription', { url, method: request.method, id: request.id, channels: request.params?.channels?.length });
        assert.ok(['public/subscribe', 'public/unsubscribe'].includes(request.method));
        queueMicrotask(() => {
          recordTrace('subscription-response', { id: request.id });
          callbacks.onMessage(JSON.stringify({ jsonrpc: '2.0', id: request.id, result: request.params.channels }));
        });
      },
      close() {},
    };
  },
  async request({ method, url, headers, body }) {
    recordTrace('request', { method, url });
    try {
      assert.ok(url.startsWith('https://drb.coinbase.com/api/v2/'));
      assert.ok([...new Headers(headers).keys()].every(k => !k.toLowerCase().includes('authorization')));
      const route = new URL(url);
      const request = body ? JSON.parse(body) : { method: route.pathname.slice('/api/v2/'.length) };
      const record = records.find(r => r.type === 'http' && r.method === request.method);
      assert.ok(record, `unrecorded public request ${request.method}`);
      assert.equal(method, record.httpMethod);
      if (method === 'GET') {
        for (const [key, value] of Object.entries(record.params)) assert.equal(route.searchParams.get(key), String(value));
      } else {
        assert.deepEqual(request.params, record.params);
      }
      const response = JSON.parse(record.body);
      if (method === 'POST') response.id = request.id;
      calls.push(request.method);
      recordTrace('response', { method: request.method, id: request.id, status: record.status });
      return { status: record.status, body: new TextEncoder().encode(JSON.stringify(response)) };
    } catch (error) {
      recordTrace('untaped', { method, url, error: error.message });
      transportErrors.push(error);
      throw error;
    }
  },
};
const host = new EngineHost(new WasmEngine('coinbase', JSON.stringify({ publicDataOnly: true })), transport, {
  onLog(level, message) { recordTrace('host-log', { level, message }); },
  onEvent(event) { recordTrace('host-event', { code: event.code, message: event.message, fields: event.fields }); },
  onFrame(buffer) {
    const frame = new Frame(buffer);
    markets = frame.markets() ?? markets;
    status = frame.status() ?? status;
    tickers.apply(frame);
    for (const book of frame.books()) books.set(book.market, book);
  },
});
const started = performance.now();
try {
  host.start();
  const mask = LOADED_FLAG.MARKETS | LOADED_FLAG.TICKERS;
  while ((status?.loaded & mask) !== mask || !socket) {
    host.flush();
    if (transportErrors.length) throw transportErrors[0];
    assert.ok(performance.now() - started < 30_000, 'markets + tickers startup deadline exceeded');
    await delay(1);
  }
  const startupMs = Math.round(performance.now() - started);
  assert.ok(markets.count >= 125);
  assert.ok(tickers.count >= 125);
  assert.deepEqual(calls, ['public/get_instruments', 'public/get_book_summary_by_currency']);
  host.command({ op: 'subscribeOrderBook', symbol: meta.instrument });
  host.command({ op: 'subscribeCandles', symbol: meta.instrument, interval: '1m' });
  await delay(0);
  for (const record of records.filter(r => r.type === 'ws' && JSON.parse(r.text).method === 'subscription')) {
    now = meta.epoch + record.t;
    socket.onMessage(record.text);
  }
  host.flush();
  assert.ok(books.size > 0);
  const candles = await host.command({ op: 'fetchCandles', symbol: meta.instrument, interval: '1m', limit: 60, from: meta.start, to: meta.end });
  assert.equal(candles.length, 60);
  const trades = await host.command({ op: 'fetchPublicTrades', symbol: meta.instrument, limit: 10 });
  assert.equal(trades.length, 10);
  assert.deepEqual(transportErrors, []);
  console.log(`Coinbase drb public WASM replay: ${markets.count} markets, ${tickers.count} tickers, book + 60 candles + 10 trades; startup ${startupMs}ms (within 30s)`);
} catch (error) {
  console.error('Coinbase drb replay diagnostics:', JSON.stringify({ calls, loaded: status?.loaded, connected: Boolean(socket), trace: trace.slice(-40) }, null, 2));
  throw error;
} finally { host.dispose(); }
