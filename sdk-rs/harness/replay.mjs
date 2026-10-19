// Replay a recorded tape through one implementation and report its final
// Store, order books, CPU time and memory as JSON on stdout.
//
//   node --expose-gc --import tsx sdk-rs/harness/replay.mjs --impl ts --tape <file>
//   node --expose-gc sdk-rs/harness/replay.mjs --impl rust --tape <file>
import { readFileSync } from 'node:fs';
import { parseArgs } from 'node:util';

const { values } = parseArgs({
  options: {
    impl: { type: 'string' },
    tape: { type: 'string' },
    books: { type: 'string', default: 'BTCUSDT,ETHUSDT,SOLUSDT' },
    out: { type: 'string' },
    // Skip modelling the worker posting books to main (same work for both).
    'no-egress': { type: 'boolean', default: false },
  },
});
const records = readFileSync(values.tape, 'utf8')
  .split('\n')
  .filter(Boolean)
  .map((line) => JSON.parse(line));
const books = values.books.split(',').filter(Boolean);

const canonicalUrl = (url) => {
  const parsed = new URL(url);
  const query = [...parsed.searchParams].sort(([a], [b]) => a.localeCompare(b));
  return `${parsed.origin}${parsed.pathname}?${new URLSearchParams(query)}`;
};

/** Serves recorded HTTP responses and websocket frames in tape order. */
class Tape {
  constructor() {
    this.responses = new Map();
    for (const record of records.filter((r) => r.type === 'http')) {
      const key = `${record.method} ${canonicalUrl(record.url)}`;
      if (!this.responses.has(key)) this.responses.set(key, []);
      this.responses.get(key).push(record);
    }
    this.frames = records.filter((r) => r.type === 'ws');
    this.open = new Map();
    this.waiters = [];
    this.unserved = 0;
  }

  respond(method, url) {
    const queue = this.responses.get(`${method} ${canonicalUrl(url)}`);
    if (!queue?.length) {
      this.unserved++;
      return { status: 599, body: '' };
    }
    const record = queue.length > 1 ? queue.shift() : queue[0];
    return { status: record.status, body: record.body };
  }

  connect(url, onOpen, onMessage) {
    const socket = { url, onMessage, closed: false };
    setImmediate(() => {
      if (socket.closed) return;
      this.open.set(url, socket);
      onOpen();
      this.waiters = this.waiters.filter((waiter) => !waiter());
    });
    return socket;
  }

  async socketFor(url) {
    while (!this.open.get(url) || this.open.get(url).closed) {
      await new Promise((resolve) => this.waiters.push(() => (this.open.get(url) ? (resolve(), true) : false)));
    }
    return this.open.get(url);
  }

  /** Deliver every frame in order, yielding to the event loop between frames. */
  async play(onTick, ready = async () => {}) {
    const urls = [...new Set(this.frames.map((f) => f.url))];
    for (const url of urls) await this.socketFor(url);
    await ready();
    let delivered = 0;
    const warm = Math.floor(this.frames.length / 4);
    for (const frame of this.frames) {
      if (delivered === warm) this.warmMark = { thread: process.threadCpuUsage(), wall: performance.now() };
      const socket = await this.socketFor(frame.url);
      socket.onMessage(frame.text);
      delivered++;
      onTick?.(frame.t);
      await new Promise(setImmediate);
    }
    return delivered;
  }
}

const settle = async () => {
  for (let i = 0; i < 20; i++) await new Promise(setImmediate);
};

const measure = (cpuStart, wallStart, heapBefore, wasmBytes = 0, warmMark) => {
  globalThis.gc?.();
  const cpu = process.cpuUsage(cpuStart.process);
  const thread = process.threadCpuUsage(cpuStart.thread);
  const steady = warmMark ? process.threadCpuUsage(warmMark.thread) : { user: 0, system: 0 };
  const memory = process.memoryUsage();
  return {
    // The worker's own thread: what competes with its message handling.
    cpuMs: (thread.user + thread.system) / 1000,
    // Main thread after the first quarter of frames: startup and JIT excluded.
    steadyCpuMs: (steady.user + steady.system) / 1000,
    // All threads, including V8 background compilation and GC helpers.
    processCpuMs: (cpu.user + cpu.system) / 1000,
    wallMs: performance.now() - wallStart,
    rssBytes: memory.rss,
    heapUsedBytes: memory.heapUsed,
    wasmBytes,
    // What the implementation keeps alive beyond the harness and loaded code.
    retainedBytes: memory.heapUsed - heapBefore + wasmBytes,
  };
};

/** Heap retained before the implementation loads, so both start from zero. */
const baseline = () => {
  globalThis.gc?.();
  return process.memoryUsage().heapUsed;
};

const sortBook = (book) => ({
  bids: book.bids.map(({ price, amount }) => [price, amount]),
  asks: book.asks.map(({ price, amount }) => [price, amount]),
});

const tickerFields = ['bid', 'ask', 'last', 'mark', 'index', 'percentage', 'fundingRate', 'fundingTime', 'volume', 'quoteVolume'];
const marketFields = (m) => ({
  id: m.id,
  symbol: m.symbol,
  base: m.base,
  quote: m.quote,
  active: m.active,
  precision: m.precision,
  limits: { amount: m.limits.amount },
});

async function runRust() {
  const { EngineHost, TICKER_FIELDS } = await import('../safe-cex-wasm/js/host.mjs');
  const { loadEngine } = await import('./engine.mjs');
  const { WasmEngine, wasmMemoryBytes } = await loadEngine();
  const tape = new Tape();
  const encoder = new TextEncoder();
  const store = { markets: [], tickers: [] };
  const finalBooks = {};
  const transport = {
    now: () => Date.now(),
    connect: (url, { onOpen, onMessage }) => {
      const socket = tape.connect(url, onOpen, onMessage);
      return { send: () => {}, close: () => (socket.closed = true) };
    },
    request: async ({ method, url }) => {
      const { status, body } = tape.respond(method, url);
      return { status, body: encoder.encode(body) };
    },
    setTimeout: (fn, ms) => setTimeout(fn, ms),
    clearTimeout: (handle) => clearTimeout(handle),
  };
  const host = new EngineHost(new WasmEngine('binance', JSON.stringify({ publicDataOnly: true })), transport, {
    onStoreUpdate: (update) => {
      if (update.markets) store.markets = update.markets;
      if (update.tickers) {
        store.tickers = update.tickers;
      }
    },
    onTickerDeltas: (flat) => {
      for (let i = 0; i < flat.length; ) {
        const ticker = store.tickers[flat[i]];
        const mask = flat[i + 1];
        i += 2;
        for (let field = 0; field < TICKER_FIELDS.length; field++) {
          if (mask & (1 << field)) ticker[TICKER_FIELDS[field]] = flat[i++];
        }
      }
    },
    onBook: (symbol, book) => {
      finalBooks[symbol] = book;
      if (!values['no-egress']) posted += structuredClone(book).bids.length;
    },
  });
  let posted = 0;
  const heapBefore = baseline();
  const cpuStart = { process: process.cpuUsage(), thread: process.threadCpuUsage() };
  const wallStart = performance.now();
  host.start();
  for (const symbol of books) host.command({ op: 'subscribeOrderBook', symbol });
  let lastFlush = -Infinity;
  const frames = await tape.play((t) => {
    if (t - lastFlush >= 250) {
      host.flush();
      lastFlush = t;
    }
  });
  await settle();
  host.flush();
  const metrics = measure(cpuStart, wallStart, heapBefore, wasmMemoryBytes(), tape.warmMark);
  host.dispose();
  return { frames, metrics, unserved: tape.unserved, store, books: finalBooks };
}

async function runTs() {
  const tape = new Tape();
  class TapeWebSocket {
    static CONNECTING = 0;
    static OPEN = 1;
    static CLOSING = 2;
    static CLOSED = 3;
    constructor(url) {
      this.url = url;
      this.readyState = 0;
      this.listeners = { open: [], message: [], close: [], error: [] };
      this.socket = tape.connect(
        url,
        () => {
          this.readyState = 1;
          for (const fn of this.listeners.open) fn({});
          this.onopen?.({});
        },
        (data) => {
          for (const fn of this.listeners.message) fn({ data });
          this.onmessage?.({ data });
        },
      );
    }
    addEventListener(type, fn) {
      this.listeners[type]?.push(fn);
    }
    removeEventListener(type, fn) {
      this.listeners[type] = (this.listeners[type] ?? []).filter((f) => f !== fn);
    }
    send() {}
    close() {
      this.readyState = 3;
      this.socket.closed = true;
    }
  }
  globalThis.WebSocket = TapeWebSocket;
  const root = '../../packages/safe-cex/src';
  const { BinanceExchange } = await import(`${root}/exchanges/binance/binance.exchange.ts`);
  const { DefaultStore } = await import(`${root}/store/store.base.ts`);
  const adapter = async (config) => {
    const base = config.baseURL ?? '';
    const url = new URL(config.url.startsWith('http') ? config.url : `${base}${config.url}`);
    for (const [key, value] of Object.entries(config.params ?? {})) {
      if (value !== undefined) url.searchParams.set(key, String(value));
    }
    const { status, body } = tape.respond((config.method ?? 'get').toUpperCase(), url.toString());
    const data = body ? JSON.parse(body) : undefined;
    const response = { data, status, statusText: String(status), headers: {}, config, request: {} };
    if (status >= 400) {
      const error = new Error(`Request failed with status code ${status}`);
      error.response = response;
      error.config = config;
      throw error;
    }
    return response;
  };
  const store = new DefaultStore();
  const exchange = new BinanceExchange('binance', { key: '', secret: '', publicDataOnly: true }, store);
  for (const client of [exchange.xhr, exchange.publicXhr]) {
    if (client?.defaults) client.defaults.adapter = adapter;
  }
  const finalBooks = {};
  const heapBefore = baseline();
  const cpuStart = { process: process.cpuUsage(), thread: process.threadCpuUsage() };
  const wallStart = performance.now();
  const started = exchange.start();
  const subscribed = (async () => {
    while (!store.markets.length) await new Promise((r) => setTimeout(r, 1));
    for (const symbol of books) exchange.listenOrderBook(symbol, (book) => (finalBooks[symbol] = book));
  })();
  // Model the worker posting each subscribed book to main every 250 ms.
  let lastFlush = -Infinity;
  let posted = 0;
  const depthHandlers = () =>
    Object.keys(exchange.publicWebsocket.messageHandlers).filter((key) => key.startsWith('depthUpdate.')).length;
  const frames = await tape.play(
    (t) => {
      if (t - lastFlush >= 250) {
        if (!values['no-egress']) {
          for (const book of Object.values(finalBooks)) posted += structuredClone(book).bids.length;
        }
        lastFlush = t;
      }
    },
    async () => {
      while (depthHandlers() < books.length) await new Promise((r) => setTimeout(r, 5));
    },
  );
  await Promise.all([started, subscribed]);
  await settle();
  const metrics = measure(cpuStart, wallStart, heapBefore, 0, tape.warmMark);
  const snapshot = {
    markets: store.markets.map((m) => ({ ...m })),
    tickers: store.tickers.map((t) => ({ ...t })),
  };
  const bookCopies = Object.fromEntries(
    Object.entries(finalBooks).map(([symbol, book]) => [symbol, JSON.parse(JSON.stringify(book))]),
  );
  exchange.dispose();
  return { frames, metrics, unserved: tape.unserved, store: snapshot, books: bookCopies };
}

const result = values.impl === 'ts' ? await runTs() : await runRust();
const { writeFileSync } = await import('node:fs');
writeFileSync(
  values.out,
  JSON.stringify({
    impl: values.impl,
    frames: result.frames,
    unserved: result.unserved,
    metrics: result.metrics,
    markets: result.store.markets.map(marketFields),
    tickers: result.store.tickers.map((t) => Object.fromEntries([['symbol', t.symbol], ...tickerFields.map((f) => [f, t[f]])])),
    books: Object.fromEntries(Object.entries(result.books).map(([s, b]) => [s, sortBook(b)])),
  }),
);
process.exit(0);
