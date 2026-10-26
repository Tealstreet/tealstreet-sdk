// Opt-in public capture. This does not run a replay or reconstruct TS traffic.
import { createRequire } from 'node:module';
import * as fs from 'node:fs';
import { dirname, resolve } from 'node:path';
import { randomUUID } from 'node:crypto';
import { setTimeout as delay } from 'node:timers/promises';
import { axiosClients, venueOf } from './venues.mjs';
import { canonicalUrl } from './requests.mjs';
import { createRecordingPolicy, createRecordingTimeline, recordingReady } from './polling.mjs';

const credentials = /^(?:key|secret|password|username|passphrase|auth|authorization|cookie|(?:x[-_])?api[-_]?key|x-mbx-apikey|signature|(?:x[-_])?(?:access[-_])?token|sessionToken|applicationId|vault|remoteSigner|externalPublicData)$/i;
const utf8 = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true });
const present = (value) => value !== undefined && value !== null && value !== '' && value !== false;
const assert = (condition, message) => { if (!condition) throw new Error(message); };
const safeFields = (value) => {
  if (!value || typeof value !== 'object') return;
  for (const [key, entry] of Object.entries(value)) {
    assert(!credentials.test(key) || !present(entry), 'Public TS recording refuses credentials or external account/provider state');
    safeFields(entry);
  }
};

export function validateTsRecording({ name, venue = venueOf(name), options = {}, books, seconds, env = process.env }) {
  safeFields(options);
  // The frozen oracle (__tests__/reference/<venue>/*.exchange.ts) is the only TS adapter; any other test path is refused.
  const oracleAdapter = /\/__tests__\/reference\/[a-z]+\/[\w.-]+\.exchange\.ts$/.test(venue.ts?.module ?? '');
  assert(!venue.recordSession && (oracleAdapter || !venue.ts?.module.includes('__tests__')), 'Session/reference TS recording is unsupported');
  assert(name === 'binance', 'TS recorder supports audited Binance USD-M public traffic only');
  assert(Object.keys(options).every((key) => key === 'testnet') &&
    (options.testnet === undefined || typeof options.testnet === 'boolean'), 'Custom TS recording options/routing are unsupported');
  assert(!['HTTP_PROXY', 'HTTPS_PROXY', 'ALL_PROXY', 'http_proxy', 'https_proxy', 'all_proxy', 'NODE_USE_ENV_PROXY'].some((key) => present(env[key])),
    'Direct TS recording refuses ambient proxy settings');
  assert(Number.isFinite(seconds) && seconds > 0 && seconds <= 3600, 'Recording seconds must be in (0, 3600]');
  assert(Array.isArray(books) && books.length > 0 && books.length <= 20 && new Set(books).size === books.length &&
    books.every((symbol) => typeof symbol === 'string' && /^[A-Z0-9]{2,40}$/.test(symbol)), 'Expected distinct Binance book symbols');
  return venue;
}

export function atomicTsTape(output, { maxBytes = 256 * 1024 * 1024, io = fs } = {}) {
  const target = resolve(output);
  io.mkdirSync(dirname(target), { recursive: true });
  assert(!io.existsSync(target), 'Recording output already exists');
  const temporary = `${target}.${randomUUID()}.partial`;
  let fd = io.openSync(temporary, 'wx'), bytes = 0, published = false;
  return {
    write(record) {
      const payload = Buffer.from(`${JSON.stringify(record)}\n`);
      assert(bytes + payload.length <= maxBytes, 'Recording output exceeds byte limit');
      let offset = 0;
      while (offset < payload.length) {
        const count = io.writeSync(fd, payload, offset, payload.length - offset);
        assert(count > 0, 'Recording output write did not progress'); offset += count;
      }
      bytes += payload.length;
    },
    publish() {
      io.fsyncSync(fd); io.closeSync(fd); fd = undefined;
      // Same-directory link is atomic and refuses overwrite, including a race.
      io.linkSync(temporary, target); published = true; io.unlinkSync(temporary);
    },
    abort() {
      if (fd !== undefined) { io.closeSync(fd); fd = undefined; }
      if (io.existsSync(temporary)) io.unlinkSync(temporary);
    },
    get published() { return published; },
  };
}

export function createTsCapture({ write, axios, now = Date.now, epoch = now(), maxBytes = 8 * 1024 * 1024 }) {
  const controller = new AbortController(), pending = new Set(), sockets = new Set(), restorers = [];
  const ordinals = new Map();
  let ordinal = 0, stopped = false, ended = false, failure, queued = 0, queue = Promise.resolve();
  let rejectFailure;
  const failed = new Promise((_, reject) => { rejectFailure = reject; });
  failed.catch(() => {});
  const fail = (error) => { if (!failure) { failure = error; rejectFailure(error); } };
  const check = () => { if (failure) throw failure; };
  const track = (promise) => { pending.add(promise); promise.then(() => pending.delete(promise), () => pending.delete(promise)); return promise; };
  const emit = (record, size = 0, convert = () => record) => {
    if (ended) return;
    const charge = Math.max(256, size * 2 + Buffer.byteLength(JSON.stringify(record)));
    try { assert(Number.isFinite(size) && size >= 0 && size <= maxBytes && queued + charge <= maxBytes * 4, 'Recording pending/body byte limit exceeded'); }
    catch (error) { fail(error); return; }
    queued += charge;
    const t = now() - epoch;
    queue = queue.then(async () => {
      check();
      const converted = await convert();
      if (!ended) await write({ t, ...converted });
    }).catch(fail).finally(() => { queued -= charge; });
  };
  const publicUrl = (url) => {
    const parsed = new URL(url);
    assert(!parsed.username && !parsed.password && !parsed.hash && ['https:', 'http:', 'wss:', 'ws:'].includes(parsed.protocol), 'Unsupported public recording URL');
    for (const [key, value] of parsed.searchParams) assert(!/key|secret|sign|token|auth|account|vault/i.test(key) || !value,
      'Public recording refuses authenticated URL parameters');
    return parsed;
  };
  const publicRequest = (method, url, body, headers) => {
    check(); assert(!stopped, 'Recording dispatch is closed'); publicUrl(url);
    assert(['GET', 'HEAD'].includes(method), 'Public Binance recorder refuses non-read HTTP requests');
    assert(body === undefined || body === null || body === '', 'Read request body is unsupported');
    safeFields(headers?.toJSON?.() ?? headers);
  };
  const raw = (data) => {
    if (typeof data === 'string') { assert(Buffer.byteLength(data) <= maxBytes, 'Recording body byte limit exceeded'); return data; }
    if (data instanceof ArrayBuffer) data = new Uint8Array(data);
    assert(data instanceof Uint8Array && data.byteLength <= maxBytes, 'Unsupported/oversize raw recording body');
    return utf8.decode(data);
  };
  const payload = (data, binary) => {
    if (typeof data === 'string') return binary ? { binary: Buffer.from(data).toString('base64') } : { text: data };
    if (data instanceof ArrayBuffer) data = new Uint8Array(data);
    assert(data instanceof Uint8Array, 'Unsupported socket recording payload');
    return binary === false ? { text: raw(data) } : { binary: Buffer.from(data).toString('base64') };
  };
  const socketData = (type, url, socket, data, binary) => {
    // Consumers receive the same event after this listener. Retain the bytes
    // we observed even if their listener reuses or mutates the buffer.
    if (data instanceof ArrayBuffer) data = new Uint8Array(data.slice(0));
    else if (data instanceof Uint8Array) data = Buffer.from(data);
    const size = typeof data === 'string' ? Buffer.byteLength(data) : data?.size ?? data?.byteLength;
    emit({}, size, async () => ({ type, url, socket, ...payload(typeof Blob !== 'undefined' && data instanceof Blob ? await data.arrayBuffer() : data, binary) }));
  };
  const wrapWebSocket = (Real) => {
    if (!Real) return Real;
    class RecordingSocket extends Real {
      constructor(url, ...args) {
        check(); assert(!stopped, 'Recording dispatch is closed'); publicUrl(url);
        for (const arg of args) if (arg && typeof arg === 'object' && !Array.isArray(arg)) {
          assert(!arg.agent && !arg.createConnection, 'Custom socket routing is unsupported'); safeFields(arg);
        }
        super(url, ...args);
        const key = canonicalUrl(String(url)), socket = ordinals.get(key) ?? 0;
        ordinals.set(key, socket + 1); sockets.add(this);
        const safe = (fn) => (...values) => { try { fn(...values); } catch (error) { fail(error); } };
        if (typeof this.on === 'function') {
          this.on('open', safe(() => emit({ type: 'open', url: String(url), socket })));
          this.on('message', safe((data, binary) => socketData('ws', String(url), socket, data, binary)));
          this.on('close', safe((code) => { sockets.delete(this); emit({ type: 'close', url: String(url), socket, code }); }));
          this.on('error', fail);
        } else {
          this.addEventListener('open', safe(() => emit({ type: 'open', url: String(url), socket })));
          this.addEventListener('message', safe((event) => socketData('ws', String(url), socket, event.data)));
          this.addEventListener('close', safe((event) => { sockets.delete(this); emit({ type: 'close', url: String(url), socket, code: event.code }); }));
          this.addEventListener('error', () => fail(new Error('Public recording socket failed')));
        }
        const send = this.send;
        this.send = (data, ...sendArgs) => {
          // Cleanup may send a real unsubscribe on an existing socket. Capture
          // it before close; stop() forbids new sockets and HTTP dispatch.
          check(); assert(!ended, 'Recording capture has ended');
          {
            try {
              assert(!(typeof Blob !== 'undefined' && data instanceof Blob), 'Outgoing Blob inspection is unsupported');
              const text = raw(data);
              if (!['ping', 'pong'].includes(text)) {
                const parsed = JSON.parse(text); safeFields(parsed);
                assert(!/auth|login|sign/i.test(parsed?.method ?? parsed?.op ?? ''), 'Public recording refuses socket authentication');
                assert(['SUBSCRIBE', 'UNSUBSCRIBE', 'LIST_SUBSCRIPTIONS', 'SET_PROPERTY', 'GET_PROPERTY'].includes(parsed?.method),
                  'Unsupported outgoing public Binance socket payload');
              }
            } catch (error) { fail(error); throw error; }
          }
          socketData('ws-send', String(url), socket, data, sendArgs[0]?.binary);
          try { return send.call(this, data, ...sendArgs); } catch (error) { fail(error); throw error; }
        };
      }
    }
    Object.defineProperty(RecordingSocket, 'WebSocket', { value: RecordingSocket });
    return RecordingSocket;
  };
  const wrapAxios = (client) => {
    const original = client.defaults.adapter, adapter = axios.getAdapter(original);
    assert(adapter === axios.getAdapter('http'), 'Only the standard HTTP Axios adapter is supported');
    const capture = (config) => track((async () => {
      const method = (config.method ?? 'get').toUpperCase(), url = client.getUri(config);
      publicRequest(method, url, config.data, config.headers);
      assert(['json', 'text', 'arraybuffer', undefined].includes(config.responseType), 'Streaming/custom Axios response type is unsupported');
      assert(config.responseEncoding === undefined || /^utf-?8$/i.test(config.responseEncoding), 'Non-UTF-8 Axios capture is unsupported');
      assert(config.adapter === capture, 'Request-level Axios adapter override is unsupported');
      const requestOrdinal = ordinal++;
      const outgoing = { ...config, proxy: false, maxContentLength: Math.min(config.maxContentLength >= 0 ? config.maxContentLength : maxBytes, maxBytes),
        // HTTP's text mode strips BOM and replaces invalid UTF-8 before our
        // wrapper sees it. Capture bytes, then restore that adapter's caller
        // representation before Axios runs the original transformResponse.
        responseType: 'arraybuffer', maxBodyLength: maxBytes,
        signal: config.signal ? AbortSignal.any([config.signal, controller.signal]) : controller.signal };
      const received = (response) => {
        assert(response.data instanceof Uint8Array, 'HTTP adapter did not return raw capture bytes');
        const body = raw(response.data);
        if (config.responseType !== 'arraybuffer') response.data = !config.responseEncoding || config.responseEncoding === 'utf8'
          ? body.replace(/^\uFEFF/, '') : body;
        response.config = config;
        const headers = Object.fromEntries(Object.entries(response.headers?.toJSON?.() ?? response.headers ?? {})
          .map(([name, value]) => [name, Array.isArray(value) ? [...value] : value]));
        emit({ type: 'http', requestOrdinal, method, url, status: response.status, headers, body }, Buffer.byteLength(body));
      };
      let response;
      try { response = await adapter(outgoing); }
      catch (error) {
        if (!error.response) { fail(error); throw error; }
        response = error.response;
        received(response); error.config = config;
        throw error;
      }
      received(response);
      return response;
    })().catch((error) => { if (!error.response) fail(error); throw error; }));
    client.defaults.adapter = capture;
    const interceptor = client.interceptors.request.use((config) => {
      try { assert(config.adapter === capture, 'Request-level Axios adapter override is unsupported'); return config; }
      catch (error) { fail(error); throw error; }
    });
    restorers.push(() => { client.interceptors.request.eject(interceptor); client.defaults.adapter = original; });
  };
  const wrapFetch = (realFetch) => (input, init = {}) => track((async () => {
    const request = input instanceof Request ? input : undefined, method = (init.method ?? request?.method ?? 'GET').toUpperCase();
    const url = request?.url ?? String(input), body = init.body;
    assert(!request?.body, 'Streaming Request bodies are unsupported');
    publicRequest(method, url, body, Object.fromEntries(new Headers(init.headers ?? request?.headers).entries()));
    const requestOrdinal = ordinal++;
    const signal = init.signal ?? request?.signal;
    const response = await realFetch(input, { ...init, signal: signal ? AbortSignal.any([signal, controller.signal]) : controller.signal });
    const length = response.headers.get('content-length');
    assert(length !== null && /^\d+$/.test(length) && Number(length) <= maxBytes, 'Direct fetch requires a bounded finite Content-Length');
    const clone = response.clone();
    track((async () => {
      const reader = clone.body?.getReader(); const chunks = []; let count = 0;
      if (reader) try {
        while (true) {
          const { done, value } = await reader.read(); if (done) break;
          count += value.byteLength; assert(count <= maxBytes, 'Fetch capture exceeds byte limit'); chunks.push(value);
        }
      } catch (error) { reader.cancel(error).catch(() => {}); throw error; }
      assert(count === Number(length), 'Fetch capture is truncated or has inconsistent length');
      const body = raw(Buffer.concat(chunks));
      emit({ type: 'http', requestOrdinal, method, url, status: response.status,
        headers: Object.fromEntries(response.headers.entries()), body }, count);
    })().catch(fail));
    return response;
  })().catch((error) => { fail(error); throw error; }));
  return { emit, wrapWebSocket, wrapAxios, wrapFetch, check, failed, fail,
    stop() { stopped = true; }, abort() { controller.abort(); for (const socket of sockets) socket.terminate?.(); },
    async closeSockets() {
      await Promise.all([...sockets].map((socket) => new Promise((resolveClose) => {
        if (typeof socket.once === 'function') socket.once('close', resolveClose);
        else socket.addEventListener('close', resolveClose, { once: true });
        socket.close();
      })));
    },
    async drain() { while (pending.size) await Promise.allSettled([...pending]); await queue; check(); },
    restore() {
      ended = true; let error;
      for (const restore of restorers.reverse()) try { restore(); } catch (caught) { error ??= caught; }
      if (error) throw error;
    },
  };
}

const loadTransports = () => {
  const require = createRequire(new URL('../../packages/safe-cex/package.json', import.meta.url));
  const wsPath = require.resolve('ws'), NodeWebSocket = require('ws'), entry = require.cache[wsPath];
  return { axios: require('axios').default, NodeWebSocket,
    installNodeSocket(Socket) {
      require.cache[wsPath] = { ...entry, exports: Socket };
      return () => { require.cache[wsPath] = entry; };
    } };
};
const loadExchange = async (venue) => {
  const [{ [venue.ts.className]: Exchange }, { DefaultStore }, { getTransport }, { directTransport }] = await Promise.all([
    import(venue.ts.module), import('../../packages/safe-cex/src/store/store.base.ts'),
    import('../../packages/safe-cex/src/transport/registry.ts'), import('../../packages/safe-cex/src/transport/direct.ts'),
  ]);
  assert(getTransport({}) === directTransport, 'Ambient non-direct TS transport is unsupported');
  return { Exchange, DefaultStore };
};

export async function recordTsPublic({ name = 'binance', books = ['BTCUSDT', 'ETHUSDT'], seconds = 60, testnet = false,
  out, env = process.env, dependencies = {} }) {
  const venue = validateTsRecording({ name, books, seconds, options: { testnet }, env });
  const sleep = dependencies.delay ?? ((ms, signal) => delay(ms, undefined, { signal }));
  const lifetime = new AbortController();
  const pause = (ms) => sleep(ms, lifetime.signal);
  const now = dependencies.now ?? (() => performance.now()), startedAt = now();
  const epoch = dependencies.epoch ?? Date.now();
  const startupDeadline = startedAt + (dependencies.startupMs ?? 120000);
  const sink = dependencies.sink ?? atomicTsTape(out ?? `sdk-rs/harness/tapes/${name}-ts-public.jsonl`);
  const options = { testnet, publicDataOnly: true, tealstreetTunnel: false, tealstreetProxy: false, tealstreetWsRoute: false };
  const policy = createRecordingPolicy(venue, options), timeline = createRecordingTimeline(venue);
  const originals = { WebSocket: Object.getOwnPropertyDescriptor(globalThis, 'WebSocket'), fetch: Object.getOwnPropertyDescriptor(globalThis, 'fetch') };
  let capture, restoreNode, exchange, unsubscribe, error, loaded = false, frames = 0, httpResponses = 0;
  const seenBooks = new Set(), tickerSymbols = new Set(), unsubscribers = [];
  const bounded = async (promise, ms, label) => {
    const timer = new AbortController();
    try { return await Promise.race([promise, sleep(ms, timer.signal).then(() => { throw new Error(label); })]); }
    finally { timer.abort(); }
  };
  try {
    const transports = (dependencies.loadTransports ?? loadTransports)();
    capture = createTsCapture({ axios: transports.axios, now, epoch: startedAt, write(record) {
      if (record.type === 'ws') {
        frames++;
        if (record.text) for (const symbol of venue.recordTickerSymbols?.(record.text) ?? []) tickerSymbols.add(symbol);
      }
      if (record.type === 'http' && record.status >= 200 && record.status < 300) httpResponses++;
      return sink.write(timeline(policy.sanitize(record), loaded));
    } });
    Object.defineProperty(globalThis, 'WebSocket', { configurable: true, writable: true, value: capture.wrapWebSocket(globalThis.WebSocket) });
    Object.defineProperty(globalThis, 'fetch', { configurable: true, writable: true, value: capture.wrapFetch(globalThis.fetch) });
    restoreNode = transports.installNodeSocket(capture.wrapWebSocket(transports.NodeWebSocket));
    capture.emit({ type: 'meta', epoch, private: false, source: 'live-ts-public', venue: name, testnet });
    const { Exchange, DefaultStore } = await bounded(Promise.race([capture.failed,
      (dependencies.loadExchange ?? loadExchange)(venue)]), Math.max(0, startupDeadline - now()), 'TS recording loading/readiness timed out');
    const store = new DefaultStore(); exchange = new Exchange(venue.ts.name, options, store);
    unsubscribe = store.subscribe((state) => { loaded = state.loaded?.markets === true && state.loaded?.tickers === true; });
    for (const client of axiosClients(exchange)) capture.wrapAxios(client);
    await bounded(Promise.race([capture.failed, (async () => {
      const started = Promise.resolve().then(() => exchange.start()); started.catch(capture.fail);
      while (!store.markets.length || (venue.tsBooksReady && !venue.tsBooksReady(exchange, store))) {
        capture.check(); await pause(10);
      }
      for (const symbol of books) unsubscribers.push(exchange.listenOrderBook(symbol, () => seenBooks.add(symbol)));
      await started;
      while (!loaded || !books.every((symbol) => seenBooks.has(symbol))) { capture.check(); await pause(10); }
    })()]), Math.max(0, startupDeadline - now()), 'TS recording startup/readiness timed out');
    await Promise.race([capture.failed, pause(seconds * 1000)]);
  } catch (caught) { error = caught; }
  finally {
    capture?.stop();
    lifetime.abort();
    try {
      for (const cleanup of [...unsubscribers, () => exchange?.dispose(), unsubscribe]) {
        try { if (typeof cleanup === 'function') cleanup(); } catch (caught) { error ??= caught; }
      }
      if (capture) await bounded((async () => { await capture.closeSockets(); await capture.drain(); })(), dependencies.shutdownMs ?? 5000, 'TS recording cleanup did not complete');
    } catch (caught) { error ??= caught; capture?.abort(); }
    finally {
      try { capture?.restore(); } catch (caught) { error ??= caught; }
      try { restoreNode?.(); } catch (caught) { error ??= caught; }
      for (const [key, descriptor] of Object.entries(originals)) {
        try { if (descriptor) Object.defineProperty(globalThis, key, descriptor); else delete globalThis[key]; }
        catch (caught) { error ??= caught; }
      }
    }
  }
  try {
    if (error) throw error;
    assert(recordingReady(venue, { frames, httpResponses, loaded, books: seenBooks.size, requestedBooks: books.length }) &&
      books.every((symbol) => tickerSymbols.has(symbol)), 'TS recording lacks complete native public data/book coverage');
    sink.publish();
    return { name, out: out ?? `sdk-rs/harness/tapes/${name}-ts-public.jsonl`, books: [...seenBooks], frames, httpResponses, seconds, source: 'live-ts-public' };
  } finally { sink.abort(); }
}
