// Replay a recorded tape through one implementation and report its final
// Store, order books, CPU time and memory as JSON on stdout.
//
//   node --expose-gc --import tsx sdk-rs/harness/replay.mjs --impl ts --tape <file>
//   node --expose-gc sdk-rs/harness/replay.mjs --impl rust --tape <file>
import { readFileSync } from 'node:fs';
import { createHmac, timingSafeEqual } from 'node:crypto';
import { parseArgs } from 'node:util';
import { gunzipSync } from 'node:zlib';
import { ORDER_COLUMNS, POSITION_COLUMNS, orderEnums, positionEnums } from '../hosts/wasm/js/wire.mjs';
import { axiosClients, venueOf } from './venues.mjs';
import { parseRequestJson } from './request-json.mjs';
import { retainedMemory } from './memory.mjs';
import { Responses } from './responses.mjs';

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
// A wrong startup URL, missing socket or lost command must fail the gate.
const replayDeadline = setTimeout(() => {
  console.error('Replay exceeded its 120-second deadline');
  process.exit(1);
}, 120_000);
replayDeadline.unref();
class ReplayTimeout extends Error {}
const within = async (promise, ms, label) => {
  let timer;
  try {
    return await Promise.race([promise, new Promise((_, reject) => {
      timer = setTimeout(() => reject(new ReplayTimeout(`${label} exceeded ${ms} ms`)), ms);
    })]);
  } finally { clearTimeout(timer); }
};
// A tape with a user stream replays an account: signed, paced in real time,
// and compared on its private state rather than its books.
const isPrivate = records.some((r) =>
  venue.isPrivateRecord ? venue.isPrivateRecord(r) : Boolean(venue.privateSocket && r.url?.includes(venue.privateSocket)),
);
const KEY = 'replay-key';
const SECRET = 'replay-secret';
const PASSPHRASE = 'replay-passphrase';

/** Per-request values: clocks, signatures, request ids and generated client ids. */
const callerIds = new Set(records.flatMap((r) => r.type === 'command' ? (r.orders ?? []).map((o) => o.clientOrderId).filter(Boolean) : []));
const VOLATILE = new Set(['timestamp', 'recvWindow', 'signature', 'newClientOrderId', 'clientAlgoId', 'requestId']);
const canonicalUrl = (url) => {
  const parsed = new URL(url);
  const query = [...parsed.searchParams]
    .filter(([name, value]) => (!VOLATILE.has(name) || callerIds.has(value)) && (callerIds.has(value) || !venue.generatedQueryId?.(name, value)))
    .map(([name, value]) => [name, /^[\[{]/.test(value) ? canonicalBody(value) : value])
    .sort(([a], [b]) => a.localeCompare(b));
  return `${parsed.origin}${parsed.pathname}?${new URLSearchParams(query)}`;
};

/**
 * A JSON request body with its keys sorted and generated fields dropped, so
 * venues that sign headers and send parameters in the body (Bybit) key their
 * writes by what they ask for. Caller-supplied IDs remain part of the match.
 * Empty for a body-less request.
 */
const canonicalBody = (body) => {
  if (body === undefined || body === null || body === '') return '';
  let value = body;
  try {
    if (typeof body === 'string') value = parseRequestJson(body);
    else if (ArrayBuffer.isView(body) || body instanceof ArrayBuffer) value = parseRequestJson(new TextDecoder().decode(body));
  } catch { return String(body); }
  if (values.venue === 'cryptocom' && value?.method && value?.params) value = value.params;
  const sorted = (value) => {
    if (Array.isArray(value)) return value.map(sorted);
    if (value === null || typeof value !== 'object') return value;
    const keys = Object.keys(value).filter((key) => value[key] !== undefined
      && !(venue.volatileBodyFields && VOLATILE.has(key) && !callerIds.has(String(value[key])))).sort();
    return Object.fromEntries(keys.map((key) => [
      key,
      !callerIds.has(value[key]) && venue.generatedBodyId?.(key, value[key]) ? '<generated>' : sorted(value[key]),
    ]));
  };
  return JSON.stringify(sorted(value), (_key, value) => typeof value === 'bigint' ? { integer: String(value) } : value);
};
const requestKey = (method, url, body) =>
  `${method} ${canonicalUrl(url)}${venue.matchRequestBody && method !== 'GET' ? ` ${canonicalBody(body)}` : ''}`;

const isWriteRequest = (method, url) => venue.isWrite?.(method, url) ?? method !== 'GET';

/** Serves recorded HTTP responses and websocket frames in tape order. */
class Tape {
  constructor(impl) {
    this.audit = venue.createSocketAudit?.(impl, { isPrivate, books: bookSymbols, tickers: venue.publicTickers?.(records) });
    this.responses = new Responses(records, requestKey, venue.checkRequest, isWriteRequest);
    this.frames = records.filter((r) => r.type === 'ws' || r.type === 'command');
    this.open = new Map();
    this.connections = [];
    this.waiters = [];
    this.commandIndex = 0;
  }

  get unserved() { return this.responses.unserved; }

  respond(method, url, body, headers) {
    venue.validateRequest?.({ method, url, body, headers });
    const key = requestKey(method, url, body);
    const queue = venue.checkRequest?.(method, url, body) === false ? undefined : this.responses.queues.get(key);
    if (!queue?.length) {
      this.responses.unserved++;
      console.error(`Unserved request: ${key}`);
      return { status: 599, body: '' };
    }
    // Staged snapshots become available only once their command starts.
    const eligible = queue.filter((r) => (r.afterCommand ?? 0) <= this.commandIndex);
    if (!eligible.length) {
      this.responses.unserved++;
      console.error(`Request preceded its taped stage: ${key}`);
      return { status: 599, body: '' };
    }
    const stage = Math.max(...eligible.map((r) => r.afterCommand ?? 0));
    const current = eligible.filter((r) => (r.afterCommand ?? 0) === stage);
    // Timed read snapshots remain reusable templates for safety reconciliation.
    const timed = current.filter((r) => r.effectiveAt !== undefined && r.effectiveAt <= (this.time ?? 0));
    const timedRead = !isWriteRequest(method, url) && current.some((r) => r.effectiveAt !== undefined);
    const record = timedRead ? (timed.at(-1) ?? current[0]) : current[0];
    // Every mutation needs its own row; ordinary reads may reconcile repeatedly.
    if (!timedRead && (current.length > 1 || isWriteRequest(method, url))) queue.splice(queue.indexOf(record), 1);
    const sent = body ? (typeof body === 'string' ? parseRequestJson(body) : body) : {};
    const responseBody = record.body.replace(/"\$(clientAlgoId|newClientOrderId)"/g, (_, field) => JSON.stringify(sent[field] ?? null));
    return { status: record.status, headers: record.headers ?? '', body: responseBody };
  }

  connect(url, onOpen, onMessage) {
    const socket = { url, onMessage, closed: false, loginVerified: false, topics: new Map() };
    setImmediate(() => {
      if (socket.closed) return;
      // Bybit adds a generated requestId to each connection URL.
      this.open.set(canonicalUrl(url), socket);
      this.connections.push(socket);
      onOpen();
      this.waiters = this.waiters.filter((waiter) => !waiter());
    });
    return socket;
  }

  send(socket, text) {
    venue.onSocketSend?.(socket, text);
    this.audit?.sent(socket.url, text);
    if (venue.pingReply) {
      const reply = venue.pingReply(text);
      if (reply) setImmediate(() => { if (!socket.closed) socket.onMessage(reply); });
    }
    if (venue.respondToPing && text === 'ping') {
      setImmediate(() => { if (!socket.closed) socket.onMessage('pong'); });
      return;
    }
    // OKX's opt-in auth validation also uses its v5 subscription argument model.
    const validateTopics = venue.validateSubscriptions ?? venue.validateLogin;
    if (!venue.validateLogin && !validateTopics) return;
    const message = JSON.parse(text);
    if (validateTopics && ['subscribe', 'unsubscribe'].includes(message.op)) {
      if (!Array.isArray(message.args)) throw new Error(`Invalid subscription on ${socket.url}`);
      for (const topic of message.args) {
        if (!topic || typeof topic.channel !== 'string' || !topic.channel) throw new Error(`Invalid subscription topic on ${socket.url}`);
        const key = JSON.stringify(Object.fromEntries(Object.entries(topic).sort(([a], [b]) => a.localeCompare(b))));
        if (message.op === 'subscribe') socket.topics.set(key, topic);
        else socket.topics.delete(key);
      }
      return;
    }
    if (!venue.validateLogin || !/\/(private|business)$/.test(new URL(socket.url).pathname)) return;
    if (message.op !== 'login') return;
    const arg = message.args?.length === 1 ? message.args[0] : undefined;
    const timestamp = String(arg?.timestamp ?? '');
    if (arg?.apiKey !== KEY || arg?.passphrase !== PASSPHRASE || !/^\d+$/.test(timestamp)
      || !Number.isSafeInteger(Number(timestamp)) || Math.abs(Number(timestamp) - Date.now() / 1000) > 30) {
      throw new Error(`Invalid OKX login credentials/timestamp on ${socket.url}`);
    }
    const expected = createHmac('sha256', SECRET).update(`${timestamp}GET/users/self/verify`).digest('base64');
    const supplied = Buffer.from(String(arg.sign ?? ''));
    const expectedBytes = Buffer.from(expected);
    if (supplied.length !== expectedBytes.length || !timingSafeEqual(supplied, expectedBytes)) {
      throw new Error(`Invalid OKX login signature on ${socket.url}`);
    }
    socket.loginVerified = true;
  }

  assertSubscribed(socket, message) {
    if (!(venue.validateSubscriptions ?? venue.validateLogin) || !message.arg?.channel || !Array.isArray(message.data)) return;
    const topics = [...socket.topics.values()].filter((topic) => topic.channel === message.arg.channel);
    // Some documented messages put instId only on each data row, not arg.
    const rows = message.data.length ? message.data : [null];
    for (const row of rows) {
      const object = row && !Array.isArray(row) && typeof row === 'object' ? row : {};
      const covered = topics.some((topic) => Object.entries(topic).every(([key, value]) => {
        if (key === 'channel') return true;
        if (key === 'instType' && value === 'ANY') return true;
        const values = [message.arg[key], object[key]].filter((v) => v !== undefined && v !== '');
        if (!values.length && key === 'instType') {
          const id = object.instId ?? message.arg.instId;
          if (id?.endsWith('-SWAP')) values.push('SWAP');
        }
        return values.length > 0 && values.every((v) => v === value);
      }));
      if (!covered) {
        throw new Error(`Unsubscribed ${message.arg.channel} frame on ${socket.url}: ${JSON.stringify(message.arg)}${object.instId ? ` instId=${object.instId}` : ''}`);
      }
    }
  }

  assertWritesConsumed() {
    const missing = this.responses.unusedWrites();
    if (missing) throw new Error(`${missing} taped write(s) were not sent`);
  }

  async socketFor(rawUrl, text) {
    const url = canonicalUrl(rawUrl);
    if (venue.socketMatches && text !== undefined) {
      const find = () => this.connections.find((s) => !s.closed && canonicalUrl(s.url) === url && venue.socketMatches(s, text));
      const started = performance.now();
      while (!find()) {
        if (performance.now() - started > 15_000) throw new Error(`matching socket did not open: ${url}`);
        await new Promise((resolve) => setTimeout(resolve, 5));
      }
      return find();
    }
    while (!this.open.get(url) || this.open.get(url).closed) {
      let timer;
      try {
        await Promise.race([
          new Promise((resolve) => this.waiters.push(() => (this.open.get(url) && !this.open.get(url).closed ? (resolve(), true) : false))),
          new Promise((_, reject) => { timer = setTimeout(() => reject(new Error(`socket did not open: ${url}`)), 30_000); }),
        ]);
      } finally { clearTimeout(timer); }
    }
    return this.open.get(url);
  }

  /**
   * Deliver every frame in order, yielding to the event loop between frames.
   * Commands are awaited where they fall. A private tape is paced: each record
   * waits until its time after the account is ready.
   */
  async play(onTick, ready = async () => {}, runCommand = async () => {}) {
    const urls = [...new Set(this.frames.filter((f) => f.url).map((f) => f.url))];
    await within((async () => {
      for (const url of urls) await this.socketFor(url);
      for (const frame of this.frames.filter((f) => venue.bootstrapFrame?.(f))) {
        const socket = await this.socketFor(frame.url, frame.text);
        socket.onMessage(frame.text);
        await new Promise(setImmediate);
      }
      await ready();
    })(), 15_000, 'Replay startup');
    const start = performance.now();
    let delivered = 0;
    const warm = Math.floor(this.frames.length / 4);
    for (const frame of this.frames) {
      if (venue.bootstrapFrame?.(frame)) continue;
      if (delivered === warm) this.warmMark = { thread: process.threadCpuUsage(), wall: performance.now() };
      if (isPrivate) {
        const wait = frame.t - (performance.now() - start);
        if (wait > 0) await new Promise((resolve) => setTimeout(resolve, wait));
      }
      this.time = frame.t;
      if (frame.type === 'command') {
        this.commandIndex++;
        this.results.push(await within(runCommand(frame), 15_000, `Command ${frame.op}`).then((ids) => ({ ok: ids }), (error) => {
          if (error instanceof ReplayTimeout) throw error;
          return { error: String(error?.message ?? error) };
        }));
      } else {
        const socket = await this.socketFor(frame.url, frame.text);
        if ((venue.validateLogin || venue.validateSubscriptions) && frame.text.trimStart().startsWith('{')) {
          const message = JSON.parse(frame.text);
          if (venue.validateLogin && message.event === 'login' && String(message.code) === '0' && !socket.loginVerified) {
            throw new Error(`Taped login success preceded a valid login on ${frame.url}`);
          }
          this.assertSubscribed(socket, message);
        }
        socket.onMessage(frame.text);
      }
      delivered++;
      onTick?.(frame.t);
      await new Promise(setImmediate);
    }
    return delivered;
  }

  unusedWrites() {
    return this.responses.unusedWrites();
  }

  results = [];
}

const settle = async () => {
  for (let i = 0; i < 20; i++) await new Promise(setImmediate);
};

const measure = async (cpuStart, wallStart, heapBefore, wasmBytes = 0, warmMark) => {
  const cpu = process.cpuUsage(cpuStart.process);
  const thread = process.threadCpuUsage(cpuStart.thread);
  const steady = warmMark ? process.threadCpuUsage(warmMark.thread) : { user: 0, system: 0 };
  const wallMs = performance.now() - wallStart;
  const memory = await baseline();
  return {
    // The worker's own thread: what competes with its message handling.
    cpuMs: (thread.user + thread.system) / 1000,
    // Main thread after the first quarter of frames: startup and JIT excluded.
    steadyCpuMs: (steady.user + steady.system) / 1000,
    // All threads, including V8 background compilation and GC helpers.
    processCpuMs: (cpu.user + cpu.system) / 1000,
    wallMs,
    rssBytes: memory.rss,
    heapUsedBytes: memory.heapUsed,
    heapRetainedBytes: memory.heapUsed - heapBefore.heapUsed,
    bufferRetainedBytes: memory.arrayBuffers - heapBefore.arrayBuffers,
    buffersBefore: heapBefore.arrayBuffers,
    buffersAfter: memory.arrayBuffers,
    wasmBytes,
    // What the implementation keeps alive beyond the harness and loaded code.
    retainedBytes: retainedMemory(heapBefore, memory, wasmBytes),
  };
};

/** Loaded code is excluded; collect before I/O and after replay state settles. */
const baseline = async () => {
  let previous;
  let stable = 0;
  for (let i = 0; i < 8; i++) {
    globalThis.gc?.();
    await new Promise(setImmediate);
    const memory = process.memoryUsage();
    stable = memory.arrayBuffers === previous ? stable + 1 : 0;
    if (stable >= 2) return memory;
    previous = memory.arrayBuffers;
  }
  throw new Error('Backing-buffer memory did not settle');
};

const sortBook = (book) => ({
  bids: book.bids.map(({ price, amount }) => [price, amount]),
  asks: book.asks.map(({ price, amount }) => [price, amount]),
});

const tickerFields = ['bid', 'ask', 'last', 'mark', 'index', 'percentage', 'openInterest', 'fundingRate', 'fundingTime', 'volume', 'quoteVolume'];
const marketFields = (m) => ({
  id: m.id,
  symbol: m.symbol,
  base: m.base,
  quote: m.quote,
  active: m.active,
  contractSize: m.contractSize ?? null,
  precision: { amount: m.precision.amount, price: m.precision.price },
  limits: { amount: m.limits.amount },
});

async function runRust() {
  const { EngineHost } = await import('../hosts/wasm/js/host.mjs');
  const { Frame, TickerState, MARKET_COLUMNS } = await import('../hosts/wasm/js/wire.mjs');
  const { loadEngine } = await import('./engine.mjs');
  const { WasmEngine, wasmMemoryBytes } = await loadEngine();
  const tape = new Tape('rust');
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
    connect: (url, { headers, onOpen, onMessage }) => {
      venue.validateSocket?.(url, headers);
      const socket = tape.connect(url, onOpen, (text) => { tape.audit?.received(url, text); onMessage(text); });
      return { send: (text) => tape.send(socket, text), close: () => (socket.closed = true) };
    },
    request: async ({ method, url, body: requestBody, headers }) => {
      const { status, headers: responseHeaders = '', body } = tape.respond(method, url, requestBody, headers);
      // Model nativeTransport's chunks so streamed lists do not allocate a whole
      // response in wasm before the venue sees the first item.
      const chunks = async function* () {
        const bytes = encoder.encode(body);
        for (let offset = 0; offset < bytes.length; offset += 16 * 1024) {
          yield bytes.subarray(offset, offset + 16 * 1024);
        }
      };
      return { status, headers: responseHeaders, body: chunks() };
    },
    setTimeout: (fn, ms) => setTimeout(fn, ms),
    clearTimeout: (handle) => clearTimeout(handle),
  };
  const options = { ...venue.options, ...(isPrivate ? { key: KEY, secret: SECRET, passphrase: PASSPHRASE } : { publicDataOnly: true }) };
  const host = new EngineHost(new WasmEngine(venue.rust, JSON.stringify(options)), transport, {
    onLog: (level, message) => console.error(`[${level}] ${message}`),
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
        fills.push({ market: executed.market[i], side: executed.side[i], amount: executed.rows[i * 2 + 1], price: executed.rows[i * 2], id: executed.orderId(i) });
      }
    },
  });
  const heapBefore = await baseline();
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
      const waiting = performance.now();
      while ((isPrivate && (status?.loaded ?? 0) !== allLoaded) || (tape.audit && !tape.audit.ready())) {
        if (performance.now() - waiting > 10_000) throw new Error(`Rust readiness incomplete: loaded=${status?.loaded}, protocol=${JSON.stringify(tape.audit?.errors())}`);
        await new Promise((r) => setTimeout(r, 5));
      }
    },
    (record) => {
      const { op, ...rest } = record;
      return host.command({ op, ...commandArgs(rest) });
    },
  );
  await settle();
  tape.assertWritesConsumed();
  clearInterval(flushTimer);
  host.flush();
  const metrics = await measure(cpuStart, wallStart, heapBefore, wasmMemoryBytes(), tape.warmMark);
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
  return { frames, metrics, unserved: tape.unserved, unusedWrites: tape.unusedWrites(), protocolErrors: tape.audit?.errors() ?? [], store: { markets: marketRows, tickers: tickerRows }, books: bookRows, account, commands: tape.results };
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
    fills: fills.map((f) => ({ id: f.id, symbol: symbol(f.market), side: f.side === 0 ? 'buy' : 'sell', amount: f.amount, ...(venue.compareFillPrices ? { price: f.price } : {}) })),
  };
}

async function runTs() {
  const tape = new Tape('ts');
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
          tape.audit?.received(url, data);
          for (const fn of this.listeners.message) fn({ data });
          this.onmessage?.({ data });
          venue.afterTsFrame?.(exchange);
        },
      );
    }
    addEventListener(type, fn) {
      this.listeners[type]?.push(fn);
    }
    removeEventListener(type, fn) {
      this.listeners[type] = (this.listeners[type] ?? []).filter((f) => f !== fn);
    }
    send(text) { tape.send(this.socket, text); }
    close() {
      this.readyState = 3;
      this.socket.closed = true;
    }
  }
  globalThis.WebSocket = TapeWebSocket;
  const { [venue.ts.className]: Exchange } = await import(venue.ts.module);
  const { DefaultStore } = await import('../../packages/safe-cex/src/store/store.base.ts');
  const adapter = async (config) => {
    const base = config.baseURL ?? '';
    const url = new URL(config.url.startsWith('http') ? config.url : `${base}${config.url}`);
    for (const [key, value] of Object.entries(config.params ?? {})) {
      if (value !== undefined) url.searchParams.set(key, String(value));
    }
    const { status, body, headers: responseHeaders } = tape.respond((config.method ?? 'get').toUpperCase(), url.toString(), config.data, config.headers?.toJSON?.() ?? config.headers);
    // The raw body: the client's own transformResponse parses it, big ids included.
    const response = { data: body, status, statusText: String(status), headers: Object.fromEntries((responseHeaders ?? '').split('\n').filter(Boolean).map((line) => { const colon = line.indexOf(':'); return [line.slice(0, colon).trim().toLowerCase(), line.slice(colon + 1).trim()]; })), config, request: {} };
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
  store.subscribe((data) => { state = data; });
  const postStore = () => {
    if (!values['no-egress'] && state) mainStore.update(structuredClone(worker._prepareStorePayload(state)), true);
  };
  const options = { ...venue.options, ...(isPrivate ? { key: KEY, secret: SECRET, passphrase: PASSPHRASE } : { key: '', secret: '', publicDataOnly: true }) };
  const exchange = new Exchange(venue.ts.name, options, store);
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
      venue.prepareTsReady?.(exchange, { isPrivate });
      while (!venue.tsBooksSubscribed(exchange, bookSymbols.length)) await new Promise((r) => setTimeout(r, 5));
      const loaded = () => Object.values(store.loaded).every(Boolean);
      while (isPrivate && !loaded()) await new Promise((r) => setTimeout(r, 5));
    },
    (record) => runTsCommand(exchange, store, record),
  );
  await within(Promise.all([started, subscribed]), 15_000, 'TS startup/subscriptions');
  if (venue.tsReplaySettled) await within((async () => {
    while (!venue.tsReplaySettled(exchange, bookSymbols.length)) await new Promise((r) => setTimeout(r, 5));
  })(), 15_000, 'TS pending book snapshots');
  await settle();
  tape.assertWritesConsumed();
  postStore();
  if (!values['no-egress']) for (const [symbol, book] of Object.entries(finalBooks)) mainBooks[symbol] = structuredClone(book);
  const metrics = await measure(cpuStart, wallStart, heapBefore, 0, tape.warmMark);
  const snapshot = {
    markets: (values['no-egress'] ? store : mainStore).markets.map((m) => ({ ...m })),
    tickers: (values['no-egress'] ? store : mainStore).tickers.map((t) => ({ ...t })),
  };
  const bookCopies = Object.fromEntries(
    Object.entries(values['no-egress'] ? finalBooks : mainBooks).map(([symbol, book]) => [symbol, JSON.parse(JSON.stringify(book))]),
  );
  const account = isPrivate
    ? {
        hedged: Boolean(store.options.isHedged),
        balance: { used: store.balance.used, free: store.balance.free, total: store.balance.total, upnl: store.balance.upnl },
        orders: store.orders.map((o) => ({ ...o })),
        // Zero-size placeholders carry each market's leverage and margin type.
        positions: store.positions.filter((p) => p.contracts > 0).map((p) => ({ ...p })),
        settings: [...new Map(store.positions.filter((p) => p.leverage > 0).map((p) => [p.symbol, { symbol: p.symbol, leverage: p.leverage, marginType: p.marginType }])).values()],
        fills: executions.map((f) => ({ id: f.orderId ?? f.id, symbol: f.symbol, side: f.side, amount: f.amount, ...(venue.compareFillPrices ? { price: f.price } : {}) })),
      }
    : undefined;
  exchange.dispose();
  return { frames, metrics, unserved: tape.unserved, unusedWrites: tape.unusedWrites(), protocolErrors: tape.audit?.errors() ?? [], store: snapshot, books: bookCopies, account, commands: tape.results };
}

async function runTsCommand(exchange, store, record) {
  const { op, ...args } = commandArgs(record);
  switch (op) {
    case 'placeOrders': return exchange.placeOrders(args.orders);
    case 'cancelOrders':
      await exchange.cancelOrders(store.orders.filter((order) => args.ids.includes(order.id)));
      return [];
    case 'cancelSymbolOrders': await exchange.cancelSymbolOrders(args.symbol); return [];
    case 'amendOrder': {
      const order = store.orders.find((o) => o.id === args.id);
      if (!order) throw new Error(`unknown order ${args.id}`);
      const update = Object.fromEntries(['price', 'amount'].filter((key) => args[key] !== undefined).map((key) => [key, args[key]]));
      return exchange.updateOrder({ order, update });
    }
    case 'setLeverage': await exchange.setLeverage(args.symbol, args.leverage); return [];
    case 'setMarginType': await exchange.changeMarginType(args.isolated, args.symbol); return [];
    case 'setPositionMode': await exchange.changePositionMode(args.hedged); return [];
    case 'fetchFills': return exchange.fetchHistoricalFills(args);
    case 'fetchOrderHistory': return exchange.fetchHistoricalOrders(args);
    case 'fetchIncome': {
      const { from, to, ...options } = args;
      return exchange.fetchIncomeHistory({ ...options, startTime: from, endTime: to });
    }
    case 'fetchPublicTrades': return exchange.fetchPublicTrades(args.symbol, args.limit);
    case 'fetchCandles': return exchange.fetchOHLCV(args);
    default: throw new Error(`unknown command ${op}`);
  }
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
    unusedWrites: result.unusedWrites,
    protocolErrors: result.protocolErrors,
    metrics: result.metrics,
    markets: result.store.markets.map(marketFields),
    tickers: result.store.tickers.map((t) => Object.fromEntries([['symbol', t.symbol], ...tickerFields.map((f) => [f, f === 'openInterest' ? (t[f] ?? 0) : t[f]])])),
    books: Object.fromEntries(Object.entries(result.books).map(([s, b]) => [s, sortBook(b)])),
    account: result.account && normalizeAccount(result.account),
    commands: result.commands,
  }),
);
clearTimeout(replayDeadline);
process.exit(0);
