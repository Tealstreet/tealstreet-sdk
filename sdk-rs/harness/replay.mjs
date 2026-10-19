// Replay a recorded tape through one implementation and report its final
// Store, order books, CPU time and memory as JSON on stdout.
//
//   node --expose-gc --import tsx sdk-rs/harness/replay.mjs --impl ts --tape <file>
//   node --expose-gc sdk-rs/harness/replay.mjs --impl rust --tape <file>
import { readFileSync } from 'node:fs';
import { parseArgs } from 'node:util';
import { gunzipSync } from 'node:zlib';
import { ORDER_COLUMNS, POSITION_COLUMNS, orderEnums, positionEnums } from '../hosts/wasm/js/wire.mjs';
import { axiosClients, venueOf } from './venues.mjs';
import { callerIdsOf, requestKey, selectResponse, subscriptionKeys } from './requests.mjs';

const { values } = parseArgs({
  options: {
    impl: { type: 'string' },
    venue: { type: 'string', default: 'binance' },
    tape: { type: 'string' },
    books: { type: 'string', default: 'BTCUSDT,ETHUSDT,SOLUSDT' },
    out: { type: 'string' },
    // Skip modelling the worker posting books to main (same work for both).
    'no-egress': { type: 'boolean', default: false },
  },
});
const venue = venueOf(values.venue);
const tapeBytes = readFileSync(values.tape);
const records = (values.tape.endsWith('.gz') ? gunzipSync(tapeBytes) : tapeBytes)
  .toString('utf8')
  .split('\n')
  .filter(Boolean)
  .map((line) => JSON.parse(line));
const bookSymbols = values.books.split(',').filter(Boolean);
// A tape with a user stream replays an account: signed, paced in real time,
// and compared on its private state rather than its books.
const config = records.find((r) => r.type === 'config') ?? {};
const isPrivate = config.private ?? (Boolean(venue.privateSocket) && records.some((r) => r.url?.includes(venue.privateSocket)));
const options = { ...(isPrivate ? { key: 'replay-key', secret: 'replay-secret' } : { key: '', secret: '', publicDataOnly: true, ...venue.publicOptions }), ...config.options };
const callerIds = callerIdsOf(records);
const keyFor = (method, url, body) => requestKey(method, url, body, callerIds);

/** Serves recorded HTTP responses and websocket frames in tape order. */
class Tape {
  constructor() {
    this.responses = new Map();
    for (const record of records.filter((r) => r.type === 'http')) {
      const key = keyFor(record.method, record.url, record.requestBody);
      if (!this.responses.has(key)) this.responses.set(key, []);
      this.responses.get(key).push(record);
    }
    this.frames = records.filter((r) => r.type === 'ws' || r.type === 'command');
    this.open = new Map();
    this.waiters = [];
    this.unserved = 0;
  }

  respond(method, url, body) {
    const key = keyFor(method, url, body);
    this.requests.add(key);
    // Startup reads see time zero; availability advances only once playback starts.
    const elapsed = this.playStartedAt === undefined ? 0 : performance.now() - this.playStartedAt;
    const record = selectResponse(this.responses.get(key), elapsed);
    if (!record) {
      this.unserved++;
      console.error(`unserved: ${key}`);
      return { status: 599, body: '' };
    }
    return { status: record.status, body: record.body };
  }

  connect(url, onOpen, onMessage) {
    const socket = { url, onMessage, closed: false };
    setImmediate(() => {
      if (socket.closed) return;
      if (!this.open.has(url)) this.open.set(url, new Set());
      this.open.get(url).add(socket);
      onOpen();
      this.waiters = this.waiters.filter((waiter) => !waiter());
    });
    return socket;
  }

  send(url, text) {
    for (const key of subscriptionKeys(url, text, callerIds)) this.subscriptions.add(key);
  }

  async socketFor(url) {
    const active = () => [...(this.open.get(url) ?? [])].filter((socket) => !socket.closed);
    while (!active().length) {
      await new Promise((resolve) => this.waiters.push(() => (active().length ? (resolve(), true) : false)));
    }
    return { onMessage: (text) => active().forEach((socket) => socket.onMessage(text)) };
  }

  /**
   * Deliver every frame in order, yielding to the event loop between frames.
   * Commands are awaited where they fall. A private tape is paced: each record
   * waits until its time after the account is ready.
   */
  async play(onTick, ready = async () => {}, runCommand = async () => {}) {
    const urls = [...new Set(this.frames.filter((f) => f.url).map((f) => f.url))];
    for (const url of urls) await this.socketFor(url);
    await ready();
    this.playStartedAt = performance.now();
    const start = this.playStartedAt;
    let delivered = 0;
    const warm = Math.floor(this.frames.length / 4);
    for (const frame of this.frames) {
      if (delivered === warm) this.warmMark = { thread: process.threadCpuUsage(), wall: performance.now() };
      if (isPrivate) {
        const wait = frame.t - (performance.now() - start);
        if (wait > 0) await new Promise((resolve) => setTimeout(resolve, wait));
      }
      if (frame.type === 'command') {
        this.results.push(await runCommand(frame).then((ids) => ({ ok: ids }), (error) => ({ error: String(error?.message ?? error) })));
      } else {
        const socket = await this.socketFor(frame.url);
        socket.onMessage(frame.text);
      }
      delivered++;
      onTick?.(frame.t);
      await new Promise(setImmediate);
    }
    return delivered;
  }

  results = [];
  requests = new Set();
  subscriptions = new Set();
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
  const { EngineHost } = await import('../hosts/wasm/js/host.mjs');
  const { Frame, TickerState, MARKET_COLUMNS } = await import('../hosts/wasm/js/wire.mjs');
  const { loadEngine } = await import('./engine.mjs');
  const { WasmEngine, wasmMemoryBytes } = await loadEngine();
  const tape = new Tape();
  const encoder = new TextEncoder();
  // What the main thread keeps: the latest market table, ticker columns, books.
  let markets;
  const tickers = new TickerState();
  const books = new Map();
  let orders;
  let positions;
  let balance;
  let settings;
  let status;
  const fills = [];
  const transport = {
    now: () => Date.now(),
    connect: (url, { onOpen, onMessage }) => {
      const socket = tape.connect(url, onOpen, onMessage);
      return { send: (text) => tape.send(url, text), close: () => (socket.closed = true) };
    },
    request: async ({ method, url, body: requestBody }) => {
      const { status, body } = tape.respond(method, url, requestBody);
      return { status, headers: '', body: encoder.encode(body) };
    },
    setTimeout: (fn, ms) => setTimeout(fn, ms),
    clearTimeout: (handle) => clearTimeout(handle),
  };
  const host = new EngineHost(new WasmEngine(venue.rust, JSON.stringify(options)), transport, {
    onFrame: (buffer) => {
      // Model postMessage(buffer, [buffer]) to main, then main reading it.
      const received = values['no-egress'] ? buffer : structuredClone(buffer, { transfer: [buffer] });
      const frame = new Frame(received);
      markets = frame.markets() ?? markets;
      tickers.apply(frame);
      for (const book of frame.books()) books.set(book.market, book);
      orders = frame.orders() ?? orders;
      positions = frame.positions() ?? positions;
      balance = frame.balance() ?? balance;
      settings = frame.settings() ?? settings;
      status = frame.status() ?? status;
      const executed = frame.fills();
      for (let i = 0; i < (executed?.count ?? 0); i++) {
        fills.push({ market: executed.market[i], side: executed.side[i], amount: executed.rows[i * 2 + 1], id: executed.orderId(i) });
      }
    },
  });
  const heapBefore = baseline();
  const cpuStart = { process: process.cpuUsage(), thread: process.threadCpuUsage() };
  const wallStart = performance.now();
  host.start();
  for (const symbol of bookSymbols) host.command({ op: 'subscribeOrderBook', symbol });
  let lastFlush = -Infinity;
  const allLoaded = 31;
  const flushTimer = isPrivate ? setInterval(() => host.flush(), 250) : undefined;
  const frames = await tape.play(
    (t) => {
      if (t - lastFlush >= 250) {
        host.flush();
        lastFlush = t;
      }
    },
    async () => {
      while (isPrivate && (status?.loaded ?? 0) !== allLoaded) await new Promise((r) => setTimeout(r, 5));
    },
    (record) => {
      const { op, ...rest } = record;
      return host.command({ op, ...commandArgs(rest) });
    },
  );
  await settle();
  clearInterval(flushTimer);
  host.flush();
  const metrics = measure(cpuStart, wallStart, heapBefore, wasmMemoryBytes(), tape.warmMark);
  host.dispose();

  // Rebuild comparable records outside the measured window.
  const marketRows = [];
  for (let i = 0; i < (markets?.count ?? 0); i++) {
    marketRows.push({
      id: markets.id(i),
      symbol: markets.symbol(i),
      base: markets.base(i),
      quote: markets.quote(i),
      active: (markets.flags[i] & 1) !== 0,
      precision: { amount: markets[MARKET_COLUMNS[0]][i], price: markets[MARKET_COLUMNS[1]][i] },
      limits: { amount: { min: markets.minAmount[i], max: markets.maxAmount[i] } },
    });
  }
  const tickerRows = [];
  for (let i = 0; i < tickers.count; i++) {
    const row = { symbol: markets.symbol(tickers.market[i]) };
    for (const [field, column] of Object.entries(tickers.columns)) row[field] = column[i];
    tickerRows.push(row);
  }
  const bookRows = {};
  for (const [market, book] of books) {
    const levels = (rows) => {
      const out = [];
      for (let i = 0; i < rows.length; i += 3) out.push({ price: rows[i], amount: rows[i + 1] });
      return out;
    };
    bookRows[markets.symbol(market)] = { bids: levels(book.bids), asks: levels(book.asks) };
  }
  const account = isPrivate ? rustAccount({ markets, orders, positions, balance, settings, status, fills }) : undefined;
  return { frames, metrics, unserved: tape.unserved, requests: [...tape.requests].sort(), subscriptions: [...tape.subscriptions].sort(), store: { markets: marketRows, tickers: tickerRows }, books: bookRows, account, commands: tape.results };
}

const commandArgs = ({ t, type, ...args }) => args;


/** Main-thread view of the account, rebuilt from the latest frame tables. */
function rustAccount({ markets, orders, positions, balance, settings, status, fills }) {
  const symbol = (i) => markets.symbol(i);
  const orderRows = [];
  for (let i = 0; i < (orders?.count ?? 0); i++) {
    const row = { id: orders.id(i), symbol: symbol(orders.market[i]), ...orderEnums(orders.enums[i]) };
    for (const column of ORDER_COLUMNS) row[column] = orders[column][i];
    orderRows.push(row);
  }
  const positionRows = [];
  for (let i = 0; i < (positions?.count ?? 0); i++) {
    const row = { symbol: symbol(positions.market[i]), ...positionEnums(positions.enums[i]) };
    for (const column of POSITION_COLUMNS) row[column] = positions[column][i];
    positionRows.push(row);
  }
  const settingRows = [];
  for (let i = 0; i < (settings?.count ?? 0); i++) {
    settingRows.push({ symbol: symbol(settings.market[i]), leverage: settings.leverage[i], marginType: settings.marginType(i) });
  }
  return {
    hedged: status?.hedged ?? false,
    balance,
    orders: orderRows,
    positions: positionRows,
    settings: settingRows,
    fills: fills.map((f) => ({ id: f.id, symbol: symbol(f.market), side: f.side === 0 ? 'buy' : 'sell', amount: f.amount })),
  };
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
    send(text) { tape.send(this.url, text); }
    close() {
      this.readyState = 3;
      this.socket.closed = true;
    }
  }
  globalThis.WebSocket = TapeWebSocket;
  globalThis.fetch = async (input, init = {}) => {
    const request = input instanceof Request ? input : undefined;
    const method = init.method ?? request?.method ?? 'GET';
    const requestBody = init.body ?? (request && !['GET', 'HEAD'].includes(method.toUpperCase()) ? await request.clone().text() : undefined);
    const { status, body } = tape.respond(method, request?.url ?? String(input), requestBody);
    return new Response([204, 205, 304].includes(status) ? null : body, { status });
  };
  const { [venue.ts.className]: Exchange } = await import(venue.ts.module);
  const { DefaultStore } = await import('../../packages/safe-cex/src/store/store.base.ts');
  const adapter = async (config) => {
    const base = config.baseURL ?? '';
    const url = new URL(config.url.startsWith('http') ? config.url : `${base}${config.url}`);
    for (const [key, value] of Object.entries(config.params ?? {})) {
      if (value !== undefined) url.searchParams.set(key, String(value));
    }
    const { status, body } = tape.respond((config.method ?? 'get').toUpperCase(), url.toString(), config.data);
    // The raw body: the client's own transformResponse parses it, big ids included.
    const response = { data: body, status, statusText: String(status), headers: {}, config, request: {} };
    if (status >= 400) {
      const error = new Error(`Request failed with status code ${status}`);
      error.response = response;
      error.config = config;
      throw error;
    }
    return response;
  };
  const store = new DefaultStore();
  const exchange = new Exchange(venue.ts.name, options, store);
  const executions = [];
  exchange.emitter.on('fill', (fill) => executions.push(fill));
  for (const client of axiosClients(exchange)) client.defaults.adapter = adapter;
  const finalBooks = {};
  const heapBefore = baseline();
  const cpuStart = { process: process.cpuUsage(), thread: process.threadCpuUsage() };
  const wallStart = performance.now();
  const started = exchange.start();
  const subscribed = (async () => {
    while (!store.markets.length) await new Promise((r) => setTimeout(r, 1));
    for (const symbol of bookSymbols) exchange.listenOrderBook(symbol, (book) => (finalBooks[symbol] = book));
  })();
  // Model the worker posting each subscribed book to main every 250 ms.
  let lastFlush = -Infinity;
  let posted = 0;
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
      while (!venue.tsBooksSubscribed(exchange, bookSymbols.length)) await new Promise((r) => setTimeout(r, 5));
      const loaded = () => Object.values(store.loaded).every(Boolean);
      while (isPrivate && !loaded()) await new Promise((r) => setTimeout(r, 5));
    },
    async (record) => {
      if (record.op === 'placeOrders') return exchange.placeOrders(record.orders);
      if (record.op === 'amendOrder') {
        const order = store.orders.find((o) => o.id === record.id);
        if (!order) throw new Error('order not found');
        const result = await exchange.updateOrder({ order, update: record.price != null && record.amount != null ? { price: record.price, amount: record.amount } : record.price != null ? { price: record.price } : { amount: record.amount } });
        return result.clientOrderId ? { orderIds: [...result], clientOrderId: result.clientOrderId } : result;
      }
      if (record.op === 'setLeverage') { await exchange.setLeverage(record.symbol, record.leverage); return []; }
      if (record.op === 'setMarginType') { await exchange.changeMarginType(record.isolated, record.symbol); return []; }
      if (record.op === 'setPositionMode') { await exchange.changePositionMode(record.hedged); return []; }
      if (record.op === 'cancelOrders') {
        await exchange.cancelOrders(store.orders.filter((order) => record.ids.includes(order.id)));
        return [];
      }
      if (record.op === 'cancelSymbolOrders') {
        await exchange.cancelSymbolOrders(record.symbol);
        return [];
      }
      throw new Error(`unknown command ${record.op}`);
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
  const account = isPrivate
    ? {
        hedged: Boolean(store.options.isHedged),
        balance: { used: store.balance.used, free: store.balance.free, total: store.balance.total, upnl: store.balance.upnl },
        orders: store.orders.map((o) => ({ ...o })),
        // Zero-size placeholders carry each market's leverage and margin type.
        positions: store.positions.filter((p) => p.contracts > 0).map((p) => ({ ...p })),
        settings: [...new Map(store.positions.filter((p) => p.leverage > 0).map((p) => [p.symbol, { symbol: p.symbol, leverage: p.leverage, marginType: p.marginType }])).values()],
        fills: executions.map((f) => ({ id: f.orderId ?? f.id, symbol: f.symbol, side: f.side, amount: f.amount })),
      }
    : undefined;
  exchange.dispose();
  return { frames, metrics, unserved: tape.unserved, requests: [...tape.requests].sort(), subscriptions: [...tape.subscriptions].sort(), store: snapshot, books: bookCopies, account, commands: tape.results };
}

/** The fields both implementations carry, in one shape. */
function normalizeAccount(account) {
  const order = (o) => ({
    id: String(o.id),
    symbol: o.symbol,
    status: o.status,
    type: o.type,
    side: o.side,
    price: o.price,
    amount: o.amount,
    filled: o.filled,
    remaining: o.remaining,
    reduceOnly: Boolean(o.reduceOnly),
    close: Boolean(o.close),
    trigger: o.tpTriggerType ?? o.slTriggerType ?? o.triggerType ?? null,
    rejectReason: o.extra?.rejectReason ?? o.rejectReason ?? null,
  });
  const position = (p) => ({
    symbol: p.symbol,
    side: p.side,
    entryPrice: p.entryPrice,
    contracts: p.contracts,
    notional: p.notional,
    leverage: p.leverage,
    unrealizedPnl: p.unrealizedPnl,
    liquidationPrice: p.liquidationPrice,
    marginType: p.marginType ?? null,
    positionMode: p.positionMode ?? null,
    marginSize: Number.isFinite(p.marginSize) ? p.marginSize : null,
  });
  const bySymbol = (a, b) => a.symbol.localeCompare(b.symbol) || String(a.id ?? '').localeCompare(String(b.id ?? ''));
  return {
    hedged: account.hedged,
    balance: account.balance,
    orders: account.orders.map(order).sort(bySymbol),
    positions: account.positions.map(position).sort(bySymbol),
    settings: [...account.settings].sort(bySymbol),
    fills: account.fills,
  };
}

const result = values.impl === 'ts' ? await runTs() : await runRust();
const { writeFileSync } = await import('node:fs');
writeFileSync(
  values.out,
  JSON.stringify({
    impl: values.impl,
    frames: result.frames,
    unserved: result.unserved,
    requests: result.requests,
    subscriptions: result.subscriptions,
    metrics: result.metrics,
    markets: result.store.markets.map(marketFields),
    tickers: result.store.tickers.map((t) => Object.fromEntries([['symbol', t.symbol], ...tickerFields.map((f) => [f, t[f]])])),
    books: Object.fromEntries(Object.entries(result.books).map(([s, b]) => [s, sortBook(b)])),
    account: result.account && normalizeAccount(result.account),
    commands: result.commands,
  }),
);
process.exit(0);
