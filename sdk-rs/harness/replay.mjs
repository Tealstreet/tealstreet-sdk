// Replay a recorded tape through one implementation and report its final
// Store, order books, CPU time and memory as JSON on stdout.
//
//   node --expose-gc --import tsx sdk-rs/harness/replay.mjs --impl ts --tape <file>
//   node --expose-gc sdk-rs/harness/replay.mjs --impl rust --tape <file>
import { readFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { createReplayEntropy } from './replay-entropy.mjs';
import { prepareRecordedHttp } from './recorded-http.mjs';
import { parseArgs } from 'node:util';
import { gunzipSync } from 'node:zlib';
import { NONE, ORDER_COLUMNS, POSITION_COLUMNS, orderEnums, positionEnums } from '../hosts/wasm/js/wire.mjs';
import { axiosClients, venueOf } from './venues.mjs';
import { Tape, replayStoreReady, ReplayClock } from './tape.mjs';
import { setTimeout as realTimeout, clearTimeout as realClearTimeout } from 'node:timers';
import { idleLoadedBaseline, measureBeforeCapture, retainedMemory, settledMemory } from './memory.mjs';
import { currencyBalances, lookupResult } from './account-shapes.mjs';
import { captureReplayState, finalizeReplay } from './replay-finalize.mjs';
import { credentialValues, findNoticeLeaks } from './notice-leaks.mjs';
import { continuousReplayStartup, deterministicReplay, frozenReplayStartup, replayTransportReady } from './polling.mjs';
import { normalizeAccount, comparesField, captureCheckpoint, captureTsFill, captureRustFill, duplicateSymbolErrors, flatPositionPnl } from './comparison-policy.mjs';

const { values } = parseArgs({
  options: {
    impl: { type: 'string' },
    performance: { type: 'boolean', default: false },
    venue: { type: 'string', default: 'binance' },
    tape: { type: 'string' },
    books: { type: 'string', default: 'BTCUSDT,ETHUSDT,SOLUSDT' },
    out: { type: 'string' },
    // Node engine package to load instead of the default build.
    wasm: { type: 'string' },
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
// Begin the signing clock after module/crypto setup, before venue startup.
// Keep this wall clock continuous through startup: venue clocks may measure
// elapsed real Date/dayjs time since their initial sync. Rebasing only Date.now
// at readiness would move their signatures ahead of the freshness oracle.
// Tape.play owns a separate readiness-relative clock for record scheduling.
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
if (values.performance && isPrivate) throw new Error('Performance mode requires a public tape; private tapes gate correctness only');
const options = {
  ...venue.options,
  ...metadata?.options,
  ...(typeof metadata?.testnet === 'boolean' ? { testnet: metadata.testnet } : {}),
  ...(isPrivate
    ? { key: 'replay-key', secret: 'replay-secret', passphrase: 'replay-passphrase', ...venue.replayCredentials }
    : { key: '', secret: '', publicDataOnly: true, ...venue.publicOptions }),
  ...config.options,
};
const initializeReplay = async () => {
  // Public performance loads production implementations cold. These harness
  // initializers construct independent signing verifiers (or return early for
  // public accounts); running them would preload dependencies such as ethers.
  if (values.performance) return;
  await venue.prepareReplay?.({ isPrivate, options });
  await venue.initializeReplay?.(values.impl, options, {
    isPrivate, tapeSha256: createHash('sha256').update(tapeBytes).digest('hex'),
  });
};

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

const measure = async (cpuStart, wallStart, heapBefore, wasmMemory = 0, warmMark, wasmBeforeBytes = 0, loadMetrics) => {
  const cpu = process.cpuUsage(cpuStart.process);
  const thread = process.threadCpuUsage(cpuStart.thread);
  const steady = warmMark ? process.threadCpuUsage(warmMark.thread) : { user: 0, system: 0 };
  const runtimeWallMs = performance.now() - wallStart;
  const runtimeCpuMs = (thread.user + thread.system) / 1000;
  const runtimeProcessCpuMs = (cpu.user + cpu.system) / 1000;
  const memory = await baseline();
  const wasmBytes = typeof wasmMemory === 'function' ? wasmMemory() : wasmMemory;
  return {
    baselineKind: 'idle-loaded',
    measuredMode: values.performance ? 'performance' : 'correctness',
    // Cold implementation loading plus runtime main-thread work. Baseline GC
    // is excluded as a separate interval; neither import nor WASM compile is.
    cpuMs: loadMetrics.idleLoadCpuMs + runtimeCpuMs,
    runtimeCpuMs,
    // Main thread after the first quarter of frames: startup and JIT excluded.
    steadyCpuMs: (steady.user + steady.system) / 1000,
    // All threads across both intervals, including background compilation.
    processCpuMs: loadMetrics.idleLoadProcessCpuMs + runtimeProcessCpuMs,
    runtimeProcessCpuMs,
    ...loadMetrics,
    runtimeWallMs,
    wallMs: loadMetrics.idleLoadWallMs + runtimeWallMs,
    rssBytes: memory.rss,
    heapBeforeBytes: heapBefore.heapUsed,
    heapUsedBytes: memory.heapUsed,
    heapRetainedBytes: memory.heapUsed - heapBefore.heapUsed,
    bufferRetainedBytes: memory.arrayBuffers - heapBefore.arrayBuffers,
    buffersBefore: heapBefore.arrayBuffers,
    buffersAfter: memory.arrayBuffers,
    wasmBytes,
    wasmBeforeBytes,
    wasmRetainedBytes: wasmBytes - wasmBeforeBytes,
    // Runtime retention above the idle-loaded implementation, including egress.
    retainedBytes: retainedMemory(heapBefore, memory, wasmBytes, wasmBeforeBytes),
  };
};

/** Settle the loaded implementation and then its live runtime state symmetrically. */
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
  ...(venue.compareInstrumentClassification ? { classification: {
    symbolType: m.info?.symbolType,
    marketRegion: m.info?.marketRegion,
    underlyingTicker: m.info?.underlyingTicker,
  } } : {}),
  ...(venue.compareOptionTerms ? { option: m.option ?? null } : {}),
});

async function runRust() {
  const { memory: heapBefore, loadMetrics, loaded: { EngineHost, Frame, TickerState, MARKET_COLUMNS, WasmEngine, wasmMemoryBytes, tape, encoder } } = await idleLoadedBaseline({
    baseline,
    load: async () => {
      await initializeReplay();
      const { EngineHost } = await import('../hosts/wasm/js/host.mjs');
      const { Frame, TickerState, MARKET_COLUMNS } = await import('../hosts/wasm/js/wire.mjs');
      const { loadEngine } = await import('./engine.mjs');
      const { WasmEngine, wasmMemoryBytes } = await loadEngine(values.wasm);
      const tape = new Tape(records, venue, isPrivate, 30000, 'rust', bookSymbols, options, { performance: values.performance });
      const encoder = new TextEncoder();
      return { EngineHost, Frame, TickerState, MARKET_COLUMNS, WasmEngine, wasmMemoryBytes, tape, encoder };
    },
  });
  const wasmBeforeBytes = wasmMemoryBytes();
  const cpuStart = { process: process.cpuUsage(), thread: process.threadCpuUsage() };
  const wallStart = performance.now();

  clockStart = performance.now();
  const clock = deterministicReplay(venue, isPrivate) ? new ReplayClock(metadata?.epoch ?? Date.now(), frozenReplayStartup(venue, isPrivate), continuousReplayStartup(venue, isPrivate)) : undefined;
  clock?.install();
  // What the main thread keeps: the latest market table, ticker columns, books.
  let markets;
  let classifications;
  let derivativeProducts;
  let positionGreeks;
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
    prepareHttp: prepareRecordedHttp,
    connect: (url, { headers, onOpen, onMessage, onBinary, onClose }) => {
      if (!values.performance) venue.validateSocket?.(url, headers);
      const socket = tape.connect(
        url,
        onOpen,
        (data) => (typeof data === 'string' ? onMessage(data) : onBinary?.(data)),
        onClose,
      );
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
    for (const method of [
      'start',
      'httpStart',
      'httpChunk',
      'httpEnd',
      'wsMessage',
      'wsMessageActions',
      'wsMessageBytes',
      'takeFrame',
    ]) {
      if (typeof engine[method] !== 'function') continue;
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
  // Private tapes run with credentials; every log line is checked for them.
  const notices = [];
  const host = new EngineHost(engine, transport, {
    ...(isPrivate ? { onLog: (_level, message) => notices.push(message) } : {}),
    onFrame: (buffer) => {
      // Model postMessage(buffer, [buffer]) to main, then main reading it.
      const received = values['no-egress'] ? buffer : structuredClone(buffer, { transfer: [buffer] });
      const frame = new Frame(received);
      const marketUpdate = frame.markets();
      if (marketUpdate) {
        markets = marketUpdate;
        classifications = frame.instrumentClassifications();
        derivativeProducts = frame.derivativeProducts();
      }
      tickers.apply(frame);
      for (const book of frame.books()) books.set(book.market, book);
      orders = frame.orders() ?? orders;
      const positionUpdate = frame.positions();
      if (positionUpdate) {
        positions = positionUpdate;
        positionGreeks = frame.positionGreeks();
      }
      balance = frame.balance() ?? balance;
      settings = frame.settings() ?? settings;
      status = frame.status() ?? status;
      const executed = frame.fills();
      for (let i = 0; i < (executed?.count ?? 0); i++) {
        fills.push(captureRustFill(executed, i));
      }
    },
  }, {
    entropy: createReplayEntropy(createHash('sha256').update(tapeBytes).digest('hex')),
  });
  host.start();
  for (const symbol of bookSymbols) host.command({ op: 'subscribeOrderBook', symbol });
  let lastFlush = -Infinity;
  const flushTimer = isPrivate ? setInterval(() => host.flush(), 250) : undefined;
  const capture = (frames = 0, final = false) => {
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
    for (let i = 0; i < (classifications?.count ?? 0); i++) {
      const row = marketRows[classifications.market[i]];
      if (!row) throw new Error('Classification has no native market');
      row.info = { symbolType: classifications.symbolType(i), marketRegion: classifications.marketRegion(i), underlyingTicker: classifications.underlyingTicker(i) };
    }
    for (let i = 0; i < (derivativeProducts?.count ?? 0); i++) {
      if (derivativeProducts.kind[i] === 1) continue;
      const row = marketRows[derivativeProducts.market[i]];
      if (!row) throw new Error('Derivative terms have no native market');
      const expiry = derivativeProducts.expiry[i];
      const strike = derivativeProducts.strike[i];
      const style = derivativeProducts.style(i);
      const settlement = derivativeProducts.settlement(i);
      const premiumAsset = derivativeProducts.premiumAsset(i);
      const settleAsset = derivativeProducts.settleAsset(i);
      if (Number.isFinite(expiry) && Number.isFinite(strike) && style === 'european' && settlement === 'cash' && premiumAsset && settleAsset) {
        row.option = { underlying: derivativeProducts.underlying(i), expiry, strike, right: derivativeProducts.kind[i] === 2 ? 'call' : 'put', style, settlement, premiumAsset, settleAsset };
      }
    }
    const tickerRows = [];
    for (let i = 0; i < tickers.count; i++) {
      const row = {
        symbol: tickers.market[i] === NONE ? tickers.identity?.(i).symbol : markets.symbol(tickers.market[i]),
      };
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
    const account = isPrivate ? rustAccount({ markets, orders, positions, positionGreeks, balance, settings, status, fills }) : undefined;
    return captureReplayState({
      frames,
      unserved: tape.unserved,
      missingWrites: tape.missingWrites,
      invalidSignatures: tape.invalidSignatures,
      requests: [...tape.requests].sort(),
      writes: tape.writes,
      subscriptions: [...tape.subscriptions].sort(),
      unusedWrites: tape.unusedWrites(),
      wsRequests: [...tape.sentRequests].sort(),
      store: { markets: marketRows, tickers: tickerRows },
      books: bookRows,
      account,
      commands: tape.results,
      notices,
      transportReceipt: tape.transportReceipt(),
    }, tape.audit, { final });
  };
  const checkpoints = [];
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
        return replayStoreReady(status?.loaded ?? 0, isPrivate && accountReady) && (!tape.audit || tape.audit.ready(accountReady));
      }, 'Rust startup');
      await until(() => replayTransportReady(venue, tape.open.size), 'Rust initial transport');
    },
    (record) => {
      const { op, ...rest } = record;
      return host.command({ op, ...commandArgs(rest) });
    },
    clock,
    undefined,
    values.performance ? undefined : (point) => {
      host.flush();
      checkpoints.push(captureCheckpoint(point, () => comparisonSnapshot(capture(point.ordinal))));
    },
  );
  await settle();
  if (!values.performance) tape.assertWritesConsumed();
  clearInterval(flushTimer);
  host.flush();
  const finish = values.performance ? measureBeforeCapture : finalizeReplay;
  const result = await finish(() => ({ ...capture(frames, true), checkpoints }), () => measure(cpuStart, wallStart, heapBefore, wasmMemoryBytes, tape.warmMark, wasmBeforeBytes, loadMetrics));
  if (values.performance) tape.assertWritesConsumed();
  host.dispose();
  clock?.restore();
  return result;
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
      await exchange.setLeverage(args.symbol, args.leverage, args.side);
      return [];
    case 'setMarginType':
      await exchange.changeMarginType(args.isolated, args.symbol);
      return [];
    case 'setPositionMode':
      await exchange.changePositionMode(args.hedged, args.symbol);
      return [];
    case 'fetchStpMode':
      return exchange.fetchStpMode();
    case 'setStpMode':
      await exchange.changeStpMode(args.mode);
      return [];
    case 'findOrderByClientOrderId':
      return exchange.findOrderByClientOrderId(args.symbol, args.clientOrderId);
    case 'fetchLeverage':
      return exchange.fetchLeverageForSymbol(args.symbol);
    case 'fetchMaxLeverage':
      return exchange.fetchSymbolMaxLeverage(args.symbol);
    case 'fetchAccount':
      return exchange.getAccount();
    case 'fetchOrderBook':
      return exchange.fetchOrderBook(args.symbol, args.limit);
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
    case 'fetchOptionChain':
      return exchange.fetchOptionChain(args.query);
    case 'fetchOptionExpiries':
      return exchange.fetchOptionExpiries(args.underlying);
    case 'fetchAccountGreeks':
      return exchange.fetchAccountGreeks();
    case 'subscribeCandles':
      // Subscriptions are compared; live candle values are not replay output.
      exchange.listenOHLCV(args, () => {});
      return undefined;
    default:
      throw new Error(`unknown command ${op}`);
  }
}

/** Main-thread view of the account, rebuilt from the latest frame tables. */
function rustAccount({ markets, orders, positions, positionGreeks, balance, settings, status, fills }) {
  const symbol = (i) => markets.symbol(i);
  const orderRows = [];
  for (let i = 0; i < (orders?.count ?? 0); i++) {
    const row = {
      id: orders.id(i),
      ...(comparesField(values.venue, 'clientOrderId') ? { clientOrderId: orders.clientOrderId(i) ?? null } : {}),
      parentId: orders.parentId(i) ?? null,
      conditionalOrderPrice: null,
      symbol: symbol(orders.market[i]),
      ...orderEnums(orders.enums[i]),
    };
    for (const column of ORDER_COLUMNS) row[column] = orders[column][i];
    if (venue.coinNativeAmounts) row.extra = { remainingNative: orders.remainingNative?.[i] };
    if (venue.inverseNativeAmounts) row.extra = { remainingNative: Math.round(Math.max(0, row.amount - row.filled) * 1e12) / 1e12 };
    orderRows.push(row);
  }
  const positionRows = [];
  for (let i = 0; i < (positions?.count ?? 0); i++) {
    const row = { symbol: symbol(positions.market[i]), ...positionEnums(positions.enums[i]) };
    for (const column of POSITION_COLUMNS) row[column] = positions[column][i];
    const greekIndex = positionGreeks && Array.from({ length: positionGreeks.count }, (_, index) => index).find((index) => positionGreeks.positionId(index) === positions.id(i));
    if (greekIndex !== undefined) row.option = { greeks: { delta: positionGreeks.delta[greekIndex], gamma: positionGreeks.gamma[greekIndex], vega: positionGreeks.vega[greekIndex], theta: positionGreeks.theta[greekIndex], units: positionGreeks.units(greekIndex) } };
    if (venue.coinNativeAmounts) row.extra = { contractsNative: row.notional / markets.contractSize[positions.market[i]], unrealizedPnlNative: row.unrealizedPnl };
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
    balance: balance ? {
      used: balance.used,
      free: balance.free,
      total: balance.total,
      upnl: balance.upnl,
      ...((venue.inverseNativeAmounts || venue.coinNativeAmounts || venue.compareCoins || venue.compareBalanceCoins) ? { coins: balance.coins ?? {} } : {}),
      ...(venue.currencyBalances && balance.currencies !== undefined ? { currencies: balance.currencies } : {}),
      ...(venue.compareBalanceCurrencies ? { currencies: currencyBalances(balance.currencies) } : {}),
    } : null,
    orders: orderRows,
    positions: positionRows,
    settings: settingRows,
    fills: fills.map((f) => ({
      id: f.id,
      symbol: symbol(f.market),
      side: f.side === 0 ? 'buy' : 'sell',
      amount: f.amount,
      price: f.price,
      executionId: f.executionId ?? null,
      fee: f.fee ?? null,
      timestamp: f.timestamp ?? null,
    })),
  };
}

async function runTs() {
  const { memory: heapBefore, loadMetrics, loaded: { tape, TapeWebSocket, Exchange, DefaultStore, ExchangeWorker, adapter } } = await idleLoadedBaseline({
    baseline,
    load: async () => {
      await initializeReplay();
      const tape = new Tape(records, venue, isPrivate, 30000, 'ts', bookSymbols, options, { performance: values.performance });
      class TapeWebSocket {
        static CONNECTING = 0;
        static OPEN = 1;
        static CLOSING = 2;
        static CLOSED = 3;
        constructor(url, _protocols, options = {}) {
          if (!values.performance) venue.validateSocket?.(url, options.headers);
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
        const headers = Object.fromEntries(new Headers(init.headers ?? request?.headers).entries());
        const { status, body } = tape.respond(method, request?.url ?? String(input), requestBody, headers);
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
        const url = new URL(config.url.startsWith('http') ? config.url : `${base.replace(/\/+$/, '')}/${config.url.replace(/^\/+/, '')}`);
        const serializer = typeof config.paramsSerializer === 'function' ? config.paramsSerializer : config.paramsSerializer?.serialize;
        if (serializer) {
          for (const [key, value] of new URLSearchParams(serializer(config.params ?? {}))) url.searchParams.append(key, value);
        } else {
          for (const [key, value] of Object.entries(config.params ?? {})) {
            if (value !== undefined) url.searchParams.set(key, String(value));
          }
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
      await venue.loadTs?.({ isPrivate });
      return { tape, TapeWebSocket, Exchange, DefaultStore, ExchangeWorker, adapter };
    },
  });
  const cpuStart = { process: process.cpuUsage(), thread: process.threadCpuUsage() };
  const wallStart = performance.now();
  clockStart = performance.now();
  const clock = deterministicReplay(venue, isPrivate) ? new ReplayClock(metadata?.epoch ?? Date.now(), frozenReplayStartup(venue, isPrivate), continuousReplayStartup(venue, isPrivate)) : undefined;
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
        if (!['removeOrders', 'fetchAndSetOrders'].includes(method))
          throw new Error(`Unsupported taped worker method ${method}`);
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
  await venue.prepareTsTransport?.(exchange, adapter);
  const finalBooks = {};
  const started = exchange.start();
  const subscribed = (async () => {
    while (!store.markets.length || (venue.tsBooksReady && !venue.tsBooksReady(exchange, store)))
      await new Promise((r) => realTimeout(r, 1));
    for (const symbol of bookSymbols) exchange.listenOrderBook(symbol, (book) => (finalBooks[symbol] = book));
  })();
  // Model the worker posting each subscribed book to main every 250 ms.
  let lastFlush = -Infinity;
  const mainBooks = {};
  const capture = (frames = 0, final = false) => {
    const receiver = values['no-egress'] ? store : mainStore;
    const snapshot = {
      markets: receiver.markets.map((m) => ({ ...m })),
      tickers: receiver.tickers.map((t) => ({ ...t })),
    };
    const bookCopies = Object.fromEntries(
      Object.entries(values['no-egress'] ? finalBooks : mainBooks).map(([symbol, book]) => [
        symbol,
        JSON.parse(JSON.stringify(book)),
      ]),
    );
    const account = isPrivate
      ? {
          hedged: Boolean(receiver.options.isHedged),
          balance: {
            used: receiver.balance.used,
            free: receiver.balance.free,
            total: receiver.balance.total,
            upnl: receiver.balance.upnl,
            ...(venue.inverseNativeAmounts || venue.coinNativeAmounts || venue.compareCoins || venue.compareBalanceCoins ? { coins: receiver.balance.coins ?? {} } : {}),
            ...(venue.currencyBalances && receiver.balance.currencies !== undefined
              ? { currencies: receiver.balance.currencies }
              : {}),
            ...(venue.compareBalanceCurrencies ? {
              currencies: currencyBalances(venue.tsBalanceCurrencies?.(receiver.balance.currencies ?? {}) ?? receiver.balance.currencies),
            } : {}),
          },
          orders: receiver.orders.map((o) => ({ ...o })),
          // Zero-size placeholders carry each market's leverage and margin type.
          positions: receiver.positions.filter((p) => p.contracts > 0).map((p) => ({ ...p })),
          flatPositionPnl: flatPositionPnl(receiver.positions),
          settings: venue.tsSettings?.(exchange, store) ?? [
            ...new Map(
              receiver.positions
                .filter((p) => p.leverage > 0)
                .map((p) => [p.symbol, { symbol: p.symbol, leverage: p.leverage, marginType: p.marginType }]),
            ).values(),
          ],
          fills: executions.map(captureTsFill),
        }
      : undefined;
    return captureReplayState({
      frames,
      unserved: tape.unserved,
      missingWrites: tape.missingWrites,
      invalidSignatures: tape.invalidSignatures,
      requests: [...tape.requests].sort(),
      writes: tape.writes,
      subscriptions: [...tape.subscriptions].sort(),
      unusedWrites: tape.unusedWrites(),
      wsRequests: [...tape.sentRequests].sort(),
      store: snapshot,
      books: bookCopies,
      account,
      commands: tape.results,
      transportReceipt: tape.transportReceipt(),
    }, tape.audit, { final });
  };
  const checkpoints = [];
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
      await (accountReady || venue.polling ? Promise.all([started, subscribed]) : subscribed);
      await venue.prepareTsReady?.(exchange, { isPrivate });
      await until(() => venue.tsBooksSubscribed(exchange, bookSymbols.length), 'TS book subscriptions');
      const loaded = () =>
        accountReady ? Object.values(store.loaded).every(Boolean) : store.loaded.markets && store.loaded.tickers;
      await until(() => !(isPrivate || venue.polling) || loaded(), `TS account startup (${JSON.stringify(store.loaded)})`);
      await until(() => replayTransportReady(venue, tape.open.size), 'TS initial transport');
      await until(() => !tape.audit || tape.audit.ready(accountReady), 'TS protocol readiness');
    },
    (record) => tsCommand(exchange, store, record),
    clock,
    undefined,
    values.performance ? undefined : (point) => {
      postStore();
      if (!values['no-egress'])
        for (const [symbol, book] of Object.entries(finalBooks)) mainBooks[symbol] = structuredClone(book);
      checkpoints.push(captureCheckpoint(point, () => comparisonSnapshot(capture(point.ordinal))));
    },
  );
  await within(Promise.all([started, subscribed]), 15_000, 'TS startup/subscriptions');
  if (venue.tsReplaySettled)
    await until(() => venue.tsReplaySettled(exchange, bookSymbols.length), 'TS pending book snapshots');
  await within(venue.settleTs?.(exchange, { isPrivate, clock }) ?? Promise.resolve(), 15_000, 'TS pending venue reads');
  await settle();
  if (!values.performance) tape.assertWritesConsumed();
  postStore();
  if (!values['no-egress'])
    for (const [symbol, book] of Object.entries(finalBooks)) mainBooks[symbol] = structuredClone(book);
  const finish = values.performance ? measureBeforeCapture : finalizeReplay;
  const result = await finish(() => ({ ...capture(frames, true), checkpoints }), () => measure(cpuStart, wallStart, heapBefore, 0, tape.warmMark, 0, loadMetrics));
  if (values.performance) tape.assertWritesConsumed();
  exchange.dispose();
  clock?.restore();
  return result;
}


function comparisonSnapshot(result) {
  return {
    duplicateSymbols: duplicateSymbolErrors(result.store),
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
    account: result.account && normalizeAccount(result.account, venue, values.impl, values.venue),
  };
}

const result = values.impl === 'ts' ? await runTs() : await runRust();
if (result.notices) {
  const leaks = findNoticeLeaks([...result.notices, ...result.commands], credentialValues(options));
  if (leaks.length) {
    for (const leak of leaks) console.error(leak);
    realClearTimeout(replayDeadline);
    process.exit(1);
  }
}
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
    transportReceipt: result.transportReceipt,
    ...comparisonSnapshot(result),
    checkpoints: result.checkpoints,
    commands: result.commands.map((result, index) => {
      const command = records.filter((record) => record.type === 'command')[index];
      return lookupResult(venue.commandResult?.(result, command, index + 1) ?? result, command);
    }),
  }),
);
realClearTimeout(replayDeadline);
process.exit(0);
