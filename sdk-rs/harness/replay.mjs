// Replay a recorded tape through one implementation and report its final
// Store, order books, CPU time and memory as JSON on stdout.
//
//   node --expose-gc --import tsx sdk-rs/harness/replay.mjs --impl ts --tape <file>
//   node --expose-gc sdk-rs/harness/replay.mjs --impl rust --tape <file>
import { readFileSync } from 'node:fs';
import { createHash, createHmac, timingSafeEqual } from 'node:crypto';
import { parseArgs } from 'node:util';
import { gunzipSync } from 'node:zlib';
import { NONE, ORDER_COLUMNS, POSITION_COLUMNS, orderEnums, positionEnums } from '../hosts/wasm/js/wire.mjs';
import { axiosClients, venueOf } from './venues.mjs';
import { isRead, subscriptionKeys, requestKey as keyOf } from './requests.mjs';
import { replayStoreReady, replayBooksReady, ReplayClock, Tape as SharedTape, canonicalUrl as sharedUrl } from './tape.mjs';
import { setTimeout as realTimeout, clearTimeout as realClearTimeout, setInterval as realInterval, clearInterval as realClearInterval } from 'node:timers';
import { parseRequestJson } from './request-json.mjs';
import { retainedMemory, settledMemory } from './memory.mjs';
import { Responses, implementationRecords } from './responses.mjs';

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
const records = implementationRecords((values.tape.endsWith('.gz') ? gunzipSync(tapeBytes) : tapeBytes)
  .toString('utf8')
  .split('\n')
  .filter(Boolean)
  .map((line) => {
    const record = JSON.parse(line);
    // Both tape spellings describe the earliest time a snapshot is visible.
    if (record.effectiveAt !== undefined) record.availableAfter ??= record.effectiveAt;
    return record;
  }), values.impl);
const replayOptions = records.find((record) => record.type === 'config')?.options ?? {};
if (values.venue === 'hyrotrader' && replayOptions.endpoints) {
  process.env.NEXT_PUBLIC_HYROTRADER_PROD_REST_URL = replayOptions.endpoints.rest;
  process.env.NEXT_PUBLIC_HYROTRADER_PROD_PUBLIC_WS_URL = replayOptions.endpoints.publicWs;
  process.env.NEXT_PUBLIC_HYROTRADER_PROD_PRIVATE_WS_URL = replayOptions.endpoints.privateWs;
}
const bookSymbols = values.books.split(',').filter(Boolean);
const metadata = records.find((r) => r.type === 'meta');
// Signing clocks remain continuous through startup. Tape.play separately
// schedules records relative to readiness without rebasing Date.now.
const clockStart = performance.now();
if (Number.isFinite(metadata?.epoch)) Date.now = () => metadata.epoch + Math.floor(performance.now() - clockStart);
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
    return await Promise.race([promise, new Promise((_, reject) => {
      timer = realTimeout(() => reject(new ReplayTimeout(`${label} exceeded ${ms} ms`)), ms);
    })]);
  } finally { realClearTimeout(timer); }
};
// A tape with a user stream replays an account: signed, paced in real time,
// and compared on its private state rather than its books.
const config = records.find((record) => record.type === 'config') ?? {};
const isPrivate = config.private ?? metadata?.private ?? records.some((r) =>
  venue.isPrivateRecord ? venue.isPrivateRecord(r) : Boolean(venue.privateSocket && r.url?.includes(venue.privateSocket)),
);
const metadataOptions = { ...metadata?.options, ...(typeof metadata?.testnet === 'boolean' ? { testnet: metadata.testnet } : {}) };
const KEY = config.options?.key ?? venue.replayCredentials?.key ?? 'replay-key';
const SECRET = config.options?.secret ?? venue.replayCredentials?.secret ?? 'replay-secret';
const PASSPHRASE = config.options?.passphrase ?? venue.replayCredentials?.passphrase ?? 'replay-passphrase';
venue.replayOptions = { ...venue.options, ...metadataOptions, ...(isPrivate
  ? { ...venue.replayCredentials, key: KEY, secret: SECRET, passphrase: PASSPHRASE }
  : { key: '', secret: '', publicDataOnly: true, ...venue.publicOptions }), ...config.options };
await venue.prepareReplay?.({ isPrivate, options: venue.replayOptions });
await venue.initializeReplay?.(values.impl, venue.replayOptions);

const callerIds = new Set();
const collectCallerIds = (value) => {
  if (!value || typeof value !== 'object') return;
  if (typeof value.clientOrderId === 'string') callerIds.add(value.clientOrderId);
  for (const nested of Object.values(value)) collectCallerIds(nested);
};
for (const record of records) if (record.type === 'command') collectCallerIds(record);

/** Per-request clocks, signatures, connection IDs and generated client IDs. */
const VOLATILE = new Set(['timestamp', 'recvWindow', 'signature', 'requestId']);
const clientFields = new Set(['newClientOrderId', 'clientAlgoId', 'clientId', 'clientOrderId']);
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
    if (typeof value === 'bigint' && values.venue === 'mexc') return String(value);
    if (Array.isArray(value)) return value.map(sorted);
    if (!value || typeof value !== 'object') return value;
    return Object.fromEntries(Object.entries(value).filter(([key, v]) => v !== undefined &&
      (!clientFields.has(key) || callerIds.has(v)) && !(venue.volatileBodyFields && VOLATILE.has(key)) &&
      !(values.venue === 'mexc' && ['externalOid', 'requestId'].includes(key) && !callerIds.has(v)))
      .sort(([a], [b]) => a.localeCompare(b)).map(([key, v]) => [key,
        !callerIds.has(v) && venue.generatedBodyId?.(key, v) ? '<generated>' : sorted(v),
      ]));
  };
  return JSON.stringify(sorted(value), (_key, value) => typeof value === 'bigint' ? { integer: String(value) } : value);
};
const canonicalUrl = (url) => {
  const parsed = new URL(venue.requestUrl?.(url) ?? url);
  if (!venue.requestUrl && values.venue === 'mexc' && /\/private\/(planorder|stoporder)\/list\/orders$/.test(parsed.pathname)
    && parsed.searchParams.has('start_time') && parsed.searchParams.has('end_time')) {
    const end = Number(parsed.searchParams.get('end_time'));
    parsed.searchParams.set('start_time', String(Number(parsed.searchParams.get('start_time')) - end));
    parsed.searchParams.set('end_time', 'now');
  }
  const query = [...parsed.searchParams]
    .filter(([name, value]) => !VOLATILE.has(name) && !venue.volatileQueryFields?.includes(name) && (!clientFields.has(name) || callerIds.has(value)))
    .map(([name, value]) => [name, value.startsWith('[') || value.startsWith('{') ? canonicalBody(value) : value])
    .sort(([a], [b]) => a.localeCompare(b));
  return `${parsed.origin}${parsed.pathname}?${new URLSearchParams(query)}`;
};
const requestKey = (method, url, body) => (['blofin', 'mexc', 'bingx', 'bitrue', 'lighter', 'kucoin', 'asterdex', 'kraken', 'apexomni', 'paradex', 'phemex'].includes(values.venue) || venue.family === 'orderly' || venue.family === 'bybit')
  ? keyOf(method, url, body, callerIds, venue)
  : `${method} ${canonicalUrl(url)} ${canonicalBody(body)}`;

const isWriteRequest = (method, url) => venue.isWrite?.(method, url) ?? !isRead(method, url);

/** Serves recorded HTTP responses and websocket frames in tape order. */
class Tape extends SharedTape {
  constructor(impl) {
    super(records, { ...venue, impl, books: bookSymbols, timeoutMs: 15000, isPrivate, canonicalize: values.venue === 'backpack' ? (url) => sharedUrl(url, callerIds, venue) : canonicalUrl,
      explicitOpens: values.venue === 'bitunix', legacyLatest: false,
      responses: values.venue === 'backpack' || values.venue === 'hyperliquid' || values.venue === 'lighter' || venue.family === 'orderly' || venue.family === 'bybit' || values.venue === 'kucoin' || values.venue === 'asterdex' || values.venue === 'kraken' || values.venue === 'apexomni' || values.venue === 'paradex' || values.venue === 'phemex' || values.venue.startsWith('bitget') ? undefined : new Responses(records, requestKey, venue.checkRequest, isWriteRequest,
        venue.matchRequestBody ? undefined : (method, url, body) => isWriteRequest(method, url) && !['', '{}'].includes(canonicalBody(body)) ? undefined : requestKey(method, url)),
      prepareFrame: (r) => {
        if (r.type !== 'ws' || typeof r.text !== 'string') return r;
        const m = r.text.trimStart().startsWith('{') ? JSON.parse(r.text) : {};
        const topic = m.channel ? { channel: m.channel } : m.ch ? (m.symbol ? { ch: m.ch, symbol: m.symbol } : { ch: m.ch }) : undefined;
        return { ...r, topic: topic && JSON.stringify(canonicalObject(topic)), channel: m.ch, subscriptionKey: m.arg && JSON.stringify(canonicalObject(m.arg)) };
      },
    });
    this.audit = venue.createSocketAudit?.(impl, { isPrivate, books: bookSymbols, tickers: venue.publicTickers?.(records), records });
    this.commandIndex = 0;
    // Retain expected keys/backing storage across both memory samples: deleting
    // startup-only harness state would subtract it from workload retained memory.
    this.startupSubscriptions = new Map(Array.from(venue.startupSubscriptions?.(records, bookSymbols) ?? [], (key) => [key, false]));
    this.pendingStartupSubscriptions = this.startupSubscriptions.size;
  }

  get unserved() { return this.responses.unserved ?? this.responseStore?.unserved ?? this._unserved; }

  time = 0;
  outbound = [];
  subscriptions = [];
  socketErrors = [];
  signatureErrors = [];
  sentRequests = new Set();

  send(socket, text) {
    const { url, connection } = socket;
    if (values.venue === 'backpack' || values.venue === 'hyperliquid' || values.venue === 'bitrue' || values.venue === 'woo' || values.venue === 'lighter' || venue.family === 'orderly' || venue.subscriptionGenerations || values.venue === 'kucoin' || values.venue === 'asterdex' || values.venue === 'kraken' || values.venue === 'apexomni' || values.venue === 'paradex' || values.venue === 'phemex' || values.venue.startsWith('bitget')) return super.send(socket, text);
    venue.validateSocketMessage?.(socket, text);
    for (const key of subscriptionKeys(socket.rawUrl, text)) {
      if (this.startupSubscriptions.get(key) === false) {
        this.startupSubscriptions.set(key, true);
        this.pendingStartupSubscriptions--;
      }
    }
    this.audit?.sent(socket.rawUrl, text);
    if (values.venue === 'bingx' && (text === 'Ping' || text === 'Pong')) return;
    if ((venue.respondToPing || venue.blofinProtocol) && text === 'ping') {
      if (venue.respondToPing) setImmediate(() => { if (!socket.closed) socket.onMessage('pong'); });
      return;
    }
    if ((venue.respondToPing || venue.blofinProtocol) && text === 'pong') return;
    const message = JSON.parse(text);
    if (venue.requestBodies) {
      if (message.op === 'subscribe' || message.op === 'unsubscribe') {
        for (const topic of message.args ?? []) this.sentRequests.add(JSON.stringify({ url, connection, op: message.op, topic: canonicalObject(topic) }));
      } else if (message.op === 'login') {
        const args = message.args.map(({ timestamp, nonce, sign, ...credentials }) => canonicalObject(credentials));
        this.sentRequests.add(JSON.stringify({ url, connection, op: 'login', args }));
      }
    }
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
    } else if (message.reqType === 'sub' || message.reqType === 'unsub') {
      const topic = JSON.stringify(message.dataType);
      if (message.reqType === 'sub') socket.topics.add(topic); else socket.topics.delete(topic);
      this.subscriptions.push(JSON.stringify([url, connection, message.reqType, message.dataType]));
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
    const response = super.respond(method, url, body, headers);
    const comparisonBody = Object.hasOwn(response, 'comparisonRequestBody') ? response.comparisonRequestBody : body;
    if (isWriteRequest(method, url)) this.outbound.push((this.bitget || this.hyperliquid) ? this.requestKey(method, url, comparisonBody) : requestKey(method, url, comparisonBody));
    return response;
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
    if (this.invalidSignatures) throw new Error(`${this.invalidSignatures} invalid replay signature(s)`);
    const missing = this.missingWrites;
    if (missing) throw new Error(`${missing} taped write(s) were not sent`);
  }

  validateTopic(socket, frame) {
    if (values.venue !== 'bitunix') return;
    if (socket.url.includes(venue.privateSocket)) {
      if (frame.channel && !socket.authenticated) this.socketErrors.push('private frame before login');
      return;
    }
    if (frame.topic && !socket.topics.has(frame.topic)) this.socketErrors.push(`unsubscribed ${socket.connection} ${frame.topic}`);
  }

  unusedWrites() {
    return this.missingWrites;
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
    heapBeforeBytes: heapBefore.heapUsed,
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
const baseline = () => settledMemory({ pause: () => new Promise((resolve) => realTimeout(resolve, 10)) });

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
  ...(venue.inverseNativeAmounts || venue.family === 'bybit' ? { isInverse: Boolean(m.isInverse) } : {}),
  ...(venue.family === 'bybit' ? { isSpot: Boolean(m.isSpot), isExpiring: Boolean(m.isExpiring) } : {}),
  contractSize: m.contractSize ?? null,
  precision: { amount: m.precision.amount, price: m.precision.price },
  limits: { amount: { min: Number(m.limits.amount.min), max: Number(m.limits.amount.max) } },
});

async function runRust() {
  const { EngineHost } = await import('../hosts/wasm/js/host.mjs');
  const { Frame, TickerState, MARKET_COLUMNS } = await import('../hosts/wasm/js/wire.mjs');
  const { loadEngine } = await import('./engine.mjs');
  const { WasmEngine, wasmMemoryBytes } = await loadEngine(venue.rust, { ...venue.options, ...metadataOptions, ...config.options });
  const tape = new Tape('rust');
  const clock = isPrivate ? undefined : new ReplayClock(metadata?.epoch ?? Date.now());
  clock?.install();
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
    webSocketHeaders: true,
    connect: (url, { channel, context, headers, onOpen, onMessage, onBinary, onClose }) => {
      venue.validateSocket?.(url, headers);
      const socket = tape.connect(url, onOpen, (data) => typeof data === 'string' ? onMessage(data) : onBinary?.(data), onClose, { channel, context });
      return { send: (text) => tape.send(socket, text), close: () => tape.close(socket, 1000, true) };
    },
    request: async ({ method, url, body: requestBody, headers, signal }) => {
      signal?.throwIfAborted();
      const { status, headers: responseHeaders = '', body } = await tape.waitForResponse(tape.respond(method, url, requestBody, headers));
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
  const options = { ...venue.replayOptions };
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
  const sampling = realInterval(() => { tape.peakBytes = Math.max(tape.peakBytes ?? 0, tape.sampleMemory()); }, 5);
  host.start();
  for (const symbol of bookSymbols) host.command({ op: 'subscribeOrderBook', symbol });
  let lastFlush = -Infinity;
  const flushTimer = isPrivate ? setInterval(() => host.flush(), 250) : undefined;
  const frames = await tape.play(
    (t) => {
      if (t - lastFlush >= 250) {
        host.flush();
        for (const [market, book] of books) bookEvents.push(bookHash(t, markets.symbol(market), book, true));
        lastFlush = t;
      }
    },
    async (accountReady = true) => {
      for (;;) {
        host.flush();
        const booksReady = !venue.initialBookSnapshots || replayBooksReady(bookSymbols, Array.from(books.keys(), (market) => markets?.symbol(market)));
        if (replayStoreReady(status?.loaded ?? 0, isPrivate && accountReady) && booksReady && (!tape.audit || tape.audit.ready()) && tape.sockets.some((s) => s.opened && !s.closed)) break;
        await new Promise((r) => realTimeout(r, 5));
      }
    },
    (record) => {
      const { op, ...rest } = record;
      return host.command({ op, ...commandArgs(rest) });
    },
    clock,
  );
  await settle();
  tape.assertWritesConsumed();
  clearInterval(flushTimer);
  host.flush();
  realClearInterval(sampling);
  const metrics = await measure(cpuStart, wallStart, heapBefore, wasmMemoryBytes(), tape.warmMark, tape.peakBytes);
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
    const row = { symbol: tickers.market[i] === NONE ? tickers.identity?.(i).symbol : markets.symbol(tickers.market[i]) };
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
  return { frames, metrics, bookEvents, requests: [...tape.requests].sort(), writes: tape.writes, unserved: tape.unserved, unusedWrites: tape.unusedWrites(), protocolErrors, invalidSignatures: tape.invalidSignatures, wsRequests: [...tape.sentRequests].sort(), signatures: tape.signatureErrors, outbound: tape.outbound, subscriptions: values.venue === 'backpack' ? [...tape.nativeSubscriptions].sort() : (tape.hyperliquid || values.venue === 'bitrue' || values.venue === 'woo' || values.venue === 'lighter' || venue.family === 'orderly' || venue.subscriptionGenerations || values.venue === 'kucoin' || values.venue === 'asterdex' || values.venue === 'kraken' || values.venue === 'apexomni' || values.venue === 'paradex' || values.venue === 'phemex') ? [...tape.fullSubscriptions].sort() : tape.bitget ? [...tape.protocolSubscriptions].sort() : tape.subscriptions.sort(), socketErrors: tape.socketErrors, candles: candleUpdates, store: { markets: marketRows, tickers: tickerRows }, books: bookRows, account, commands: tape.results };
}

const commandArgs = ({ t, type, expected, ...args }) => args;


/** Main-thread view of the account, rebuilt from the latest frame tables. */
function rustAccount({ markets, orders, positions, balance, settings, status, fills }) {
  const symbol = (i) => markets.symbol(i);
  const orderRows = [];
  for (let i = 0; i < (orders?.count ?? 0); i++) {
    const row = { id: orders.id(i), clientOrderId: orders.clientOrderId(i), parentId: orders.parentId(i), symbol: symbol(orders.market[i]), ...orderEnums(orders.enums[i]) };
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
    // Match TS placeholders while retaining margin-only rails in real frames.
    if (!venue.independentLeverage && !(settings.leverage[i] > 0)) continue;
    settingRows.push({ symbol: symbol(settings.market[i]), marginType: settings.marginType(i),
      ...(venue.independentLeverage ? { buyLeverage: settings.buyLeverage[i], sellLeverage: settings.sellLeverage[i] } : { leverage: settings.leverage[i] }) });
  }
  return {
    hedged: status?.hedged ?? false,
    balance: { used: balance.used, free: balance.free, total: balance.total, upnl: balance.upnl,
      ...((venue.inverseNativeAmounts || venue.compareBalanceCoins) ? { coins: balance.coins ?? {} } : {}),
      ...(venue.compareBalanceCurrencies ? { currencies: balance.currencies ?? {} } : {}) },
    orders: orderRows,
    positions: positionRows,
    settings: settingRows,
    fills: fills.map((f) => ({ id: f.id, symbol: symbol(f.market), side: f.side === 0 ? 'buy' : 'sell', amount: f.amount, ...(venue.compareFillPrices ? { price: f.price } : {}) })),
  };
}

async function runTs() {
  const tape = new Tape('ts');
  const clock = isPrivate ? undefined : new ReplayClock(metadata?.epoch ?? Date.now());
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
      tape.close(this.socket, 1000, true);
    }
  }
  globalThis.WebSocket = TapeWebSocket;
  globalThis.fetch = async (input, init = {}) => {
    const request = input instanceof Request ? input : undefined;
    const method = (init.method ?? request?.method ?? 'GET').toUpperCase();
    const requestBody = init.body ?? (request && !['GET', 'HEAD'].includes(method) ? await request.clone().text() : undefined);
    const { status, body } = await tape.waitForResponse(tape.respond(method, request?.url ?? String(input), requestBody, Object.fromEntries(new Headers(init.headers ?? request?.headers).entries())));
    return new Response([204, 205, 304].includes(status) ? null : body, { status });
  };
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
    const { status, body, headers: responseHeaders } = await tape.waitForResponse(tape.respond((config.method ?? 'get').toUpperCase(), url.toString(), config.data, config.headers?.toJSON?.() ?? config.headers));
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
  await venue.loadTs?.({ isPrivate });
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
  const options = { ...venue.replayOptions };
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
  await venue.prepareTsTransport?.(exchange, adapter);
  const sampling = realInterval(() => { tape.peakBytes = Math.max(tape.peakBytes ?? 0, tape.sampleMemory()); }, 5);
  const started = exchange.start();
  const prepared = started.then(() => venue.prepareTsReady?.(exchange, { isPrivate }));
  const subscribed = (async () => {
    while (!store.markets.length || (venue.tsBooksReady && !venue.tsBooksReady(exchange, store))) await new Promise((r) => realTimeout(r, 1));
    for (const symbol of bookSymbols) exchange.listenOrderBook(symbol, (book) => {
      // KNOWN_TS_FIXES: BingX's feed supplies descending asks. Apply the
      // independent price-order contract before consumers and flush hashes.
      if (values.venue === 'bingx') {
        book.bids.sort((a, b) => b.price - a.price);
        book.asks.sort((a, b) => a.price - b.price);
      }
      finalBooks[symbol] = book;
    });
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
    async (accountReady = true) => {
      await (accountReady ? Promise.all([prepared, subscribed]) : subscribed);
      while (!venue.tsBooksSubscribed(exchange, bookSymbols.length)) await new Promise((r) => realTimeout(r, 5));
      while (venue.initialBookSnapshots && !replayBooksReady(bookSymbols, Object.keys(finalBooks))) await new Promise((r) => realTimeout(r, 5));
      const loaded = () => accountReady ? Object.values(store.loaded).every(Boolean)
        : store.loaded.markets && store.loaded.tickers;
      while ((!accountReady || isPrivate) && !loaded()) await new Promise((r) => realTimeout(r, 5));
    },
    (record) => runTsCommand(exchange, store, record, { candleListeners, candleUpdates }),
    clock,
  );
  await within(Promise.all([started, subscribed]), 15_000, 'TS startup/subscriptions');
  if (venue.tsReplaySettled) {
    const pending = (async () => { while (!venue.tsReplaySettled(exchange, bookSymbols.length)) {
      if (clock?.active) { await clock.advance(clock.time - clock.epoch + 1); await new Promise(setImmediate); }
      else await new Promise((resolve) => realTimeout(resolve, 1));
    } })();
    await within(pending, 15_000, 'TS pending book snapshots');
  }
  await within(venue.settleTs?.(exchange, { isPrivate, clock }) ?? Promise.resolve(), 15_000, 'TS pending venue reads');
  await settle();
  tape.assertWritesConsumed();
  postStore();
  if (!values['no-egress']) for (const [symbol, book] of Object.entries(finalBooks)) mainBooks[symbol] = structuredClone(book);
  realClearInterval(sampling);
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
        balance: { used: store.balance.used, free: store.balance.free, total: store.balance.total, upnl: store.balance.upnl,
          ...((venue.inverseNativeAmounts || venue.compareBalanceCoins) ? { coins: store.balance.coins ?? {} } : {}),
          ...(venue.compareBalanceCurrencies ? { currencies: store.balance.currencies ?? {} } : {}) },
        orders: store.orders.map((o) => ({ ...o })),
        // Zero-size placeholders carry each market's leverage and margin type.
        positions: store.positions.filter((p) => p.contracts > 0).map((p) => ({ ...p })),
        settings: venue.tsSettings?.(exchange, store) ?? [...new Map(store.positions.filter((p) => p.leverage > 0).sort((a, b) => a.contracts - b.contracts).map((p) => [p.symbol, { symbol: p.symbol, leverage: p.leverage, marginType: p.marginType }])).values()],
        fills: executions.map((f) => ({ id: f.orderId ?? f.id, symbol: f.symbol, side: f.side, amount: f.amount, ...(venue.compareFillPrices ? { price: f.price } : {}) })),
      }
    : undefined;
  const protocolErrors = tape.audit?.errors() ?? [];
  exchange.dispose();
  clock?.restore();
  return { frames, metrics, bookEvents, requests: [...tape.requests].sort(), writes: tape.writes, unserved: tape.unserved, unusedWrites: tape.unusedWrites(), protocolErrors, invalidSignatures: tape.invalidSignatures, wsRequests: [...tape.sentRequests].sort(), signatures: tape.signatureErrors, outbound: tape.outbound, subscriptions: values.venue === 'backpack' ? [...tape.nativeSubscriptions].sort() : (tape.hyperliquid || values.venue === 'bitrue' || values.venue === 'woo' || values.venue === 'lighter' || venue.family === 'orderly' || venue.subscriptionGenerations || values.venue === 'kucoin' || values.venue === 'asterdex' || values.venue === 'kraken' || values.venue === 'apexomni' || values.venue === 'paradex' || values.venue === 'phemex') ? [...tape.fullSubscriptions].sort() : tape.bitget ? [...tape.protocolSubscriptions].sort() : tape.subscriptions.sort(), socketErrors: tape.socketErrors, candles: candleUpdates, store: snapshot, books: bookCopies, account, commands: tape.results };
}

async function runTsCommand(exchange, store, record, { candleListeners, candleUpdates }) {
  const { op, ...args } = commandArgs(record);
  switch (op) {
    case 'placeOrders': return exchange.placeOrders(args.orders);
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
      const update = Object.fromEntries(['price', 'amount'].filter((key) => args[key] !== undefined).map((key) => [key, args[key]]));
      const result = await exchange.updateOrder({ order, update });
      return values.venue === 'mexc' ? result ?? [] : result;
    }
    case 'setLeverage': await exchange.setLeverage(args.symbol, args.leverage); return [];
    case 'setMarginType': await exchange.changeMarginType(args.isolated, args.symbol); return [];
    case 'setPositionMode': await exchange.changePositionMode(args.hedged); return [];
    case 'findOrderByClientOrderId':
      return exchange.findOrderByClientOrderId(args.symbol, args.clientOrderId);
    case 'fetchLeverage': return exchange.fetchLeverageForSymbol(args.symbol);
    case 'fetchStpMode': return exchange.fetchStpMode();
    case 'setStpMode': await exchange.changeStpMode(args.mode); return [];
    case 'getAccount':
    case 'fetchAccount': return exchange.getAccount();
    case 'fetchOrderBook': return exchange.fetchOrderBook(args.symbol, args.limit);
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
    ...(venue.compareClientIds ? { clientOrderId: o.clientOrderId ?? null } : {}),
    symbol: o.symbol,
    status: o.status,
    ...(values.venue === 'bitunix' ? {
      timeInForce: ({ GTC: 'GoodTillCancel', POST_ONLY: 'PostOnly', GTX: 'PostOnly', IOC: 'ImmediateOrCancel', FOK: 'FillOrKill' })[o.timeInForce] ?? o.timeInForce ?? 'GoodTillCancel',
      // TS keeps native identity in info; virtual protection rows share that
      // info with their parent, so only the native row owns its client ID.
      clientOrderId: o.clientOrderId || (String(o.info?.orderId) === String(o.id) ? o.info?.clientId : null) || null,
      parentId: o.parentId || (o.info?.orderId && ['tp', 'sl'].some((leg) => o.id === `${o.info.orderId}__${leg}`) ? String(o.info.orderId) : null),
    } : {}),
    ...(venue.compareClientIds && o.type === 'limit' ? { timeInForce: o.timeInForce ?? null } : {}),
    type: o.type,
    side: o.side,
    price: o.price,
    amount: o.amount,
    filled: o.filled,
    ...(venue.inverseNativeAmounts ? { remainingNative: o.extra?.remainingNative } : {}),
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
    settings: account.settings.map((s) => ({ symbol: s.symbol, marginType: s.marginType,
      ...(venue.independentLeverage ? { buyLeverage: s.buyLeverage, sellLeverage: s.sellLeverage } : { leverage: s.leverage }) })).sort(bySymbol),
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
    unusedWrites: result.unusedWrites,
    missingWrites: result.unusedWrites,
    invalidSignatures: result.invalidSignatures,
    protocolErrors: result.protocolErrors,
    wsRequests: result.wsRequests,
    metrics: result.metrics,
    markets: result.store.markets.filter(venue.marketFilter ?? (() => true)).map(marketFields),
    tickers: result.store.tickers.filter(venue.marketFilter ?? (() => true)).map((t) => Object.fromEntries([['symbol', t.symbol], ...tickerFields.map((f) => [f, f === 'openInterest' ? (t[f] ?? 0) : t[f]])])),
    books: Object.fromEntries(Object.entries(result.books).map(([s, b]) => [s, sortBook(b)])),
    account: result.account && normalizeAccount(result.account),
    commands: result.commands.map((result, index) => venue.commandResult?.(result,
      records.filter((record) => record.type === 'command')[index], index + 1) ?? result),
    signatures: result.signatures,
    outbound: result.outbound,
    subscriptions: result.subscriptions,
    candles: result.candles,
    socketErrors: result.socketErrors,
    bookEvents: result.bookEvents.sort((a, b) => a[0] - b[0] || a[1].localeCompare(b[1])),
  }),
);
realClearTimeout(replayDeadline);
process.exit(0);
