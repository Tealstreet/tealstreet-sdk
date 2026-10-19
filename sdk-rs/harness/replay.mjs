// Replay a recorded tape through one implementation and report its final
// Store, order books, CPU time and memory as JSON on stdout.
//
//   node --expose-gc --import tsx sdk-rs/harness/replay.mjs --impl ts --tape <file>
//   node --expose-gc sdk-rs/harness/replay.mjs --impl rust --tape <file>
import { readFileSync } from 'node:fs';
import { createHash, createHmac, timingSafeEqual } from 'node:crypto';
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
const callerIds = new Set(records.flatMap((r) => r.type === 'command' ? (r.orders ?? []).map((o) => o.clientOrderId).filter(Boolean) : []));

/** Per-request clocks, signatures, connection IDs and generated client IDs. */
const VOLATILE = new Set(['timestamp', 'recvWindow', 'signature', 'requestId']);
const clientFields = new Set(['newClientOrderId', 'clientAlgoId', 'clientId']);
const canonicalObject = (value) => {
  if (Array.isArray(value)) return value.map(canonicalObject);
  if (!value || typeof value !== 'object') return value;
  return Object.fromEntries(Object.entries(value).filter(([, v]) => v !== undefined)
    .sort(([a], [b]) => a.localeCompare(b)).map(([key, v]) => [key, canonicalObject(v)]));
};
const canonicalBody = (body) => {
  if (body == null || body === '') return '';
  let value = body;
  try {
    if (typeof body === 'string') value = parseRequestJson(body);
    else if (ArrayBuffer.isView(body) || body instanceof ArrayBuffer) value = parseRequestJson(new TextDecoder().decode(body));
  } catch { return String(body); }
  if (values.venue === 'cryptocom' && value?.method && value?.params) value = value.params;
  const sorted = (value) => {
    if (Array.isArray(value)) return value.map(sorted);
    if (!value || typeof value !== 'object') return value;
    return Object.fromEntries(Object.entries(value).filter(([key, v]) => v !== undefined &&
      (!clientFields.has(key) || callerIds.has(v)) && !(venue.volatileBodyFields && VOLATILE.has(key)))
      .sort(([a], [b]) => a.localeCompare(b)).map(([key, v]) => [key,
        !callerIds.has(v) && venue.generatedBodyId?.(key, v) ? '<generated>' : sorted(v),
      ]));
  };
  return JSON.stringify(sorted(value), (_key, value) => typeof value === 'bigint' ? { integer: String(value) } : value);
};
const canonicalUrl = (url) => {
  const parsed = new URL(url);
  const query = [...parsed.searchParams]
    .filter(([name, value]) => !VOLATILE.has(name) && (!clientFields.has(name) || callerIds.has(value)))
    .map(([name, value]) => [name, value.startsWith('[') || value.startsWith('{') ? canonicalBody(value) : value])
    .sort(([a], [b]) => a.localeCompare(b));
  return `${parsed.origin}${parsed.pathname}?${new URLSearchParams(query)}`;
};
const requestKey = (method, url, body) => `${method} ${canonicalUrl(url)} ${canonicalBody(body)}`;
const apiSupported = (value) => value == null || value === '' ? true
  : typeof value === 'number' ? value !== 0
  : typeof value === 'boolean' ? value
  : !['false', '0', 'no', 'n', 'disabled', 'unsupported'].includes(String(value).trim().toLowerCase());

/** Serves recorded HTTP responses and websocket frames in tape order. */
class Tape {
  constructor(impl) {
    this.audit = venue.createSocketAudit?.(impl, { isPrivate, books: bookSymbols, tickers: venue.publicTickers?.(records) });
    this.responses = new Responses(records, requestKey, venue.checkRequest, venue.isWrite,
      venue.matchRequestBody ? undefined : (method, url) => requestKey(method, url));
    this.frames = records.filter((r) => ['ws', 'command', 'open', 'close'].includes(r.type)).map((r) => {
      if (r.url) r = { ...r, url: canonicalUrl(r.url) };
      if (r.type !== 'ws') return r;
      const m = r.text.trimStart().startsWith('{') ? JSON.parse(r.text) : {};
      const topic = m.channel ? { channel: m.channel } : m.ch ? (m.symbol ? { ch: m.ch, symbol: m.symbol } : { ch: m.ch }) : undefined;
      return { ...r, topic: topic && JSON.stringify(canonicalObject(topic)), channel: m.ch, subscriptionKey: m.arg && JSON.stringify(canonicalObject(m.arg)) };
    });
    this.sockets = [];
    this.connectionCounts = new Map();
    this.byConnection = new Map();
    this.commandIndex = 0;
  }

  get unserved() { return this.responses.unserved; }

  time = 0;
  outbound = [];
  subscriptions = [];
  socketErrors = [];
  signatureErrors = [];

  send(socket, text) {
    const { url, connection } = socket;
    this.audit?.sent(socket.rawUrl, text);
    if (venue.respondToPing && text === 'ping') {
      setImmediate(() => { if (!socket.closed) socket.onMessage('pong'); });
      return;
    }
    const message = JSON.parse(text);
    if (message.op === 'login' && values.venue === 'bitunix') {
      socket.authenticated = true;
      for (const arg of message.args) this.verifySignature(arg, '', '', true);
    }
    if (message.op === 'subscribe' || message.op === 'unsubscribe') {
      const validate = venue.validateSubscriptions ?? venue.validateLogin;
      if (validate && !Array.isArray(message.args)) throw new Error(`Invalid subscription on ${socket.url}`);
      for (const arg of message.args) {
        if (validate && (!arg || typeof arg.channel !== 'string' || !arg.channel)) throw new Error(`Invalid subscription topic on ${socket.url}`);
        const topic = JSON.stringify(canonicalObject(arg));
        const was = socket.topics.has(topic);
        if (message.op === 'subscribe') {
          socket.topics.add(topic);
          socket.subscriptionArgs.set(topic, arg);
          if (!was && ['ticker', 'price'].includes(arg.ch)) socket.publicTopics.add(topic);
        } else { socket.topics.delete(topic); socket.publicTopics.delete(topic); socket.subscriptionArgs.delete(topic); }
        this.subscriptions.push(JSON.stringify([url, connection, message.op, canonicalObject(arg)]));
      }
    } else if (message.event === 'sub' || message.event === 'unsub') {
      const topic = JSON.stringify(canonicalObject(message.params));
      if (message.event === 'sub') socket.topics.add(topic); else socket.topics.delete(topic);
      this.subscriptions.push(JSON.stringify([url, connection, message.event, canonicalObject(message.params)]));
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

  verifySignature(headers, query, body, ws = false) {
    const h = (name) => headers.get?.(name) ?? headers[name];
    const key = ws ? headers.apiKey : h('api-key');
    const nonce = h('nonce');
    const timestamp = h('timestamp');
    const signature = h(ws ? 'sign' : 'sign');
    const hash = (v) => createHash('sha256').update(v).digest('hex');
    const expected = hash(hash(`${nonce}${timestamp}${key}${query}${body ?? ''}`) + SECRET);
    if (key !== KEY || !nonce || !timestamp || signature !== expected) this.signatureErrors.push('Bitunix signature mismatch');
  }

  respond(method, url, body, headers = {}) {
    if (values.venue === 'bitunix' && !new URL(url).pathname.includes('/market/')) {
      const query = [...new URL(url).searchParams].sort(([a], [b]) => a.localeCompare(b))
        .map(([key, value]) => `${key}${value}`).join('').replace(/[=&]/g, '');
      this.verifySignature(headers, query, body);
    }
    if (venue.isWrite?.(method, url) ?? method !== 'GET') this.outbound.push(requestKey(method, url, body));
    return this.responses.respond(method, url, body, this.commandIndex, this.time);
  }

  connect(url, onOpen, onMessage, onClose) {
    const rawUrl = url;
    url = canonicalUrl(url);
    const connection = this.connectionCounts.get(url) ?? 0;
    this.connectionCounts.set(url, connection + 1);
    const socket = { url, rawUrl, connection, onOpen, onMessage, onClose, closed: false, opened: false, topics: new Set(), publicTopics: new Set(), subscriptionArgs: new Map(), loginVerified: false };
    this.sockets.push(socket);
    this.byConnection.set(`${url}:${connection}`, socket);
    // Old synthetic fixtures have no lifecycle records; live tapes drive every
    // open explicitly, including failed startup attempts and reconnects.
    const hasLifecycle = records.some((r) => r.type === 'open' && canonicalUrl(r.url) === url);
    const legacyInitial = values.venue !== 'bitunix' && connection === 0 &&
      !records.some((r) => r.type === 'connect' && canonicalUrl(r.url) === url);
    if (!hasLifecycle || legacyInitial) setImmediate(() => this.openSocket(socket));
    return socket;
  }

  openSocket(socket) {
    if (socket.closed || socket.opened) return;
    socket.opened = true;
    socket.onOpen();
  }

  assertSubscribed(socket, message, subscriptionKey) {
    if (!(venue.validateSubscriptions ?? venue.validateLogin) || !message.arg?.channel || !Array.isArray(message.data)) return;
    // Public arg objects usually identify one exact subscription. Avoid an
    // order-dependent scan through every market's topics on each replay frame.
    const exact = subscriptionKey && socket.subscriptionArgs.get(subscriptionKey);
    if (exact) {
      const covered = message.data.every((row) => {
        const object = row && !Array.isArray(row) && typeof row === 'object' ? row : {};
        return Object.entries(exact).every(([key, value]) => key === 'channel' ||
          (key === 'instType' && value === 'ANY') || object[key] === undefined || object[key] === '' || object[key] === value);
      });
      if (!covered) throw new Error(`Mismatched subscription row on ${socket.url}: ${JSON.stringify(message.arg)}`);
      return;
    }
    const topics = [...socket.subscriptionArgs.values()].filter((topic) => topic.channel === message.arg.channel);
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

  async socketFor(frame) {
    const deadline = performance.now() + 10_000;
    for (;;) {
      const exact = frame.connection !== undefined && this.byConnection.get(`${frame.url}:${frame.connection}`);
      if (exact && (!exact.closed || frame.type === 'close')) return exact;
      if (performance.now() > deadline) throw new Error(`missing tape socket ${frame.url} connection=${frame.connection}; active=${this.sockets.map((s) => `${s.url}:${s.connection}:${s.closed}`).join()}`);
      const candidates = this.sockets.filter((s) => s.url === frame.url && (!s.closed || frame.type === 'close') &&
        (frame.connection === undefined || s.connection === frame.connection));
      if (frame.connection !== undefined || candidates.length === 1) {
        if (candidates[0]) return candidates[0];
      } else if (candidates.length > 1 && frame.text) {
        const message = JSON.parse(frame.text);
        const topic = JSON.stringify(canonicalObject(message.ch ? { ch: message.ch, symbol: message.symbol } : { channel: message.channel }));
        const matched = candidates.filter((s) => s.topics.has(topic));
        if (matched.length === 1) return matched[0];
        throw new Error(`ambiguous legacy tape socket ${frame.url} ${topic}; record connection IDs`);
      }
      await new Promise((r) => setTimeout(r, 1));
    }
  }

  validateTopic(socket, frame) {
    if (values.venue !== 'bitunix') return;
    if (socket.url.includes(venue.privateSocket)) {
      if (frame.channel && !socket.authenticated) this.socketErrors.push('private frame before login');
      return;
    }
    if (frame.topic && !socket.topics.has(frame.topic)) this.socketErrors.push(`unsubscribed ${socket.connection} ${frame.topic}`);
  }

  async lifecycle(frame) {
    const socket = await this.socketFor(frame);
    if (frame.type === 'open') this.openSocket(socket);
    else {
      if (frame.local && !socket.closed) throw new Error(`recorded local close was not requested: ${frame.url}:${frame.connection}`);
      if (!frame.local && socket.closed) throw new Error('socket closed before recorded remote close');
      socket.closed = true;
      if (!frame.local) socket.onClose?.(frame.code ?? 1006);
    }
  }

  /**
   * Deliver every frame in order, yielding to the event loop between frames.
   * Commands are awaited where they fall. A private tape is paced: each record
   * waits until its time after the account is ready.
   */
  async play(onTick, ready = async () => {}, runCommand = async () => {}) {
    let initialized;
    const ensureReady = () => initialized ??= within((async () => {
      await ready();
      if (venue.startupTopics) {
        const expected = venue.startupTopics(records, bookSymbols).map((arg) => JSON.stringify(canonicalObject(arg)));
        const complete = () => expected.every((topic) => this.sockets.some((socket) => socket.opened && !socket.closed && socket.topics.has(topic)));
        while (!complete()) await new Promise((r) => setTimeout(r, 10));
      }
      if (values.venue !== 'bitunix') return;
      const symbols = JSON.parse(records.find((r) => r.type === 'http' && r.url.includes('/market/trading_pairs')).body).data
        .filter((m) => apiSupported(m.isApiSupported) && m.symbolStatus === 'OPEN' && m.quote === 'USDT').length;
      const active = () => this.sockets.filter((s) => s.opened && !s.closed).reduce((sum, s) => sum + s.publicTopics.size, 0);
      const deadline = performance.now() + 10_000;
      while (active() < symbols * 2) {
        if (performance.now() > deadline) throw new Error(`active startup topics missing: expected ${symbols * 2}, got ${active()}`);
        await new Promise((r) => setTimeout(r, 5));
      }
    })(), 15_000, 'Replay startup');
    const recordedLifecycle = records.some((r) => r.type === 'connect');
    if (values.venue !== 'bitunix' && !recordedLifecycle) await ensureReady();
    this.peakBytes = Math.max(this.peakBytes ?? 0, this.sampleMemory?.() ?? 0);
    const start = performance.now();
    let delivered = 0;
    const warm = Math.floor(this.frames.length / 4);
    for (const frame of this.frames) {
      if (delivered === warm) this.warmMark = { thread: process.threadCpuUsage(), wall: performance.now() };
      if (isPrivate) {
        const wait = frame.t - (performance.now() - start);
        if (wait > 0) await new Promise((resolve) => setTimeout(resolve, wait));
      }
      this.time = frame.t;
      if (frame.type === 'command') {
        await ensureReady();
        this.commandIndex++;
        this.results.push(await within(runCommand(frame), 15_000, `Command ${frame.op}`).then((ids) => ({ ok: ids }), (error) => {
          if (error instanceof ReplayTimeout) throw error;
          return { error: String(error?.message ?? error) };
        }));
      } else {
        const socket = await this.socketFor(frame);
        if (frame.type === 'open' || frame.type === 'close') await this.lifecycle(frame);
        else {
          if (!socket.opened) throw new Error('frame before recorded socket open');
          if (values.venue === 'bitunix' && frame.topic && !socket.url.includes(venue.privateSocket)) {
            const deadline = performance.now() + 10_000;
            while (!socket.topics.has(frame.topic) && !socket.closed && performance.now() < deadline)
              await new Promise((r) => setTimeout(r, 5));
          }
          if ((venue.validateLogin || venue.validateSubscriptions) && frame.text.trimStart().startsWith('{')) {
            const message = JSON.parse(frame.text);
            if (venue.validateLogin && message.event === 'login' && String(message.code) === '0' && !socket.loginVerified)
              throw new Error(`Taped login success preceded a valid login on ${frame.url}`);
            this.assertSubscribed(socket, message, frame.subscriptionKey);
          }
          this.validateTopic(socket, frame);
          socket.onMessage(frame.text);
        }
      }
      delivered++;
      if (delivered % 64 === 0) this.peakBytes = Math.max(this.peakBytes, this.sampleMemory?.() ?? 0);
      onTick?.(frame.t);
      await new Promise(setImmediate);
    }
    await ensureReady();
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

const measure = async (cpuStart, wallStart, heapBefore, wasmBytes = 0, warmMark, peakBytes = 0) => {
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
    peakBytes: Math.max(peakBytes, retainedMemory(heapBefore, memory, wasmBytes)),
    arrayBufferBytes: memory.arrayBuffers,
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

const bookHash = (t, symbol, book, packed = false) => {
  const levels = (rows) => packed ? Array.from({ length: rows.length / 3 }, (_, i) => [rows[i * 3], rows[i * 3 + 1]]) : rows.map((r) => [r.price, r.amount]);
  return [t, symbol, createHash('sha256').update(JSON.stringify([levels(book.bids), levels(book.asks)])).digest('hex')];
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
  limits: { amount: { min: Number(m.limits.amount.min), max: Number(m.limits.amount.max) } },
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
  const bookEvents = [];
  let orders;
  let positions;
  let balance;
  let settings;
  let status;
  const fills = [];
  const candleUpdates = [];
  const transport = {
    now: () => Date.now(),
    connect: (url, { onOpen, onMessage, onClose }) => {
      const socket = tape.connect(url, onOpen, (text) => { tape.audit?.received(url, text); onMessage(text); }, onClose);
      return { send: (text) => tape.send(socket, text), close: () => (socket.closed = true) };
    },
    request: async ({ method, url, body: requestBody, headers }) => {
      const { status, headers: responseHeaders = '', body } = tape.respond(method, url, requestBody, headers);
      const chunks = async function* () {
        const bytes = encoder.encode(body);
        for (let offset = 0; offset < bytes.length; offset += 16 * 1024) yield bytes.subarray(offset, offset + 16 * 1024);
      };
      return { status, headers: responseHeaders, body: chunks() };
    },
    setTimeout: (fn, ms) => setTimeout(fn, ms),
    clearTimeout: (handle) => clearTimeout(handle),
  };
  const options = { ...venue.options, ...(isPrivate ? { key: KEY, secret: SECRET, passphrase: PASSPHRASE } : { publicDataOnly: true }) };
  await settle();
  const heapBefore = await baseline();
  tape.sampleMemory = () => retainedMemory(heapBefore, process.memoryUsage(), wasmMemoryBytes());
  const cpuStart = { process: process.cpuUsage(), thread: process.threadCpuUsage() };
  const wallStart = performance.now();
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
      const candles = frame.candles();
      for (let i = 0; i < (candles?.count ?? 0); i++) {
        const row = { symbol: markets.symbol(candles.market[i]), interval: candles.interval(i), candle: Array.from(candles.rows.subarray(i * 6, i * 6 + 6)) };
        candleUpdates.push(row);
      }
      const executed = frame.fills();
      for (let i = 0; i < (executed?.count ?? 0); i++) {
        fills.push({ market: executed.market[i], side: executed.side[i], amount: executed.rows[i * 2 + 1], price: executed.rows[i * 2], id: executed.orderId(i) });
      }
    },
  });
  const sampling = setInterval(() => { tape.peakBytes = Math.max(tape.peakBytes ?? 0, tape.sampleMemory()); }, 5);
  host.start();
  for (const symbol of bookSymbols) host.command({ op: 'subscribeOrderBook', symbol });
  let lastFlush = -Infinity;
  const allLoaded = 31;
  const flushTimer = isPrivate ? setInterval(() => host.flush(), 250) : undefined;
  const frames = await tape.play(
    (t) => {
      if (t - lastFlush >= 250) {
        host.flush();
        for (const [market, book] of books) bookEvents.push(bookHash(t, markets.symbol(market), book, true));
        lastFlush = t;
      }
    },
    async () => {
      while ((isPrivate && (status?.loaded ?? 0) !== allLoaded) || (tape.audit && !tape.audit.ready())) await new Promise((r) => setTimeout(r, 5));
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
  clearInterval(sampling);
  const metrics = await measure(cpuStart, wallStart, heapBefore, wasmMemoryBytes(), tape.warmMark, tape.peakBytes);
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
  return { frames, metrics, bookEvents, unserved: tape.unserved, unusedWrites: tape.unusedWrites(), protocolErrors: tape.audit?.errors() ?? [], signatures: tape.signatureErrors, outbound: tape.outbound, subscriptions: tape.subscriptions.sort(), socketErrors: tape.socketErrors, candles: candleUpdates, store: { markets: marketRows, tickers: tickerRows }, books: bookRows, account, commands: tape.results };
}

const commandArgs = ({ t, type, ...args }) => args;


/** Main-thread view of the account, rebuilt from the latest frame tables. */
function rustAccount({ markets, orders, positions, balance, settings, status, fills }) {
  const symbol = (i) => markets.symbol(i);
  const orderRows = [];
  for (let i = 0; i < (orders?.count ?? 0); i++) {
    const row = { id: orders.id(i), clientOrderId: orders.clientOrderId(i), parentId: orders.parentId(i), symbol: symbol(orders.market[i]), ...orderEnums(orders.enums[i]) };
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
          tape.audit?.received(url, data);
          for (const fn of this.listeners.message) fn({ data });
          this.onmessage?.({ data });
          venue.afterTsFrame?.(exchange);
        },
        (code) => { this.readyState = 3; for (const fn of this.listeners.close) fn({ code }); this.onclose?.({ code }); },
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
    const { status, body } = tape.respond((config.method ?? 'get').toUpperCase(), url.toString(), config.data, config.headers);
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
  const { ExchangeWorker } = await import('../../packages/safe-cex/src/webworker/ExchangeWorker.ts');
  const finalBooks = {};
  const bookEvents = [];
  const candleUpdates = [];
  const candleListeners = new Map();
  const mainBooks = {};
  await settle();
  const heapBefore = await baseline();
  tape.sampleMemory = () => retainedMemory(heapBefore, process.memoryUsage());
  const cpuStart = { process: process.cpuUsage(), thread: process.threadCpuUsage() };
  const wallStart = performance.now();
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
  const executions = [];
  exchange.emitter.on('fill', (fill) => executions.push(fill));
  for (const client of axiosClients(exchange)) client.defaults.adapter = adapter;
  const sampling = setInterval(() => { tape.peakBytes = Math.max(tape.peakBytes ?? 0, tape.sampleMemory()); }, 5);
  const started = exchange.start();
  const prepared = started.then(() => venue.prepareTs?.(exchange, { isPrivate }));
  const subscribed = (async () => {
    while (!store.markets.length) await new Promise((r) => setTimeout(r, 1));
    for (const symbol of bookSymbols) exchange.listenOrderBook(symbol, (book) => (finalBooks[symbol] = book));
  })();
  // Model the worker posting each subscribed book to main every 250 ms.
  let lastFlush = -Infinity;
  const frames = await tape.play(
    (t) => {
      if (t - lastFlush >= 250) {
        for (const [symbol, book] of Object.entries(finalBooks)) bookEvents.push(bookHash(t, symbol, book));
        postStore();
        if (!values['no-egress']) {
          for (const [symbol, book] of Object.entries(finalBooks)) mainBooks[symbol] = structuredClone(book);
        }
        lastFlush = t;
      }
    },
    async () => {
      await prepared;
      while (!venue.tsBooksSubscribed(exchange, bookSymbols.length)) await new Promise((r) => setTimeout(r, 5));
      const loaded = () => Object.values(store.loaded).every(Boolean);
      while (isPrivate && !loaded()) await new Promise((r) => setTimeout(r, 5));
    },
    (record) => runTsCommand(exchange, store, record, { candleListeners, candleUpdates }),
  );
  await within(Promise.all([started, subscribed]), 15_000, 'TS startup/subscriptions');
  await settle();
  tape.assertWritesConsumed();
  postStore();
  if (!values['no-egress']) for (const [symbol, book] of Object.entries(finalBooks)) mainBooks[symbol] = structuredClone(book);
  clearInterval(sampling);
  const metrics = await measure(cpuStart, wallStart, heapBefore, 0, tape.warmMark, tape.peakBytes);
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
        settings: [...new Map(store.positions.filter((p) => p.leverage > 0).sort((a, b) => a.contracts - b.contracts).map((p) => [p.symbol, { symbol: p.symbol, leverage: p.leverage, marginType: p.marginType }])).values()],
        fills: executions.map((f) => ({ id: f.id, symbol: f.symbol, side: f.side, amount: f.amount, ...(venue.compareFillPrices ? { price: f.price } : {}) })),
      }
    : undefined;
  exchange.dispose();
  return { frames, metrics, bookEvents, unserved: tape.unserved, unusedWrites: tape.unusedWrites(), protocolErrors: tape.audit?.errors() ?? [], signatures: tape.signatureErrors, outbound: tape.outbound, subscriptions: tape.subscriptions.sort(), socketErrors: tape.socketErrors, candles: candleUpdates, store: snapshot, books: bookCopies, account, commands: tape.results };
}

async function runTsCommand(exchange, store, record, { candleListeners, candleUpdates }) {
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
    case 'subscribeCandles': {
      const key = `${args.symbol}:${args.interval}`;
      candleListeners.set(key, exchange.listenOHLCV(args, (c) => candleUpdates.push({ symbol: args.symbol, interval: args.interval, candle: [c.timestamp, c.open, c.high, c.low, c.close, c.volume] })));
      return;
    }
    case 'unsubscribeCandles': candleListeners.get(`${args.symbol}:${args.interval}`)?.(); return;
    default: throw new Error(`unknown command ${op}`);
  }
}

/** The fields both implementations carry, in one shape. */
function normalizeAccount(account) {
  const order = (o) => ({
    id: String(o.id),
    symbol: o.symbol,
    status: o.status,
    ...(values.venue === 'bitunix' ? {
      timeInForce: ({ GTC: 'GoodTillCancel', POST_ONLY: 'PostOnly', GTX: 'PostOnly', IOC: 'ImmediateOrCancel', FOK: 'FillOrKill' })[o.timeInForce] ?? o.timeInForce ?? 'GoodTillCancel',
      // TS keeps native identity in info; virtual protection rows share that
      // info with their parent, so only the native row owns its client ID.
      clientOrderId: o.clientOrderId || (String(o.info?.orderId) === String(o.id) ? o.info?.clientId : null) || null,
      parentId: o.parentId || (o.info?.orderId && ['tp', 'sl'].some((leg) => o.id === `${o.info.orderId}__${leg}`) ? String(o.info.orderId) : null),
    } : {}),
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
    ...(values.venue === 'bitunix' ? { realizedPnl: Number.isFinite(p.realizedPnl) ? p.realizedPnl : null } : {}),
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
    signatures: result.signatures,
    outbound: result.outbound,
    subscriptions: result.subscriptions,
    candles: result.candles,
    socketErrors: result.socketErrors,
    bookEvents: result.bookEvents.sort((a, b) => a[0] - b[0] || a[1].localeCompare(b[1])),
  }),
);
clearTimeout(replayDeadline);
process.exit(0);
