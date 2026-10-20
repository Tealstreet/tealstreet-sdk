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
import { createTape, withTimeout, replayStoreReady, ReplayClock, prepareTsReplaySockets } from './tape.mjs';
import { setTimeout as realTimeout, clearTimeout as realClearTimeout } from 'node:timers';
import { retainedMemory, settledMemory } from './memory.mjs';

const { values } = parseArgs({
  options: {
    impl: { type: 'string' },
    venue: { type: 'string', default: 'binance' },
    tape: { type: 'string' },
    books: { type: 'string', default: 'BTCUSDT,ETHUSDT,SOLUSDT' },
    out: { type: 'string' },
    // Skip modelling the worker posting books to main (same work for both).
    'no-egress': { type: 'boolean', default: false },
    delivery: { type: 'string', default: 'batch' },
    cadence: { type: 'string', default: 'recorded' },
    // Opt-in diagnostics; gates leave profiling disabled.
    profile: { type: 'boolean', default: false },
  },
});
const venue = venueOf(values.venue);
if (!['single', 'batch'].includes(values.delivery)) throw new Error('delivery must be single or batch');
if (!['frame', 'recorded'].includes(values.cadence)) throw new Error('cadence must be frame or recorded');
const tapeBytes = readFileSync(values.tape);
const records = (values.tape.endsWith('.gz') ? gunzipSync(tapeBytes) : tapeBytes)
  .toString('utf8')
  .split('\n')
  .filter(Boolean)
  .map((line) => JSON.parse(line));
const bookSymbols = values.books.split(',').filter(Boolean);
// A tape with a user stream replays an account: signed, paced in real time,
// and compared on its private state rather than its books.
const metadata = records.find((record) => record.type === 'meta');
let clockStart = performance.now();
if (values.venue === 'mexc' && Number.isFinite(metadata?.epoch)) Date.now = () => metadata.epoch + Math.floor(performance.now() - clockStart);
const config = records.find((record) => record.type === 'config') ?? {};
const isPrivate =
  config.private ?? metadata?.private ?? records.some(venue.isPrivateRecord ?? ((r) => r.url?.includes(venue.privateSocket)));
const replayOptions = {
  ...venue.options,
  ...(isPrivate
    ? { key: 'replay-key', secret: 'replay-secret', passphrase: 'replay-passphrase', ...venue.replayCredentials }
    : { key: '', secret: '', publicDataOnly: true, ...venue.publicOptions }),
  ...config.options,
};
const credentials = { key: replayOptions.key, secret: replayOptions.secret, passphrase: replayOptions.passphrase };
const replayDeadline = realTimeout(() => {
  console.error('Replay exceeded its 120-second deadline');
  process.exit(1);
}, 120000);
replayDeadline.unref();

const until = async (predicate, label) => withTimeout((async () => {
  while (!predicate()) await new Promise((resolve) => realTimeout(resolve, 5));
})(), label, 15000);

const settle = async () => {
  for (let i = 0; i < 20; i++) await new Promise(setImmediate);
};

const measure = async (cpuStart, wallStart, heapBefore, wasmBytes = 0, warmMark) => {
  const cpu = process.cpuUsage(cpuStart.process);
  const thread = process.threadCpuUsage(cpuStart.thread);
  const steady = warmMark ? process.threadCpuUsage(warmMark.thread) : { user: 0, system: 0 };
  const wallMs = performance.now() - wallStart;
  const memory = await baseline();
  const linearBytes = typeof wasmBytes === 'function' ? wasmBytes() : wasmBytes;
  return {
    // The worker's own thread: what competes with its message handling.
    cpuMs: (thread.user + thread.system) / 1000,
    // Main thread after the first quarter of frames: startup and JIT excluded.
    steadyCpuMs: (steady.user + steady.system) / 1000,
    // All threads, including V8 background compilation and GC helpers.
    processCpuMs: (cpu.user + cpu.system) / 1000,
    wallMs,
    rssBytes: memory.rss,
    heapBeforeBytes: heapBefore.heapUsed,
    heapUsedBytes: memory.heapUsed,
    heapRetainedBytes: memory.heapUsed - heapBefore.heapUsed,
    bufferRetainedBytes: memory.arrayBuffers - heapBefore.arrayBuffers,
    buffersBefore: heapBefore.arrayBuffers,
    buffersAfter: memory.arrayBuffers,
    wasmBytes: linearBytes,
    // What the implementation keeps alive beyond the harness and loaded code.
    retainedBytes: retainedMemory(heapBefore, memory, linearBytes),
  };
};

/** Loaded code is excluded; collect before I/O and after replay state settles. */
const baseline = () => settledMemory({ pause: () => new Promise((resolve) => realTimeout(resolve, 10)) });

const sortBook = (book) => ({
  bids: book.bids.map(({ price, amount }) => [price, amount]),
  asks: book.asks.map(({ price, amount }) => [price, amount]),
});

const tickerFields = [
  'bid',
  'ask',
  'last',
  'mark',
  'index',
  'percentage',
  'fundingRate',
  'fundingTime',
  'volume',
  'quoteVolume',
  'openInterest',
];
const marketFields = (m) => ({
  id: m.id,
  symbol: m.symbol,
  base: m.base,
  quote: m.quote,
  active: m.active,
  contractSize: m.contractSize ?? null,
  // The engine frame carries trading increments (RUST_REWRITE.md MARKETS).
  precision: { amount: m.precision.amount, price: m.precision.price },
  limits: { amount: m.limits.amount },
});

async function runRust() {
  const { EngineHost } = await import('../hosts/wasm/js/host.mjs');
  const { Frame, TickerState, MARKET_COLUMNS } = await import('../hosts/wasm/js/wire.mjs');
  const { loadEngine } = await import('./engine.mjs');
  const { WasmEngine, wasmMemoryBytes } = await loadEngine(venue.rust);
  const tape = createTape(records, venue, isPrivate, { impl: 'rust', books: bookSymbols, credentials, cadence: values.cadence, engineOptions: replayOptions });
  tape.cadence = values.cadence;
  const clock = values.venue === 'mexc' && !isPrivate ? new ReplayClock(metadata?.epoch ?? Date.now()) : undefined;
  clock?.install();
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
    webSocketHeaders: true,
    connect: (url, { headers, onOpen, onMessage, onClose }) => {
      venue.validateSocket?.(url, headers);
      const socket = tape.connect(url, onOpen, onMessage, onClose);
      return { send: (text) => tape.send(socket, text), close: () => tape.close(socket) };
    },
    request: async ({ method, url, body: requestBody, headers, signal }) => {
      signal?.throwIfAborted();
      const { status, headers: responseHeaders = '', body } = tape.respond(method, url, requestBody, headers);
      const chunks = async function* () {
        const bytes = encoder.encode(body);
        for (let offset = 0; offset < bytes.length; offset += 16 * 1024) {
          signal?.throwIfAborted();
          yield bytes.subarray(offset, offset + 16 * 1024);
        }
      };
      return { status, headers: responseHeaders, body: chunks() };
    },
    setTimeout: (fn, ms) => setTimeout(fn, ms),
    clearTimeout: (handle) => clearTimeout(handle),
  };
  const options = replayOptions;
  const engine = new WasmEngine(venue.rust, JSON.stringify(options));
  if (process.env.RUST_MEMORY_TRACE) {
    let last = wasmMemoryBytes();
    for (const method of ['start', 'httpStart', 'httpChunk', 'httpEnd', 'wsMessage', 'wsMessages', 'takeFrame']) {
      if (typeof engine[method] !== 'function') continue;
      const original = engine[method].bind(engine);
      engine[method] = (...args) => {
        const result = original(...args);
        const bytes = wasmMemoryBytes();
        if (bytes !== last) console.error(`memory ${method} ${method === 'wsMessages' ? `${args[0].byteLength} bytes` : args[0]}: ${last} -> ${bytes}`);
        last = bytes;
        return result;
      };
    }
  }
  const profile = values.profile ? (await import('./profile-engine.mjs')).profileEngine(engine) : undefined;
  const host = new EngineHost(profile?.engine ?? engine, transport, {
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
        fills.push({
          market: executed.market[i],
          side: executed.side[i],
          amount: executed.rows[i * 2 + 1],
          ...(venue.compareFillPrices ? { price: executed.rows[i * 2] } : {}),
          id: executed.orderId(i),
        });
      }
    },
  }, { batchMessages: values.delivery === 'batch' });
  const heapBefore = await baseline();
  const cpuStart = { process: process.cpuUsage(), thread: process.threadCpuUsage() };
  const wallStart = performance.now();
  host.start();
  for (const symbol of bookSymbols) host.command({ op: 'subscribeOrderBook', symbol });
  let lastFlush = -Infinity;
  const flushTimer = isPrivate ? setInterval(() => host.flush(), 250) : undefined;
  const frames = await tape.play(
    (t) => {
      if (t - lastFlush >= 250) {
        host.flush();
        lastFlush = t;
      }
    },
    async () => {
      const deadline = performance.now() + 15000;
      while (
        !replayStoreReady(status?.loaded ?? 0, isPrivate) ||
        (tape.audit && !tape.audit.ready()) ||
        !tape.open.size
      ) {
        if (performance.now() > deadline) throw new Error('Replay timed out waiting for Rust startup');
        host.flush();
        await new Promise((resolve) => realTimeout(resolve, 5));
      }
    },
    (record) => {
      const { op, ...rest } = record;
      return host.command({ op, ...commandArgs(rest) });
    },
    clock,
    () => {
      if (isPrivate) clockStart = performance.now();
    },
  );
  await settle();
  tape.assertWritesConsumed();
  clearInterval(flushTimer);
  host.flush();
  const metrics = await measure(cpuStart, wallStart, heapBefore, wasmMemoryBytes, tape.warmMark);
  if (profile) metrics.profile = profile.methods;
  host.dispose();
  clock?.restore();

  // Rebuild comparable records outside the measured window.
  const marketRows = [];
  for (let i = 0; i < (markets?.count ?? 0); i++) {
    marketRows.push({
      id: markets.id(i),
      symbol: markets.symbol(i),
      base: markets.base(i),
      quote: markets.quote(i),
      active: (markets.flags[i] & 1) !== 0,
      contractSize: markets.contractSize[i] || undefined,
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
  return {
    frames,
    metrics,
    unserved: tape.unserved,
    requests: [...tape.requests].sort(),
    writes: tape.writes,
    missingWrites: tape.missingWrites,
    unusedWrites: tape.missingWrites,
    protocolErrors: tape.audit?.errors() ?? [],
    invalidSignatures: tape.invalidSignatures,
    wsRequests: [...tape.sentRequests].sort(),
    subscriptions: [...(venue.compareSubscriptions ? tape.normalizedSubscriptions : tape.subscriptions)].sort(),
    store: { markets: marketRows, tickers: tickerRows },
    books: bookRows,
    account,
    commands: tape.results,
  };
}

const commandArgs = ({ t, type, ...args }) => args;

/** Main-thread view of the account, rebuilt from the latest frame tables. */
function rustAccount({ markets, orders, positions, balance, settings, status, fills }) {
  const symbol = (i) => markets.symbol(i);
  const orderRows = [];
  for (let i = 0; i < (orders?.count ?? 0); i++) {
    const row = {
      id: orders.id(i),
      ...(venue.compareClientIds ? { clientOrderId: orders.clientOrderId(i) ?? null } : {}),
      symbol: symbol(orders.market[i]),
      ...orderEnums(orders.enums[i]),
    };
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
    // TS has no margin-only placeholder; frames retain it for actual consumers.
    if (values.venue === 'blofin' && !(settings.leverage[i] > 0)) continue;
    settingRows.push({
      symbol: symbol(settings.market[i]),
      marginType: settings.marginType(i),
      ...(venue.independentLeverage
        ? { buyLeverage: settings.buyLeverage[i], sellLeverage: settings.sellLeverage[i] }
        : { leverage: settings.leverage[i] }),
    });
  }
  return {
    hedged: status?.hedged ?? false,
    balance,
    orders: orderRows,
    positions: positionRows,
    settings: settingRows,
    fills: fills.map((f) => ({
      id: f.id,
      symbol: symbol(f.market),
      side: f.side === 0 ? 'buy' : 'sell',
      amount: f.amount,
      ...(venue.compareFillPrices ? { price: f.price } : {}),
    })),
  };
}

async function runTs() {
  const tape = createTape(records, venue, isPrivate, { impl: 'ts', books: bookSymbols, credentials, cadence: values.cadence, engineOptions: replayOptions });
  tape.cadence = values.cadence;
  const clock = values.venue === 'mexc' && !isPrivate ? new ReplayClock(metadata?.epoch ?? Date.now()) : undefined;
  clock?.install();
  class TapeWebSocket {
    static CONNECTING = 0;
    static OPEN = 1;
    static CLOSING = 2;
    static CLOSED = 3;
    constructor(url, _protocols, options = {}) {
      venue.validateSocket?.(url, options.headers);
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
          venue.afterTsFrame?.(exchange);
        },
        (code) => {
          this.readyState = 3;
          for (const fn of this.listeners.close) fn({ code });
          this.onclose?.({ code });
        },
      );
    }
    addEventListener(type, fn) {
      this.listeners[type]?.push(fn);
    }
    removeEventListener(type, fn) {
      this.listeners[type] = (this.listeners[type] ?? []).filter((f) => f !== fn);
    }
    send(text) {
      tape.send(this.socket, text);
    }
    close() {
      this.readyState = 3;
      tape.close(this.socket);
    }
  }
  globalThis.WebSocket = TapeWebSocket;
  // Hyperliquid's SDK uses fetch instead of the axios adapters below.
  if (values.venue === 'hyperliquid') globalThis.fetch = async (input, init = {}) => {
    const request = input instanceof Request ? input : undefined;
    const method = init.method ?? request?.method ?? 'GET';
    const requestBody = init.body ?? (request && !['GET', 'HEAD'].includes(method.toUpperCase()) ? await request.clone().text() : undefined);
    const { status, body, headers = '' } = tape.respond(method, request?.url ?? String(input), requestBody);
    const responseHeaders = headers.split('\n').filter(Boolean).map((line) => {
      const colon = line.indexOf(':');
      return [line.slice(0, colon).trim(), line.slice(colon + 1).trim()];
    });
    return new Response([204, 205, 304].includes(status) ? null : body, { status, headers: responseHeaders });
  };
  // Header-capable Node sockets resolve `ws` directly instead of the global.
  const { createRequire } = await import('node:module');
  const require = createRequire(new URL('../../packages/safe-cex/src/exchanges/base.ws.ts', import.meta.url));
  const wsModule = require.resolve('ws');
  require.cache[wsModule] = { id: wsModule, filename: wsModule, loaded: true, exports: TapeWebSocket };

  const { [venue.ts.className]: Exchange } = await import(venue.ts.module);
  const { DefaultStore } = await import('../../packages/safe-cex/src/store/store.base.ts');
  const adapter = async (config) => {
    const base = config.baseURL ?? '';
    const url = new URL(config.url.startsWith('http') ? config.url : `${base}${config.url}`);
    for (const [key, value] of Object.entries(config.params ?? {})) {
      if (value !== undefined) url.searchParams.set(key, String(value));
    }
    const {
      status,
      body,
      headers: responseHeaders,
    } = tape.respond(
      (config.method ?? 'get').toUpperCase(),
      url.toString(),
      config.data,
      config.headers?.toJSON?.() ?? config.headers,
    );
    // The raw body: the client's own transformResponse parses it, big ids included.
    const headers = Object.fromEntries(
      (responseHeaders ?? '')
        .split('\n')
        .filter(Boolean)
        .map((line) => {
          const colon = line.indexOf(':');
          return [line.slice(0, colon).trim().toLowerCase(), line.slice(colon + 1).trim()];
        }),
    );
    const response = { data: body, status, statusText: String(status), headers, config, request: {} };
    if (status >= 400) {
      const error = new Error(`Request failed with status code ${status}`);
      error.response = response;
      error.config = config;
      throw error;
    }
    return response;
  };
  const { ExchangeWorker } = await import('../../packages/safe-cex/src/webworker/ExchangeWorker.ts');
  const store = new DefaultStore();
  const mainStore = new DefaultStore();
  // Exercise the production browser-worker packer, including its ticker
  // baseline, and the real receiver. Rust already transfers every store table.
  const worker = new ExchangeWorker({}, 'replay', venue.ts.name, {}, {}, {}, 'replay');
  worker._store = store;
  let state;
  store.subscribe((data) => {
    state = data;
  });
  const postStore = () => {
    if (!values['no-egress'] && state) mainStore.update(structuredClone(worker._prepareStorePayload(state)), true);
  };
  const options = replayOptions;
  const exchange = new Exchange(venue.ts.name, { ...options, tealstreetTunnel: false }, store);
  venue.configureTs?.(exchange);
  venue.prepareTs?.(exchange, TapeWebSocket);
  const executions = [];
  exchange.emitter.on('fill', (fill) => executions.push(fill));
  for (const client of axiosClients(exchange)) client.defaults.adapter = adapter;
  const finalBooks = {};
  const heapBefore = await baseline();
  const cpuStart = { process: process.cpuUsage(), thread: process.threadCpuUsage() };
  const wallStart = performance.now();
  const started = exchange.start();
  const subscribed = (async () => {
    while (!store.markets.length) await new Promise((r) => setTimeout(r, 1));
    for (const symbol of bookSymbols) exchange.listenOrderBook(symbol, (book) => (finalBooks[symbol] = book));
  })();
  // Model the worker posting each subscribed book to main every 250 ms.
  let lastFlush = -Infinity;
  const mainBooks = {};
  const frames = await tape.play(
    (t) => {
      if (t - lastFlush >= 250) {
        postStore();
        if (!values['no-egress']) {
          for (const [symbol, book] of Object.entries(finalBooks)) mainBooks[symbol] = structuredClone(book);
        }
        lastFlush = t;
      }
    },
    async () => {
      await Promise.all([started, subscribed]);
      await prepareTsReplaySockets(tape, () => venue.prepareTsReady?.(exchange, { isPrivate }), until);
      while (!venue.tsBooksSubscribed(exchange, bookSymbols.length)) await new Promise((r) => setTimeout(r, 5));
      const loaded = () => Object.values(store.loaded).every(Boolean);
      while (isPrivate && !loaded()) await new Promise((r) => setTimeout(r, 5));
    },
    (record) => runTsCommand(exchange, store, record),
    clock,
    () => {
      if (isPrivate) clockStart = performance.now();
    },
  );
  await withTimeout(Promise.all([started, subscribed]), 'TS startup/subscriptions');
  await settle();
  tape.assertWritesConsumed();
  postStore();
  if (!values['no-egress'])
    for (const [symbol, book] of Object.entries(finalBooks)) mainBooks[symbol] = structuredClone(book);
  const metrics = await measure(cpuStart, wallStart, heapBefore, 0, tape.warmMark);
  const snapshot = {
    markets: (values['no-egress'] ? store : mainStore).markets.map((m) => ({ ...m })),
    tickers: (values['no-egress'] ? store : mainStore).tickers.map((t) => ({ ...t })),
  };
  const bookCopies = Object.fromEntries(
    Object.entries(values['no-egress'] ? finalBooks : mainBooks).map(([symbol, book]) => [
      symbol,
      JSON.parse(JSON.stringify(book)),
    ]),
  );
  const account = isPrivate
    ? {
        hedged: Boolean(store.options.isHedged),
        balance: {
          used: store.balance.used,
          free: store.balance.free,
          total: store.balance.total,
          upnl: store.balance.upnl,
        },
        orders: store.orders.map((o) => ({ ...o })),
        // Zero-size placeholders carry each market's leverage and margin type.
        positions: store.positions.filter((p) => p.contracts > 0).map((p) => ({ ...p })),
        settings: venue.tsSettings?.(exchange, store) ?? [
          ...new Map(
            store.positions
              .filter((p) => p.leverage > 0)
              .map((p) => [p.symbol, { symbol: p.symbol, leverage: p.leverage, marginType: p.marginType }]),
          ).values(),
        ],
        fills: executions.map((f) => ({
          id: f.orderId ?? f.id,
          symbol: f.symbol,
          side: f.side,
          amount: f.amount,
          ...(venue.compareFillPrices ? { price: f.price } : {}),
        })),
      }
    : undefined;
  exchange.dispose();
  clock?.restore();
  return {
    frames,
    metrics,
    unserved: tape.unserved,
    requests: [...tape.requests].sort(),
    writes: tape.writes,
    missingWrites: tape.missingWrites,
    unusedWrites: tape.missingWrites,
    protocolErrors: tape.audit?.errors() ?? [],
    invalidSignatures: tape.invalidSignatures,
    wsRequests: [...tape.sentRequests].sort(),
    subscriptions: [...(venue.compareSubscriptions ? tape.normalizedSubscriptions : tape.subscriptions)].sort(),
    store: snapshot,
    books: bookCopies,
    account,
    commands: tape.results,
  };
}

async function runTsCommand(exchange, store, record) {
  const { op, ...args } = commandArgs(record);
  switch (op) {
    case 'placeOrders':
      return exchange.placeOrders(args.orders);
    case 'cancelOrders':
      await exchange.cancelOrders(store.orders.filter((order) => args.ids.includes(order.id)));
      return [];
    case 'cancelSymbolOrders':
      await exchange.cancelSymbolOrders(args.symbol);
      return [];
    case 'amendOrder': {
      const order = store.orders.find((o) => o.id === args.id);
      if (!order) throw new Error(values.venue === 'mexc' ? `Order ${args.id} not found` : `unknown order ${args.id}`);
      const update = Object.fromEntries(
        ['price', 'amount'].filter((key) => args[key] !== undefined).map((key) => [key, args[key]]),
      );
      const result = await exchange.updateOrder({ order, update });
      if (values.venue === 'hyperliquid') {
        const ids = result ?? [];
        return ids.clientOrderId ? { orderIds: [...ids], clientOrderId: ids.clientOrderId } : ids;
      }
      return ['mexc', 'backpack'].includes(values.venue) ? result ?? [] : result;
    }
    case 'setLeverage':
      await exchange.setLeverage(args.symbol, args.leverage);
      return [];
    case 'setMarginType':
      await exchange.changeMarginType(args.isolated, args.symbol);
      return [];
    case 'setPositionMode':
      await exchange.changePositionMode(args.hedged);
      return [];
    case 'findOrderByClientOrderId':
      return exchange.findOrderByClientOrderId(args.symbol, args.clientOrderId);
    case 'fetchLeverage':
      return exchange.fetchLeverageForSymbol(args.symbol);
    case 'fetchFills':
      return exchange.fetchHistoricalFills(args);
    case 'fetchOrderHistory':
      return exchange.fetchHistoricalOrders(args);
    case 'fetchIncome': {
      const { from, to, ...options } = args;
      return exchange.fetchIncomeHistory({ ...options, startTime: from, endTime: to });
    }
    case 'fetchPublicTrades':
      return exchange.fetchPublicTrades(args.symbol, args.limit);
    case 'fetchCandles':
      return exchange.fetchOHLCV(args);
    default:
      throw new Error(`unknown command ${op}`);
  }
}

/** The fields both implementations carry, in one shape. */
function normalizeAccount(account) {
  const order = (o) => ({
    id: String(o.id),
    ...(venue.compareClientIds ? { clientOrderId: o.clientOrderId ?? null } : {}),
    symbol: o.symbol,
    status: o.status,
    type: o.type,
    side: o.side,
    price: o.price,
    amount: o.amount,
    filled: o.filled,
    remaining: o.remaining,
    ...(venue.compareClientIds && o.type === 'limit' ? { timeInForce: o.timeInForce ?? null } : {}),
    reduceOnly: Boolean(o.reduceOnly),
    close: Boolean(o.close),
    trigger: o.tpTriggerType ?? o.slTriggerType ?? o.triggerType ?? null,
    rejectReason: o.extra?.rejectReason ?? o.rejectReason ?? venue.tsRejectReason?.(o) ?? null,
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
    settings: account.settings
      .map((setting) => ({
        symbol: setting.symbol,
        marginType: setting.marginType,
        ...(venue.independentLeverage
          ? { buyLeverage: setting.buyLeverage, sellLeverage: setting.sellLeverage }
          : { leverage: setting.leverage }),
      }))
      .sort(bySymbol),
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
    writes: result.writes,
    writeCount: result.writes.length,
    missingWrites: result.missingWrites,
    unusedWrites: result.unusedWrites,
    protocolErrors: result.protocolErrors,
    invalidSignatures: result.invalidSignatures,
    wsRequests: result.wsRequests,
    subscriptions: result.subscriptions,
    metrics: result.metrics,
    markets: result.store.markets.filter(venue.marketFilter ?? (() => true)).map(marketFields),
    tickers: result.store.tickers
      .filter(venue.marketFilter ?? (() => true))
      .map((t) =>
        Object.fromEntries([
          ['symbol', t.symbol],
          ...tickerFields.map((f) => [f, f === 'openInterest' ? (t[f] ?? 0) : t[f]]),
        ]),
      ),
    books: Object.fromEntries(Object.entries(result.books).map(([s, b]) => [s, sortBook(b)])),
    account: result.account && normalizeAccount(result.account),
    commands: result.commands,
  }),
);
realClearTimeout(replayDeadline);
process.exit(0);
