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
import { Tape, replayStoreReady, ReplayClock } from './tape.mjs';
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
  },
});
const venue = venueOf(values.venue);
const tapeBytes = readFileSync(values.tape);
const records = (values.tape.endsWith('.gz') ? gunzipSync(tapeBytes) : tapeBytes)
  .toString('utf8')
  .split('\n')
  .filter(Boolean)
  .map((line) => {
    const record = JSON.parse(line);
    // Both tape spellings describe the earliest time a snapshot is visible.
    if (record.effectiveAt !== undefined) record.availableAfter ??= record.effectiveAt;
    return record;
  });
const replayOptions = records.find((r) => r.type === 'config')?.options ?? {};
if (values.venue === 'hyrotrader' && replayOptions.endpoints) {
  process.env.NEXT_PUBLIC_HYROTRADER_PROD_REST_URL = replayOptions.endpoints.rest;
  process.env.NEXT_PUBLIC_HYROTRADER_PROD_PUBLIC_WS_URL = replayOptions.endpoints.publicWs;
  process.env.NEXT_PUBLIC_HYROTRADER_PROD_PRIVATE_WS_URL = replayOptions.endpoints.privateWs;
}
const bookSymbols = values.books.split(',').filter(Boolean);
const metadata = records.find((r) => r.type === 'meta');
// Pin generated timestamps and freshness windows to the tape, across processes.
let clockStart = performance.now();
if (Number.isFinite(metadata?.epoch)) Date.now = () => metadata.epoch + Math.floor(performance.now() - clockStart);
// A tape with a user stream replays an account: signed, paced in real time,
// and compared on its private state rather than its books.
// A wrong startup URL, missing socket or lost command must fail the gate.
const replayDeadline = realTimeout(() => {
  console.error('Replay exceeded its 120-second deadline');
  process.exit(1);
}, 120_000);
replayDeadline.unref();
class ReplayTimeout extends Error {}
const within = async (promise, ms, label) => {
  let timer;
  try {
    return await Promise.race([
      promise,
      new Promise((_, reject) => {
        timer = realTimeout(() => reject(new ReplayTimeout(`${label} exceeded ${ms} ms`)), ms);
      }),
    ]);
  } finally {
    realClearTimeout(timer);
  }
};
const config = records.find((record) => record.type === 'config') ?? {};
const isPrivate =
  config.private ??
  metadata?.private ??
  records.some((record) =>
    venue.isPrivateRecord
      ? venue.isPrivateRecord(record)
      : Boolean(venue.privateSocket && record.url?.includes(venue.privateSocket)),
  );
const options = {
  ...venue.options,
  ...metadata?.options,
  ...(typeof metadata?.testnet === 'boolean' ? { testnet: metadata.testnet } : {}),
  ...(isPrivate
    ? { key: 'replay-key', secret: 'replay-secret', passphrase: 'replay-passphrase', ...venue.replayCredentials }
    : { key: '', secret: '', publicDataOnly: true, ...venue.publicOptions }),
  ...config.options,
};
await venue.prepareReplay?.({ isPrivate, options });
await venue.initializeReplay?.(values.impl, options);

/** Serves recorded HTTP responses and websocket frames in tape order. */

const settle = async () => {
  for (let i = 0; i < 20; i++) await new Promise(setImmediate);
};
const until = async (predicate, label) => {
  const deadline = performance.now() + 15000;
  while (!predicate()) {
    if (performance.now() > deadline) throw new Error(`Replay timed out waiting for ${label}`);
    await new Promise((resolve) => realTimeout(resolve, 5));
  }
};

const measure = async (cpuStart, wallStart, heapBefore, wasmMemory = 0, warmMark) => {
  const cpu = process.cpuUsage(cpuStart.process);
  const thread = process.threadCpuUsage(cpuStart.thread);
  const steady = warmMark ? process.threadCpuUsage(warmMark.thread) : { user: 0, system: 0 };
  const wallMs = performance.now() - wallStart;
  const memory = await baseline();
  const wasmBytes = typeof wasmMemory === 'function' ? wasmMemory() : wasmMemory;
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
    wasmBytes,
    // Include implementation modules, retained frames and reserved wasm symmetrically.
    retainedBytes: retainedMemory(heapBefore, memory, wasmBytes),
  };
};

/** Include implementation modules; collect until backing buffers settle. */
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
  'openInterest',
  'fundingRate',
  'fundingTime',
  'volume',
  'quoteVolume',
];
const marketFields = (m) => ({
  id: m.id,
  symbol: m.symbol,
  base: m.base,
  quote: m.quote,
  active: m.active,
  isInverse: Boolean(m.isInverse),
  isSpot: Boolean(m.isSpot),
  isExpiring: Boolean(m.isExpiring),
  contractSize: m.contractSize ?? null,
  precision: { amount: m.precision.amount, price: m.precision.price },
  limits: { amount: m.limits.amount },
});

async function runRust() {
  const heapBefore = await baseline();
  const { EngineHost } = await import('../hosts/wasm/js/host.mjs');
  const { Frame, TickerState, MARKET_COLUMNS } = await import('../hosts/wasm/js/wire.mjs');
  const { loadEngine } = await import('./engine.mjs');
  const { WasmEngine, wasmMemoryBytes } = await loadEngine(venue.rust, options);
  const tape = new Tape(records, venue, isPrivate, 30000, 'rust', bookSymbols, options);
  const encoder = new TextEncoder();
  const cpuStart = { process: process.cpuUsage(), thread: process.threadCpuUsage() };
  const wallStart = performance.now();

  const clock = isPrivate ? undefined : new ReplayClock(metadata?.epoch ?? Date.now());
  clock?.install();
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
      // Model nativeTransport's chunks so streamed lists do not allocate a whole
      // response in wasm before the venue sees the first item.
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
  const engine = new WasmEngine(venue.rust, JSON.stringify(options));
  if (process.env.RUST_MEMORY_TRACE) {
    let last = wasmMemoryBytes();
    for (const method of ['start', 'httpStart', 'httpChunk', 'httpEnd', 'wsMessage', 'takeFrame']) {
      const original = engine[method].bind(engine);
      engine[method] = (...args) => {
        const result = original(...args);
        const bytes = wasmMemoryBytes();
        if (bytes !== last) console.error(`memory ${method} ${args[0]}: ${last} -> ${bytes}`);
        last = bytes;
        return result;
      };
    }
  }
  const host = new EngineHost(engine, transport, {
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
          price: executed.rows[i * 2],
          id: executed.orderId(i),
        });
      }
    },
  });
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
    async (accountReady) => {
      await until(() => {
        host.flush();
        return replayStoreReady(status?.loaded ?? 0, isPrivate && accountReady) && (!tape.audit || tape.audit.ready());
      }, 'Rust startup');
      await until(() => tape.open.size > 0, 'Rust initial socket');
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
  const protocolErrors = tape.audit?.errors() ?? [];
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
      isInverse: (markets.flags[i] & 2) !== 0,
      isSpot: (markets.flags[i] & 4) !== 0,
      isExpiring: (markets.flags[i] & 16) !== 0,
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
    missingWrites: tape.missingWrites,
    invalidSignatures: tape.invalidSignatures,
    requests: [...tape.requests].sort(),
    writes: tape.writes,
    subscriptions: [...tape.subscriptions].sort(),
    unusedWrites: tape.unusedWrites(),
    protocolErrors,
    wsRequests: [...tape.sentRequests].sort(),
    store: { markets: marketRows, tickers: tickerRows },
    books: bookRows,
    account,
    commands: tape.results,
  };
}

const commandArgs = ({ t, type, expected, ...args }) => args;

async function tsCommand(exchange, store, record) {
  const { op, ...args } = commandArgs(record);
  switch (op) {
    case 'getAccount':
      return exchange.getAccount();
    case 'placeOrders':
      return exchange.placeOrders(args.orders);
    case 'cancelOrders':
      await exchange.cancelOrders(store.orders.filter((order) => args.ids.includes(order.id)));
      return [];
    case 'cancelSymbolOrders':
      if (args.symbol === '') await exchange.cancelAllOrders();
      else await exchange.cancelSymbolOrders(args.symbol);
      return [];
    case 'amendOrder': {
      const order = store.orders.find((o) => o.id === args.id);
      if (!order) throw new Error(values.venue === 'mexc' ? `Order ${args.id} not found` : `unknown order ${args.id}`);
      const update = Object.fromEntries(
        ['price', 'amount'].filter((key) => args[key] !== undefined).map((key) => [key, args[key]]),
      );
      const result = (await exchange.updateOrder({ order, update })) ?? [];
      return result.clientOrderId ? { orderIds: [...result], clientOrderId: result.clientOrderId } : result;
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
    case 'fetchIncome':
      return exchange.fetchIncomeHistory({
        symbol: args.symbol,
        incomeType: args.incomeType,
        startTime: args.from,
        endTime: args.to,
        limit: args.limit,
      });
    case 'fetchPublicTrades':
      return exchange.fetchPublicTrades(args.symbol, args.limit);
    case 'fetchCandles':
      return exchange.fetchOHLCV(args);
    default:
      throw new Error(`unknown command ${op}`);
  }
}

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
    if (venue.inverseNativeAmounts) row.extra = { remainingNative: Math.round(Math.max(0, row.amount - row.filled) * 1e12) / 1e12 };
    orderRows.push(row);
  }
  const positionRows = [];
  for (let i = 0; i < (positions?.count ?? 0); i++) {
    const row = { symbol: symbol(positions.market[i]), ...positionEnums(positions.enums[i]) };
    for (const column of POSITION_COLUMNS) row[column] = positions[column][i];
    if (venue.inverseNativeAmounts) row.extra = { contractsNative: row.notional, unrealizedPnlNative: row.unrealizedPnl };
    positionRows.push(row);
  }
  const settingRows = [];
  for (let i = 0; i < (settings?.count ?? 0); i++) {
    // TS has no margin-only placeholder; frames retain it for actual consumers.
    if (!venue.independentLeverage && !(settings.leverage[i] > 0)) continue;
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
  const heapBefore = await baseline();
  const tape = new Tape(records, venue, isPrivate, 30000, 'ts', bookSymbols, options);
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
  globalThis.fetch = async (input, init = {}) => {
    const request = input instanceof Request ? input : undefined;
    const method = init.method ?? request?.method ?? 'GET';
    const requestBody =
      init.body ??
      (request && !['GET', 'HEAD'].includes(method.toUpperCase()) ? await request.clone().text() : undefined);
    const { status, body } = tape.respond(method, request?.url ?? String(input), requestBody);
    return new Response([204, 205, 304].includes(status) ? null : body, { status });
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
    const response = {
      data: body,
      status,
      statusText: String(status),
      headers: Object.fromEntries(
        (responseHeaders ?? '')
          .split('\n')
          .filter(Boolean)
          .map((line) => {
            const colon = line.indexOf(':');
            return [line.slice(0, colon).trim().toLowerCase(), line.slice(colon + 1).trim()];
          }),
      ),
      config,
      request: {},
    };
    if (status >= 400) {
      const error = new Error(`Request failed with status code ${status}`);
      error.response = response;
      error.config = config;
      throw error;
    }
    return response;
  };
  const { ExchangeWorker } = await import('../../packages/safe-cex/src/webworker/ExchangeWorker.ts');
  const cpuStart = { process: process.cpuUsage(), thread: process.threadCpuUsage() };
  const wallStart = performance.now();
  const clock = isPrivate ? undefined : new ReplayClock(metadata?.epoch ?? Date.now());
  clock?.install();

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
  const exchange = new Exchange(venue.ts.name, { ...options, tealstreetTunnel: false }, store);
  // Model worker dispatch through the exchange's real local methods.
  exchange.emitter.on('callMethod', ({ method, args, requestId }) => {
    setImmediate(async () => {
      try {
        if (!['removeOrders', 'fetchAndSetOrders'].includes(method)) throw new Error(`Unsupported taped worker method ${method}`);
        const result = await exchange[method](...args);
        exchange.emitter.emit('callMethod:done', { requestId, success: true, result });
      } catch (error) {
        exchange.emitter.emit('callMethod:done', { requestId, success: false, result: { message: error.message } });
      }
    });
  });
  venue.configureTs?.(exchange);
  await venue.prepareTs?.(exchange, TapeWebSocket);
  const executions = [];
  exchange.emitter.on('fill', (fill) => executions.push(fill));
  for (const client of axiosClients(exchange)) client.defaults.adapter = adapter;
  const finalBooks = {};
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
    async (accountReady) => {
      await (accountReady ? Promise.all([started, subscribed]) : subscribed);
      venue.prepareTsReady?.(exchange, { isPrivate });
      await until(() => venue.tsBooksSubscribed(exchange, bookSymbols.length), 'TS book subscriptions');
      const loaded = () =>
        accountReady ? Object.values(store.loaded).every(Boolean) : store.loaded.markets && store.loaded.tickers;
      await until(() => !isPrivate || loaded(), `TS account startup (${JSON.stringify(store.loaded)})`);
      await until(() => tape.open.size > 0, 'TS initial socket');
    },
    (record) => tsCommand(exchange, store, record),
    clock,
    () => {
      if (isPrivate) clockStart = performance.now();
    },
  );
  await within(Promise.all([started, subscribed]), 15_000, 'TS startup/subscriptions');
  await within(venue.settleTs?.(exchange, { isPrivate, clock }) ?? Promise.resolve(), 15_000, 'TS pending venue reads');
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
          ...(venue.inverseNativeAmounts ? { coins: store.balance.coins ?? {} } : {}),
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
  const protocolErrors = tape.audit?.errors() ?? [];
  exchange.dispose();
  clock?.restore();
  return {
    frames,
    metrics,
    unserved: tape.unserved,
    missingWrites: tape.missingWrites,
    invalidSignatures: tape.invalidSignatures,
    requests: [...tape.requests].sort(),
    writes: tape.writes,
    subscriptions: [...tape.subscriptions].sort(),
    unusedWrites: tape.unusedWrites(),
    protocolErrors,
    wsRequests: [...tape.sentRequests].sort(),
    store: snapshot,
    books: bookCopies,
    account,
    commands: tape.results,
  };
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
    ...(venue.inverseNativeAmounts ? { remainingNative: o.extra?.remainingNative } : {}),
    ...(venue.compareClientIds && o.type === 'limit' ? { timeInForce: o.timeInForce ?? null } : {}),
    remaining: o.remaining,
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
    ...(venue.inverseNativeAmounts ? { contractsNative: p.extra?.contractsNative, unrealizedPnlNative: p.extra?.unrealizedPnlNative } : {}),
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
      .map((s) => ({
        symbol: s.symbol,
        marginType: s.marginType,
        ...(venue.independentLeverage
          ? { buyLeverage: s.buyLeverage, sellLeverage: s.sellLeverage }
          : { leverage: s.leverage }),
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
    missingWrites: result.missingWrites,
    invalidSignatures: result.invalidSignatures,
    requests: result.requests,
    writes: result.writes,
    writeCount: result.writes.length,
    subscriptions: result.subscriptions,
    unusedWrites: result.unusedWrites,
    protocolErrors: result.protocolErrors,
    wsRequests: result.wsRequests,
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
    commands: result.commands.map((result, index) => venue.commandResult?.(result,
      records.filter((record) => record.type === 'command')[index], index + 1) ?? result),
  }),
);
realClearTimeout(replayDeadline);
process.exit(0);
