import { createHash, createHmac, timingSafeEqual } from 'node:crypto';
import { parseRequestJson } from './request-json.mjs';
import { Responses, implementationRecords } from './responses.mjs';
import { isRead, subscriptionKeys, requestKey as typedRequestKey, subscriptionMessage, selectResponse } from './requests.mjs';
import { setTimeout as realTimeout, clearTimeout as realClearTimeout } from 'node:timers';
import { LOADED_FLAG } from '../hosts/wasm/js/wire.mjs';

const apiSupported = (value) => value == null || value === '' ? true : typeof value === 'number' ? value !== 0 : typeof value === 'boolean' ? value : !['false','0','no','n','disabled','unsupported'].includes(String(value).trim().toLowerCase());

export const replayStoreReady = (loaded, isPrivate) => {
  const publicMask = LOADED_FLAG.MARKETS | LOADED_FLAG.TICKERS;
  const required = isPrivate
    ? publicMask | LOADED_FLAG.BALANCE | LOADED_FLAG.ORDERS | LOADED_FLAG.POSITIONS
    : publicMask;
  return (loaded & required) === required;
};
export const replayBooksReady = (required, available) => {
  const symbols = new Set(available);
  return required.every((symbol) => symbols.has(symbol));
};
/** Per-request values: clocks, signatures and generated client ids. */
const VOLATILE = new Set(['timestamp', 'recvWindow', 'signature', 'newClientOrderId', 'clientAlgoId', 'requestId']);
export const canonicalUrl = (url, callerIds = new Set(), venue = {}) => {
  const parsed = new URL(venue.requestUrl?.(url) ?? url);
  // Active trigger lists use a rolling lookback ending at request time.
  // Keep its width exact while allowing the two implementations' clock skew.
  if (
    !venue.requestUrl && venue.rollingListWindows &&
    /\/private\/(planorder|stoporder)\/list\/orders$/.test(parsed.pathname) &&
    parsed.searchParams.has('start_time') &&
    parsed.searchParams.has('end_time')
  ) {
    const end = Number(parsed.searchParams.get('end_time'));
    parsed.searchParams.set('start_time', String(Number(parsed.searchParams.get('start_time')) - end));
    parsed.searchParams.set('end_time', 'now');
  }

  const query = [...parsed.searchParams]
    .filter(([name, value]) => (!VOLATILE.has(name) && !venue.volatileQueryFields?.includes(name)) || callerIds.has(value))
    .map(([name, value]) => [name, /^[\[{]/.test(value) ? canonicalBody(value, callerIds, venue) : value])
    .sort(([a], [b]) => a.localeCompare(b));
  return `${parsed.origin}${parsed.pathname}?${new URLSearchParams(query)}`;
};

export const stable = (value) => {
  if (Array.isArray(value)) return value.map(stable);
  if (value === null || typeof value !== 'object') return value;
  return Object.fromEntries(
    Object.keys(value)
      .sort()
      .map((key) => [key, stable(value[key])]),
  );
};
export const sameCommandResults = (ts, rust) => JSON.stringify(stable(ts)) === JSON.stringify(stable(rust));

export const canonicalBody = (body, callerIds = new Set(), venue = {}, context = {}) => {
  if (!body) return '';
  const clean = (value, key = '') => {
    if (typeof value === 'bigint' && venue.canonicalIntegerStrings) return String(value);
    if (Array.isArray(value)) return value.map((v) => clean(v));
    if (value && typeof value === 'object')
      return Object.fromEntries(
        Object.entries(value)
          .filter(
            ([k, v]) =>
              v !== undefined &&
              !(
                ((!venue.matchRequestBody || venue.volatileBodyFields) &&
                  VOLATILE.has(k) && !venue.preserveBodyFields?.includes(k) &&
                  !callerIds.has(String(v))) ||
                (k === 'clientId' && !venue.preserveClientId && !callerIds.has(String(v)) &&
                  (!venue.ignoreGeneratedBodyId || venue.ignoreGeneratedBodyId(k, v, callerIds, context))) ||
                (venue.ignoredBodyFields?.includes(k) && !callerIds.has(String(v))) ||
                (venue.mexcProtocol && k === 'requestId' && !callerIds.has(String(v)))
              ),
          )
          .sort(([a], [b]) => a.localeCompare(b))
          .map(([k, v]) => [
            k,
            !callerIds.has(String(v)) && venue.generatedBodyId?.(k, v) ? '<generated>' : clean(v, k),
          ]),
      );
    if (typeof value === 'string' && !/id$/i.test(key) && /^-?(?:0|[1-9]\d*)(?:\.\d+)?$/.test(value)) {
      const [whole, fraction] = value.split('.');
      const trimmed = fraction?.replace(/0+$/, '');
      return trimmed ? `${whole}.${trimmed}` : whole;
    }
    return value;
  };
  try {
    let parsed =
      typeof body === 'string'
        ? parseRequestJson(body)
        : ArrayBuffer.isView(body) || body instanceof ArrayBuffer
          ? parseRequestJson(new TextDecoder().decode(body))
          : body;
    if (venue.requestBody) parsed = venue.requestBody(parsed, { url: 'https://api.mexc.com/api/v1/private/order/cancel', ...context });
    if (venue.requestBodyParams && parsed?.method && parsed?.params) parsed = parsed.params;
    return JSON.stringify(clean(parsed), (_key, value) =>
      typeof value === 'bigint' ? { integer: String(value) } : value,
    );
  } catch {
    return String(body);
  }
};

export const withTimeout = async (promise, label, ms = 30000) => {
  let timer;
  try {
    return await Promise.race([
      promise,
      new Promise((_, reject) => {
        timer = realTimeout(() => reject(new Error(`replay timeout: ${label}`)), ms);
      }),
    ]);
  } finally {
    realClearTimeout(timer);
  }
};

export async function bodyBytes(body) {
  if (body instanceof Uint8Array) return body;
  const chunks = [];
  let length = 0;
  for await (const chunk of body) {
    chunks.push(chunk);
    length += chunk.length;
  }
  const bytes = new Uint8Array(length);
  let offset = 0;
  for (const chunk of chunks) {
    bytes.set(chunk, offset);
    offset += chunk.length;
  }
  return bytes;
}

// Public tapes run faster than real time. Timers must follow the recorded
// clock so a faster implementation does not skip a retry the slower one runs.
export class ReplayClock {
  constructor(epoch) {
    this.epoch = epoch;
    this.time = epoch;
    this.active = false;
    this.timers = new Map();
    this.original = { now: Date.now, setTimeout, clearTimeout, setInterval, clearInterval };
  }
  install() {
    Date.now = () => (this.active ? this.time : this.original.now());
    globalThis.setTimeout = (fn, ms = 0, ...args) => this.schedule(fn, ms, args, false);
    globalThis.setInterval = (fn, ms = 0, ...args) => this.schedule(fn, ms, args, true);
    globalThis.clearTimeout = globalThis.clearInterval = (timer) => {
      if (this.timers.has(timer)) {
        this.original.clearTimeout(timer.handle);
        this.timers.delete(timer);
      } else this.original.clearTimeout(timer);
    };
  }
  schedule(fn, ms, args, repeat) {
    this.invalidateStartup();
    const timer = {
      fn,
      args,
      delay: Math.max(repeat ? 1 : 0, Number(ms) || 0),
      repeat,
      ref() {
        return this;
      },
      unref() {
        return this;
      },
    };
    timer.at = Date.now() + timer.delay;
    this.timers.set(timer, timer);
    if (!this.active) timer.handle = this.original.setTimeout(() => this.fire(timer), timer.delay);
    return timer;
  }
  fire(timer) {
    this.invalidateStartup();
    if (!this.timers.has(timer)) return;
    if (timer.repeat) {
      timer.at = Date.now() + timer.delay;
      if (!this.active) timer.handle = this.original.setTimeout(() => this.fire(timer), timer.delay);
    } else this.timers.delete(timer);
    timer.fn(...timer.args);
  }
  beginStartup() {
    this.invalidateStartup();
    this.start(0);
    this.startup = true;
  }
  start(elapsed = 0) {
    // Public records are anchored before construction. Readiness does not
    // establish a new epoch: genuine startup timers have already spent time.
    if (this.startup) { this.startup = false; return; }
    const previous = this.active ? this.time : this.original.now();
    for (const timer of this.timers.values()) {
      this.original.clearTimeout(timer.handle);
      timer.at = this.epoch + elapsed + Math.max(0, timer.at - previous);
    }
    this.active = true;
    this.startup = false;
    this.time = this.epoch + elapsed;
  }
  work = new Set();
  holds = new Set();
  invalidateStartup() { this.startupSettled = false; }
  hold() {
    this.invalidateStartup();
    const token = {};
    this.holds.add(token);
    return () => { if (this.holds.delete(token)) this.invalidateStartup(); };
  }
  beginWork() {
    this.invalidateStartup();
    const token = {};
    this.work.add(token);
    return () => { if (this.work.delete(token)) this.invalidateStartup(); };
  }
  async drainStartup(cancelled = () => false) {
    if (!this.startup) return;
    // Every externally pending operation is owned explicitly. Event turns only
    // drain its continuations; they are not a substitute for the I/O guard.
    while (this.startup && !cancelled()) {
      await new Promise(setImmediate);
      if (!this.startup || cancelled()) return;
      if (this.holds.size || this.startupBlocked?.()) continue;
      // A full event turn drains runnable promise continuations. External I/O
      // remains explicitly owned; new observer work repeats this same guard.
      if (this.observeStartup?.()) { this.invalidateStartup(); continue; }
      this.startupSettled = true;
      return;
    }
  }
  nextTimer(ceiling = Infinity) {
    let next;
    for (const timer of this.timers.values())
      if (timer.at <= ceiling && (!next || timer.at < next.at)) next = timer;
    return next;
  }
  async until(predicate, label, timeoutMs = 30000, ceiling = Infinity) {
    if (this.startupOwner) throw new Error('Concurrent logical startup scheduler');
    this.startupOwner = true;
    let stopped = false;
    const run = async () => {
      while (!stopped) {
        await this.drainStartup(() => stopped);
        if (stopped) return;
        if (predicate()) return;
        const next = this.nextTimer(this.epoch + ceiling);
        if (next) {
          this.time = Math.max(this.time, next.at);
          this.fire(next);
        } else await new Promise(setImmediate);
      }
    };
    const remaining = this.startupDeadline === undefined ? timeoutMs
      : Math.max(1, Math.min(timeoutMs, this.startupDeadline - performance.now()));
    let completed = false;
    try { const result = await withTimeout(run(), label, remaining); completed = true; return result; }
    finally { stopped = true; this.startupOwner = false; if (!completed) this.invalidateStartup(); }
  }
  async advance(t, reuseStartupSettlement = false) {
    if (!this.active) return;
    const target = Math.max(this.time, this.epoch + t);
    if (this.startup) {
      if (reuseStartupSettlement && this.startupSettled && !this.holds.size && !this.startupBlocked?.() && !this.nextTimer(target)) {
        this.time = target;
        return;
      }
      await this.until(() => {
        if (this.nextTimer(target)) return false;
        this.time = target;
        return true;
      }, 'Logical startup timeline', this.startupTimeoutMs ?? 30000, t);
      return;
    }
    for (;;) {
      const next = this.nextTimer(target);
      if (!next) break;
      this.time = next.at;
      this.fire(next);
      if (this.startup) await this.drainStartup();
      else await new Promise(setImmediate);
    }
    this.time = target;
  }
  async wait(promise, label, timeoutMs, { maxSteps = Infinity, maxIdleSteps = Infinity, progress } = {}) {
    let done = false;
    const result = Promise.resolve(promise).finally(() => {
      done = true;
    });
    const pump = (async () => {
      let steps = 0, idle = 0, previous = progress?.();
      while (!done) {
        await new Promise(setImmediate);
        if (done) break;
        if (++steps > maxSteps) throw new Error(`replay timeout: ${label} virtual step budget exceeded`);
        const current = progress?.();
        if (current !== previous) { previous = current; idle = 0; }

        let next;
        for (const timer of this.timers.values()) if (!next || timer.at < next.at) next = timer;
        if (next) {
          idle = 0;
          this.time = Math.max(this.time, next.at);
          this.fire(next);
        } else if (++idle > maxIdleSteps) throw new Error(`replay timeout: ${label} idle step budget exceeded`);
      }
      return result;
    })();
    try {
      return await withTimeout(Promise.race([result, pump]), label, timeoutMs);
    } finally {
      done = true;
    }
  }
  restore() {
    this.invalidateStartup();
    this.startup = false;
    this.active = false;
    for (const timer of this.timers.values()) this.original.clearTimeout(timer.handle);
    this.timers.clear();
    Date.now = this.original.now;
    for (const name of ['setTimeout', 'clearTimeout', 'setInterval', 'clearInterval'])
      globalThis[name] = this.original[name];
  }
}

const bitgetTopic = (arg) => `${arg.instType ?? ''}|${arg.channel ?? arg.topic}|${arg.instId ?? arg.symbol ?? ''}`;

export const deliverSocketFrame = (socket, text, audit, validate) => {
  audit?.received(socket.rawUrl ?? socket.url, text, socket); validate?.(socket, text);
  (socket.onData ?? socket.onMessage)(text);
};

// Bitget mints client identities only on specific placement/modify routes.
const bitgetContract = (callerIds, venue) => {
  const generatedAliases = new Map();
  const mintedField = (key, url) => {
    const path = new URL(url).pathname;
    return key === 'newClientOid'
      ? /\/modify-order$/.test(path)
      : key === 'clientOid' &&
          /\/(?:batch-place-order|place-order|place-tpsl-order|place-plan-order|place-strategy-order)$/.test(path);
  };
  const canonicalBody = (body, url) => {
    if (body === undefined || body === null || body === '') return '';
    let parsed = body;
    try {
      if (typeof body === 'string') parsed = parseRequestJson(body);
      else if (ArrayBuffer.isView(body) || body instanceof ArrayBuffer)
        parsed = parseRequestJson(new TextDecoder().decode(body));
    } catch {
      return String(body);
    }
    if (venue.requestBody) parsed = venue.requestBody(parsed);
    const sorted = (value) => {
      if (Array.isArray(value)) return value.map(sorted);
      if (value === null || typeof value !== 'object') return value;
      const keys = Object.keys(value)
        .filter(
          (key) =>
            value[key] !== undefined &&
            !(venue.volatileBodyFields && VOLATILE.has(key) && !callerIds.has(String(value[key]))),
        )
        .sort();
      return Object.fromEntries(
        keys.map((key) => [
          key,
          ((mintedField(key, url) && typeof value[key] === 'string' && value[key].length) ||
            venue.generatedBodyId?.(key, value[key])) &&
          !callerIds.has(value[key])
            ? '<generated>'
            : key === 'clientOid' && typeof value[key] === 'string'
                ? (generatedAliases.get(value[key]) ?? value[key])
                : sorted(value[key]),
        ]),
      );
    };
    return JSON.stringify(sorted(parsed), (_key, value) =>
      typeof value === 'bigint' ? { integer: String(value) } : value,
    );
  };
  const generated = new Set();
  return {
    requestKey: (method, url, body) => `${method} ${canonicalUrl(url, callerIds, venue)}${method !== 'GET' ? ` ${canonicalBody(body, url)}` : ''}`,
    match: (record, method, url, body) => {
      if (method !== 'GET' && /(?:place|modify)-/.test(new URL(url).pathname)) {
        const ids = [];
        const visit = (value) => {
          if (!value || typeof value !== 'object') return;
          for (const [key, item] of Object.entries(value)) {
            if (mintedField(key, url) && (key === 'newClientOid' || !callerIds.has(item))) {
              if (
                typeof item !== 'string' ||
                !/^[.A-Za-z0-9:/_-]{1,32}$/.test(item) ||
                (key === 'newClientOid' && callerIds.has(item))
              )
                throw new Error(`invalid generated ${key}`);
              ids.push(item);
            } else visit(item);
          }
        };
        visit(typeof body === 'string' ? parseRequestJson(body) : body);
        const reply = JSON.parse(record.body);
        // A rejected signature may be re-signed once with the same identity.
        if (String(reply.code) !== '40009') {
          if (new Set(ids).size !== ids.length) throw new Error('generated client ID reused within batch');
          for (const id of ids) if (generated.has(id)) throw new Error(`generated client ID reused: ${id}`);
          for (const id of ids) generated.add(id);
        }
      }
      if (method !== 'GET') {
        const decode = (value) => (typeof value === 'string' ? parseRequestJson(value) : value);
        const bind = (expected, actual) => {
          if (!expected || !actual || typeof expected !== 'object' || typeof actual !== 'object') return;
          for (const key of Object.keys(expected)) {
            if (
              mintedField(key, url) &&
              typeof expected[key] === 'string' &&
              typeof actual[key] === 'string' &&
              !callerIds.has(actual[key])
            )
              generatedAliases.set(actual[key], expected[key]);
            else bind(expected[key], actual[key]);
          }
        };
        bind(decode(record.requestBody), decode(body));
      }
    },
  };
};

/** Serves recorded HTTP responses and websocket frames in tape order. */
export class Tape {
  constructor(records, venue = {}, isPrivate = venue.isPrivate ?? false, timeoutMs = venue.timeoutMs ?? 30000, impl = venue.impl, books = venue.books ?? [], options = venue.replayOptions) {
    records = implementationRecords(records, impl);
    this.records = records;
    this.impl = impl;
    this.canonicalize = venue.canonicalize ?? ((url) => canonicalUrl(url, this.callerIds, venue));
    this.explicitOpens = venue.explicitOpens ?? false;
    this.legacyLatest = venue.legacyLatest ?? true;
    this.venue = venue;
    this.liveSmoke = records.some((record) => record.type === 'meta' && record.liveSmoke === true);
    this.bootstrapFrame = (frame) => venue.bootstrapFrame?.(frame) || (this.liveSmoke && ['open', 'close', 'ws'].includes(frame.type) && frame.bootstrap === true);
    this.isPrivate = isPrivate;
    this.timeoutMs = timeoutMs;
    this.books = books;
    this.options = options ?? { key: 'replay-key', secret: 'replay-secret', passphrase: 'replay-passphrase',
      ...venue.options, ...venue.replayCredentials };
    this.validateRequest = venue.createRequestValidator?.(this.options) ?? venue.validateRequest;
    this.commandIndex = 0;
    this.time = 0;
    this.generations = new Map();
    this.recordedSocketRequests = new Map();
    this.recordedSocketQueues = new Map();
    for (const record of records.filter((r) => r.type === 'ws-send')) {
      let message;
      try { message = JSON.parse(record.text); } catch { continue; /* Literal heartbeat. */ }
      const request = venue.socketRequest?.(message);
      if (!request) continue;
      const scope = `${this.canonicalize(record.url)} ${record.connection ?? record.socket ?? 0}`;
      const key = `${scope} ${request.id}`;
      if (this.recordedSocketRequests.has(key)) throw new Error(`duplicate taped websocket request ${key}`);
      this.recordedSocketRequests.set(key, request.key);
      const queueKey = `${scope} ${request.key}`;
      if (!this.recordedSocketQueues.has(queueKey)) this.recordedSocketQueues.set(queueKey, []);
      this.recordedSocketQueues.get(queueKey).push(request.id);
    }
    this.startupSubscriptions = new Map(
      Array.from(venue.startupSubscriptions?.(records, books) ?? [], (key) => [key, false]),
    );
    this.pendingStartupSubscriptions = this.startupSubscriptions.size;
    this.requestAudit = venue.createRequestAudit?.();
    this.audit = venue.createSocketAudit?.(impl, { isPrivate, books, tickers: venue.publicTickers?.(records), records, ...(this.requestAudit ? { requestAudit: this.requestAudit } : {}) });
    if (venue.polling) { this.selectedPollingResponses = new Set(); this.polledRequestKeys = new Set(); }
    this.callerIds = new Set();
    const collectCallerIds = (value) => {
      if (!value || typeof value !== 'object') return;
      if (typeof value.clientOrderId === 'string') this.callerIds.add(value.clientOrderId);
      for (const nested of Object.values(value)) collectCallerIds(nested);
    };
    for (const record of records) if (record.type === 'command') collectCallerIds(record);
    this.bitget = venue.rust?.startsWith('bitget') ?? false;
    this.hyperliquid = venue.rust === 'hyperliquid';
    this.contract = this.bitget ? bitgetContract(this.callerIds, venue) : undefined;
    this.requestKey = this.contract?.requestKey ?? venue.requestKey?.bind(venue) ?? (venue.mexcProtocol ? (method, url, body) => typedRequestKey(method, url, body, this.callerIds, venue) : undefined);
    this.bodyKey = (body, url, method) => canonicalBody(body, this.callerIds, {
      ...venue,
      preserveClientId: venue.preserveClientId || (venue.rust === 'backpack' &&
        (method !== 'POST' || !/\/api\/v1\/orders?$/.test(new URL(url).pathname))),
    }, { method, url });
    this.responses = venue.responses ?? new Map();
    this.writeOrder = [];
    if (!venue.responses) for (const record of records.filter((r) => r.type === 'http')) {
      // Per-fixture KNOWN_TS_FIXES exception; never a fallback for Rust.
      const expectedBody = impl === 'ts' ? (record.tsRequestBody ?? record.requestBody) : record.requestBody;
      const key = `${record.method} ${canonicalUrl(record.url, this.callerIds, this.venue)}${expectedBody === undefined ? '' : ` BODY ${this.bodyKey(expectedBody, record.url, record.method)}`}`;
      if (!this.responses.has(key)) this.responses.set(key, []);
      this.responses.get(key).push(record);
      if (this.isWrite(record.method, record.url)) this.writeOrder.push(record);
    }
    if (!venue.responses && (this.contract || venue.checkRequest || venue.requestKey || venue.requestBodies || venue.requestBody || venue.validateMatchedRequest || venue.polling)) {
      const responseKey = (method, url, body) => {
        if (this.requestKey) return this.requestKey(method, url, body, this.callerIds);
        if (venue.requestBodies || venue.requestBody || venue.polling) return typedRequestKey(method, url, body, this.callerIds, venue);
        const base = `${method} ${canonicalUrl(url, this.callerIds, this.venue)}`;
        const exact = `${base} BODY ${canonicalBody(body, this.callerIds, venue)}`;
        return this.responses.has(exact) ? exact : base;
      };
      const responseRecords = records.map((record) =>
        impl === 'ts' && record.tsRequestBody !== undefined
          ? { ...record, requestBody: record.tsRequestBody, comparisonRequestBody: record.requestBody }
          : record,
      );
      // Ordering and matching must inspect the same exact fixture alternative.
      this.writeOrder = responseRecords.filter((record) => record.type === 'http' && this.isWrite(record.method, record.url));
      this.responseStore = new Responses(
        responseRecords,
        responseKey,
        venue.checkRequest,
        this.isWrite.bind(this),
        undefined,
        impl,
      );
    }
    // Share immutable canonical URL strings across every frame and both samples.
    const urls = new Map();
    const recordedContext = { books, isPrivate, tickers: venue.publicTickers?.(records) ?? [] };
    this.frames = records.filter((r) => (['ws', 'command', 'open', 'close', 'wait'].includes(r.type) || (venue.polling && r.type === 'http'))).map((frame) => {
      frame = venue.recordedFrame?.(frame, recordedContext) ?? frame;
      if (frame.type === 'wait' && (!Number.isFinite(frame.t) || frame.t < 0)) throw new Error('Invalid replay wait timestamp');
      if (frame.binary !== undefined) frame = { ...frame, data: Uint8Array.from(Buffer.from(frame.binary, 'base64')) };
      if (frame.url) {
        if (!urls.has(frame.url)) urls.set(frame.url, this.canonicalize(frame.url));
        frame = { ...frame, url: urls.get(frame.url), connection: frame.connection ?? frame.socket };
      }
      return venue.prepareFrame ? venue.prepareFrame(frame) : frame;
    });
    this.lifecycleStreams = new Map();
    this.lifecycleSockets = new Map();
    if (venue.subscriptionRouting) for (const frame of this.frames) {
      if (frame.type !== 'ws' || frame.connection === undefined) continue;
      const stream = venue.streamOf?.(frame.data ?? frame.text);
      if (!stream) continue;
      const key = `${frame.url} ${frame.connection}`;
      if (!this.lifecycleStreams.has(key)) this.lifecycleStreams.set(key, new Set());
      this.lifecycleStreams.get(key).add(stream);
    }
    // Legacy captures omit connect anchors. Only the open cohort before the
    // first close may bootstrap readiness; later opens still drive reconnects.
    this.socketLifecycle = new Map();
    for (const frame of this.frames) {
      if (!['open', 'close', 'ws'].includes(frame.type)) continue;
      let lifecycle = this.socketLifecycle.get(frame.url);
      if (!lifecycle) {
        lifecycle = { hasOpen: false, initial: new Set(), pastInitial: false };
        this.socketLifecycle.set(frame.url, lifecycle);
      }
      if (frame.type === 'close') lifecycle.pastInitial = true;
      else if (frame.type === 'open') {
        lifecycle.hasOpen = true;
        if (!lifecycle.pastInitial) lifecycle.initial.add(frame.connection ?? lifecycle.initial.size);
      } else if (!lifecycle.pastInitial && frame.connection !== undefined) lifecycle.initial.add(frame.connection);
    }
    this.startupDelivered = new Set();
    this.sockets = [];
    this.open = new Map();
    this.connectionCounts = new Map();
    this.byConnection = new Map();
    this._unserved = 0;
    this.waiters = new Set();

  }

  get responseTime() {
    return this.replayClock?.active ? this.replayClock.time - this.replayClock.epoch : this.time ?? 0;
  }

  activeCommands = 0;
  async executeCommand(frame, runCommand) {
    this.commandIndex = (this.commandIndex ?? 0) + 1;
    this.activeCommands++;
    try {
      const result = Promise.resolve().then(() => runCommand(frame));
      return await (this.replayClock?.active ? this.replayClock.wait(result, `command ${frame.op}`, this.venue.commandTimeoutMs ?? this.timeoutMs) : withTimeout(result, `command ${frame.op}`, this.venue.commandTimeoutMs ?? this.timeoutMs));
    }
    finally { this.activeCommands--; }
  }

  pendingResponses = new Set();
  responseDeliveries = new WeakMap();
  async waitForResponse(response) {
    // A recorded completion follows any depth frames received while its request
    // was in flight. Serialized commands cannot await a later tape timer; their
    // replies remain immediate, as do startup and private replies.
    if (!this.isPrivate && !this.activeCommands && this.replayClock?.active && response.recordedAt > this.responseTime) {
      const completion = { at: response.recordedAt };
      this.pendingResponses.add(completion);
      try {
        await new Promise((resolve) => setTimeout(resolve, completion.at - this.responseTime));
      } finally { this.pendingResponses.delete(completion); }
    }
    const delivery = this.responseDeliveries.get(response);
    if (delivery) { this.responseDeliveries.delete(response); delivery(); }
    return response;
  }

  async finishResponses() {
    if (this.isPrivate || !this.replayClock?.active) return;
    // Advance only to selected HTTP completions. Other recurring timers do not
    // extend the replay; any callbacks before that completion retain tape order.
    while (this.pendingResponses.size) {
      const next = Math.min(...Array.from(this.pendingResponses, (response) => response.at));
      this.time = Math.max(this.responseTime, next);
      await this.replayClock.advance(this.time);
      await new Promise(setImmediate);
    }
  }

  get unserved() { return this.responseStore?.unserved ?? this._unserved; }
  invalidSignatures = 0;
  fullSubscriptions = new Set();
  nativeSubscriptions = new Set();
  subscriptions = new Set();
  protocolSubscriptions = new Set();
  isWrite(method, url) {
    return this.venue.isWrite?.(method, url) ?? !isRead(method, url);
  }
  get unusedWrites() { return this.missingWrites; }
  get missingWrites() {
    if (typeof this.responses.unusedWrites === 'function') return this.responses.unusedWrites();
    if (this.responseStore) return this.responseStore.unusedWrites();
    return [...this.responses.values()].flat().filter((record) => this.isWrite(record.method, record.url)).length;
  }
  requests = new Set();
  writes = [];
  respond(method, url, body, headers) {
    let response;
    const rejectedBefore = this.unserved;
    try { response = this.respondMatched(method, url, body, headers); }
    catch (error) {
      if (this.selectedPollingResponses && this.unserved === rejectedBefore) {
        if (this.responseStore) this.responseStore.unserved++;
        else this._unserved++;
      }
      this.pollingFence?.reject(error); throw error;
    }
    if (response.status === 599) this.pollingFence?.reject(new Error('Polling replay sent an unmatched HTTP request'));
    if (this.requestAudit && response.status !== 599) {
      const delivered = () => this.requestAudit.response({ method, url, body, headers, response });
      // Standalone protocol probes have no asynchronous transport. Production
      // activates sessions only after waitForResponse delivers recorded bytes.
      if (this.venue.deferResponseAudit || this.replayClock) this.responseDeliveries.set(response, delivered);
      else delivered();
    }
    const compared = Object.hasOwn(response, 'comparisonRequestBody') ? response.comparisonRequestBody : body;
    const key = this.requestKey ? this.requestKey(method, url, compared, this.callerIds)
      : typedRequestKey(method, url, compared, this.callerIds, this.venue);
    this.requests.add(key);
    if (this.isWrite(method, url)) this.writes.push(key);
    return response;
  }

  respondMatched(method, url, body, headers) {
    try { this.validateRequest?.({ method, url, body, headers, callerIds: this.callerIds }); this.requestAudit?.validate({ method, url, body, headers }); }
    catch (error) { this.invalidSignatures++; throw error; }
    if ((this.isPrivate || this.venue.verifyPublicRequest) && this.venue.verifyRequest) {
      try { if (!this.venue.verifyRequest({ method, url, body, headers, callerIds: this.callerIds })) throw new Error(`invalid replay signature: ${method} ${url}`); }
      catch (error) { this.invalidSignatures++; throw error; }
    }
    const validateMatch = (record) => {
      this.contract?.match(record, method, url, body);
      if (this.selectedPollingResponses && !this.isWrite(method, url)) {
        const key = this.responseStore?.key(method, url, body) ?? this.requestKey(method, url, body, this.callerIds);
        this.polledRequestKeys.add(key); this.selectedPollingResponses.add(record);
        this.pollingFence?.matched(key, record);
      }
      if (this.venue.compareWriteOrder && this.isWrite(method, url) && this.writeOrder[0] !== record)
        throw new Error('Unexpected replay write order');
      this.venue.validateMatchedRequest?.(record, { method, url, body, headers, commandIndex: this.commandIndex });
      if (this.isWrite(method, url)) this.writeOrder.splice(this.writeOrder.indexOf(record), 1);
    };
    if (typeof this.responses.respond === 'function') return this.responses.respond(method, url, body, this.commandIndex, this.responseTime,
      this.venue.validateMatchedRequest || this.selectedPollingResponses ? validateMatch : undefined, headers);
    const now = this.responseTime;
    if (this.responseStore) {
      if (this.venue.compareWriteOrder && this.isWrite(method, url)) {
        const first = this.writeOrder[0];
        if (!first || this.responseStore.key(method, url, body) !== this.responseStore.key(first.method, first.url, first.requestBody)) {
          this.responseStore.unserved++;
          this._unserved = this.responseStore.unserved;
          return { status: 599, body: '' };
        }
      }
      const response = this.responseStore.respond(method, url, body, this.commandIndex, now,
        this.contract || this.venue.validateMatchedRequest || this.venue.compareWriteOrder || this.selectedPollingResponses ? validateMatch : undefined, headers);
      this._unserved = this.responseStore.unserved;
      return response;
    }
    const key = `${method} ${canonicalUrl(url, this.callerIds, this.venue)}`;
    const bodyKey = this.bodyKey(body, url, method);
    const queue = this.responses.get(`${key} BODY ${bodyKey}`) ??
      ((!this.isWrite(method, url) || !bodyKey || bodyKey === '{}') ? this.responses.get(key) : undefined);
    if (!queue?.length) {
      if (this.unserved < 10)
        console.error(`unserved ${method} ${url} ${canonicalBody(body, this.callerIds, this.venue)}`);
      this._unserved++;
      return { status: 599, body: '' };
    }
    // Reconciliation reads may advance to a new fixture stage after a command.
    const eligible = queue.filter(
      (r) => (r.afterCommand ?? 0) <= this.commandIndex && Math.max(r.availableAfter ?? 0, r.effectiveAt ?? 0, r.availableAt ?? 0, r.after ?? 0) <= now,
    );
    if (!eligible.length) {
      this._unserved++;
      return { status: 599, body: '' };
    }
    const stage = Math.max(...eligible.map((r) => r.afterCommand ?? 0));
    const current = eligible.filter((r) => (r.afterCommand ?? 0) === stage);
    const timed = current.filter((record) => (record.availableAfter ?? record.effectiveAt ?? 0) <= now);
    const timedRead =
      !this.isWrite(method, url) &&
      current.some((record) => record.availableAfter !== undefined || record.effectiveAt !== undefined || record.availableAt !== undefined || record.after !== undefined);
    const record = timedRead ? timed.at(-1) : current[0];
    if (!record) {
      this._unserved++;
      return { status: 599, body: '' };
    }
    if (this.isWrite(method, url) && this.venue.compareWriteOrder && this.writeOrder[0] !== record) {
      this._unserved++;
      return { status: 599, body: '' };
    }
    if (this.isWrite(method, url)) this.writeOrder.splice(this.writeOrder.indexOf(record), 1);
    if (!timedRead && (this.isWrite(method, url) || current.length > 1)) queue.splice(queue.indexOf(record), 1);
    let responseBody = record.body;
    if (/"\$(clientAlgoId|newClientOrderId)"/.test(responseBody)) {
      const sent = typeof body === 'string' ? parseRequestJson(body) : (body ?? {});
      responseBody = responseBody.replace(/"\$(clientAlgoId|newClientOrderId)"/g, (_, field) =>
        JSON.stringify(sent[field] ?? null),
      );
    }
    return { status: record.status, headers: record.headers ?? '', body: responseBody,
      // Only an exactly validated fixture exception may repair the TS oracle.
      ...(this.impl === 'ts' && record.tsRequestBody !== undefined ? { comparisonRequestBody: record.requestBody } : {}),
      ...(Number.isFinite(record.t) ? { recordedAt: record.t } : {}) };
  }

  async finishPolling(clock) {
    if (!this.selectedPollingResponses) return;
    if (this.unserved) throw new Error('Polling replay contains unmatched HTTP requests');
    const targets = new Map();
    for (const [key, queue] of (this.responseStore ?? this.responses).queues) {
      if (!this.polledRequestKeys.has(key) || !queue.length || this.isWrite(queue[0].method, queue[0].url)) continue;
      const target = selectResponse([...queue], this.time, { commandIndex: this.commandIndex });
      if (target && (target.availableAt !== undefined || target.effectiveAt !== undefined || target.availableAfter !== undefined)
          && !this.selectedPollingResponses.has(target)) targets.set(key, target);
    }
    if (!targets.size) return;
    const label = `polling final HTTP anchors (${[...targets.values()].map((record) => `${record.method} ${new URL(record.url).pathname}`).join(', ')})`;
    if (!clock) throw new Error(`${label} require a replay clock`);
    let resolve;
    let reject;
    const pending = new Promise((accept, fail) => { resolve = accept; reject = fail; });
    const attempts = new Map();
    this.pollingFence = {
      reject,
      matched(key, record) {
        const target = targets.get(key);
        if (!target) return;
        if (record === target) {
          targets.delete(key);
          if (!targets.size) resolve();
        } else {
          const count = (attempts.get(key) ?? 0) + 1;
          attempts.set(key, count);
          if (count >= 2) reject(new Error(`${label} were superseded before a natural poll selected them`));
        }
      },
    };
    try {
      await clock.wait(pending, label, this.timeoutMs, {
        maxSteps: 4096 * (targets.size + 1), maxIdleSteps: 64, progress: () => targets.size,
      });
    } finally {
      this.pollingFence = undefined;
    }
  }

  connect(url, onOpen, onMessage, onClose, metadata = {}) {
    const rawUrl = url;
    url = this.canonicalize(url);
    const connection = this.connectionCounts.get(url) ?? 0;
    this.connectionCounts.set(url, connection + 1);
    const socket = { url, rawUrl, channel: metadata.channel, context: metadata.context, connection, onOpen, onData: onMessage, onMessage: (text) => {
      try { return deliverSocketFrame(socket, text, this.audit, this.venue.validateSocketFrame); }
      catch (error) { this.invalidSignatures++; throw error; }
    }, onClose, closed: false, opened: false, topics: new Set(), publicTopics: new Set(), subscriptionArgs: new Map(), subscriptions: new Set(), generation: connection + 1, ordinal: connection, loginVerified: false };
    if (this.venue.socketRequest || this.venue.socketResponse || this.venue.bootstrapPrivateFromTape) {
      Object.assign(socket, { requests: new Map(), requestIds: new Set(), requestCounts: new Map(), mappedRequests: new Map(), responseIds: new Set() });
      const deliver = socket.onMessage;
      socket.onMessage = (text) => deliver(this.correlateSocketResponse(socket, text));
    }
    this.audit?.connected?.(rawUrl, socket);
    this.sockets.push(socket);
    this.byConnection.set(`${url}:${connection}`, socket);
    // A recorded open may already be waiting for this socket to be constructed.
    for (const waiter of [...this.waiters]) waiter();
    // Old synthetic fixtures have no lifecycle records; live tapes drive every
    // open explicitly, including failed startup attempts and reconnects.
    const lifecycle = this.socketLifecycle.get(url);
    const hasLifecycle = lifecycle?.hasOpen;
    const explicit = this.explicitOpens || this.records.some((r) => r.type === 'connect' && this.canonicalize(r.url) === url);
    if (!hasLifecycle || (!explicit && lifecycle.initial.has(connection))) this.deferCallback(() => this.openSocket(socket));
    return socket;
  }

  deferCallback(callback) {
    // A queued synchronous host callback is runnable work, not a reason to
    // advance a protocol timer. Its HTTP descendants own their own lifetimes.
    const release = this.replayClock?.startup ? this.replayClock.hold() : undefined;
    setImmediate(() => { try { callback(); } finally { release?.(); } });
  }

  openSocket(socket) {
    if (socket.closed || socket.opened) return;
    socket.opened = true;
    this.open.set(`${socket.url}:${socket.connection}`, socket);
    socket.onOpen();
    if (!socket.closed && !this.playing && this.venue.startupMessage) {
      const frame = this.frames.find((record) => record.type !== 'open' && record.type !== 'command' && record.type !== 'wait'
        && !this.startupDelivered.has(record) && record.url === socket.url
        && (record.connection === undefined || record.connection === socket.connection));
      if (frame?.type === 'ws' && this.venue.startupMessage(frame)) {
        this.startupDelivered.add(frame);
        socket.onMessage(frame.text);
      }
    }
    for (const waiter of [...this.waiters]) waiter();
  }

  close(socket, code = 1000, local = false) {
    if (socket.closed) return;
    socket.closed = true;
    this.audit?.closed?.(socket.rawUrl ?? socket.url, socket);
    if (!local) socket.onClose?.(code);
  }

  send(socket, text) {
    if (this.venue.bootstrapPrivateFromTape) {
      const message = JSON.parse(text);
      if (message.op && typeof message.req_id === 'string' && message.req_id) socket.requests.set(message.op, message.req_id);
    }
    try { this.venue.validateSocketMessage?.(socket, text); }
    catch (error) { this.invalidSignatures++; throw error; }
    let parsed;
    try { parsed = JSON.parse(text); } catch { /* Literal heartbeat. */ }
    if (parsed && this.isPrivate && this.venue.verifySubscribe) {
      try { if (!this.venue.verifySubscribe(parsed)) throw new Error('invalid replay websocket signature'); }
      catch (error) { this.invalidSignatures++; throw error; }
    }
    if (parsed) {
      const request = this.venue.socketRequest?.(parsed);
      if (request) {
        if (socket.requestIds.has(request.id)) throw new Error(`duplicate websocket request ${request.key}`);
        if (this.recordedSocketRequests.size || this.venue.requireSocketRequestRecords) {
          const queueKey = `${socket.url} ${socket.ordinal} ${request.key}`;
          const count = socket.requestCounts.get(request.key) ?? 0;
          const recordedId = this.recordedSocketQueues.get(queueKey)?.[count];
          if (recordedId === undefined) throw new Error(`unrecorded websocket request ${request.key}`);
          socket.requestCounts.set(request.key, count + 1);
          socket.mappedRequests.set(recordedId, request.id);
        }
        socket.requests.set(request.id, request.key);
        socket.requestIds.add(request.id);
        this.sentRequests.add(JSON.stringify(stable({ url: socket.url, request: request.key })));
      }
      if (subscriptionMessage(parsed)) {
        const subscription = subscriptionMessage(parsed);
        for (const topic of subscription?.topics ?? []) {
          if (typeof topic !== 'string' || !topic) {
            if (this.venue.socketResponse) throw new Error('invalid websocket subscription topic');
            continue;
          }
          if (subscription.op === 'subscribe') socket.subscriptions.add(topic);
          else socket.subscriptions.delete(topic);
        }
      }
    }
    for (const key of subscriptionKeys(socket.rawUrl ?? socket.url, text, this.callerIds, { ...this.venue, fullSocketIdentity: true })) {
      this.fullSubscriptions.add(this.venue.subscriptionGenerations ? `${socket.generation} ${key}` : key);
      if (this.startupSubscriptions.get(key) === false) { this.startupSubscriptions.set(key, true); this.pendingStartupSubscriptions--; }
    }
    this.sent(socket.url, text);
    for (const key of subscriptionKeys(socket.rawUrl ?? socket.url, text)) {
      this.protocolSubscriptions.add(`${socket.generation} ${key}`);
      if (this.startupSubscriptions.get(key) === false) {
        this.startupSubscriptions.set(key, true);
        this.pendingStartupSubscriptions--;
      }
    }
    this.venue.onSocketSend?.(socket, text);
    this.audit?.sent(socket.rawUrl ?? socket.url, text, socket);
    if (typeof parsed?.params?.channel === 'string') {
      if (parsed.method === 'subscribe') socket.subscriptions.add(parsed.params.channel);
      if (parsed.method === 'unsubscribe') socket.subscriptions.delete(parsed.params.channel);
    }
    for (const reply of this.venue.socketReplies?.(socket.rawUrl ?? socket.url, parsed) ?? []) {
      this.deferCallback(() => { if (!socket.closed) socket.onMessage(reply); });
    }
    for (const waiter of [...this.waiters]) waiter();
    const pingReply = this.venue.pingReply?.(text);
    if (pingReply) this.deferCallback(() => { if (!socket.closed) socket.onMessage(pingReply); });
    if (this.venue.respondToJsonPing && parsed?.type === 'ping') {
      this.deferCallback(() => {
        if (!socket.closed) socket.onMessage(JSON.stringify({ type: 'pong', id: parsed.id }));
      });
      return;
    }
    if (this.venue.respondToPing && text === 'ping') {
      this.deferCallback(() => {
        if (!socket.closed) socket.onMessage('pong');
      });
      return;
    }
    try {
      const message = JSON.parse(text);
      for (const topic of Array.isArray(message.params) ? message.params : []) {
        if (message.method === 'SUBSCRIBE') {
          socket.subscriptions.add(topic);
          if (
            !this.venue.marketFilter ||
            topic.startsWith('account.') ||
            this.venue.marketFilter({ symbol: topic.split('.').at(-1) })
          )
            { this.nativeSubscriptions.add(`${message.method} ${topic}`); if (this.subscriptions instanceof Set && this.subscriptions !== this.protocolSubscriptions) this.subscriptions.add(`${message.method} ${topic}`); }
        }
        if (message.method === 'UNSUBSCRIBE') socket.subscriptions.delete(topic);
      }
      for (const waiter of [...this.waiters]) waiter();
      // OKX's opt-in auth validation also uses its v5 subscription argument model.
      const validateTopics = this.bitget || (this.venue.validateSubscriptions ?? this.venue.validateLogin);
      if (validateTopics && ['subscribe', 'unsubscribe'].includes(message.op)) {
        if (!Array.isArray(message.args)) throw new Error(`Invalid subscription on ${socket.url}`);
        for (const topic of message.args) {
          if (!topic || typeof (topic.channel ?? topic.topic) !== 'string' || !(topic.channel ?? topic.topic))
            throw new Error(`Invalid subscription topic on ${socket.url}`);
          const key = JSON.stringify(Object.fromEntries(Object.entries(topic).sort(([a], [b]) => a.localeCompare(b))));
          if (message.op === 'subscribe') { socket.subscriptionArgs.set(key, topic); socket.topics.add(this.bitget ? bitgetTopic(topic) : key); }
          else { socket.subscriptionArgs.delete(key); socket.topics.delete(this.bitget ? bitgetTopic(topic) : key); }
        }
        return;
      }
      if (
        !this.venue.validateLogin ||
        !/\/(private|business)$/.test(new URL(socket.url).pathname) ||
        message.op !== 'login'
      )
        return;
      const arg = message.args?.length === 1 ? message.args[0] : undefined;
      const timestamp = String(arg?.timestamp ?? '');
      if (
        arg?.apiKey !== (this.venue.replayCredentials?.key ?? 'replay-key') ||
        arg?.passphrase !== (this.venue.replayCredentials?.passphrase ?? 'replay-passphrase') ||
        !/^\d+$/.test(timestamp) ||
        !Number.isSafeInteger(Number(timestamp)) ||
        Math.abs(Number(timestamp) - Date.now() / 1000) > 30
      ) {
        throw new Error(`Invalid OKX login credentials/timestamp on ${socket.url}`);
      }
      const expected = createHmac('sha256', this.venue.replayCredentials?.secret ?? 'replay-secret')
        .update(`${timestamp}GET/users/self/verify`)
        .digest('base64');
      const supplied = Buffer.from(String(arg.sign ?? ''));
      const expectedBytes = Buffer.from(expected);
      if (supplied.length !== expectedBytes.length || !timingSafeEqual(supplied, expectedBytes)) {
        throw new Error(`Invalid OKX login signature on ${socket.url}`);
      }
      socket.loginVerified = true;
    } catch (error) {
      if (!(error instanceof SyntaxError)) throw error;
    }
  }

  assertSubscribed(socket, message) {
    if (
      !(this.venue.validateSubscriptions ?? this.venue.validateLogin) ||
      !message.arg?.channel ||
      !Array.isArray(message.data)
    )
      return;
    const topics = [...socket.subscriptionArgs.values()].filter((topic) => topic.channel === message.arg.channel);
    // Some documented messages put instId only on each data row, not arg.
    const rows = message.data.length ? message.data : [null];
    for (const row of rows) {
      const object = row && !Array.isArray(row) && typeof row === 'object' ? row : {};
      const covered = topics.some((topic) =>
        Object.entries(topic).every(([key, value]) => {
          if (key === 'channel') return true;
          if (key === 'instType' && value === 'ANY') return true;
          const values = [message.arg[key], object[key]].filter((v) => v !== undefined && v !== '');
          if (!values.length && key === 'instType') {
            const id = object.instId ?? message.arg.instId;
            if (id?.endsWith('-SWAP')) values.push('SWAP');
          }
          return values.length > 0 && values.every((v) => v === value);
        }),
      );
      if (!covered) {
        throw new Error(
          `Unsubscribed ${message.arg.channel} frame on ${socket.url}: ${JSON.stringify(message.arg)}${object.instId ? ` instId=${object.instId}` : ''}`,
        );
      }
    }
  }

  assertWritesConsumed() {
    if (this.invalidSignatures) throw new Error(`${this.invalidSignatures} invalid replay signature(s)`);
    if (this.missingWrites) throw new Error(`${this.missingWrites} taped write(s) were not sent`);
  }

  correlateSocketResponse(socket, text) {
    if (!this.venue.socketResponse) return text;
    let message;
    try {
      message = JSON.parse(text);
    } catch {
      return text;
    }
    const response = this.venue.socketResponse(message);
    if (!response) return text;
    if (socket.responseIds.has(response.id)) throw new Error('duplicate websocket response');
    const scope = `${socket.url} ${socket.ordinal}`;
    const recordedKey = `${scope} ${response.id}`;
    const recorded = this.recordedSocketRequests.get(recordedKey);
    if ((this.recordedSocketRequests.size || this.venue.requireSocketRequestRecords) && recorded === undefined)
      throw new Error('websocket response has no taped request');
    if (recorded !== undefined && response.key !== undefined && recorded !== response.key)
      throw new Error('websocket response symbol differs from taped request');
    const key = recorded ?? response.key;
    const id = this.recordedSocketRequests.size
      ? socket.mappedRequests.get(response.id)
      : [...socket.requests].find(([, requestKey]) => requestKey === key)?.[0];
    if (id === undefined) throw new Error(`unsolicited websocket response ${key ?? response.id}`);
    if (socket.requests.get(id) !== key) throw new Error('websocket response differs from pending request');
    socket.requests.delete(id);
    socket.mappedRequests.delete(response.id);
    socket.responseIds.add(response.id);
    return JSON.stringify({ ...message, id });
  }

  async socketFor(frame, textOrOrdinal, ordinal) {
    if (typeof frame === 'string') frame = { url: this.canonicalize(frame),
      text: typeof textOrOrdinal === 'string' ? textOrOrdinal : undefined,
      connection: typeof textOrOrdinal === 'number' ? textOrOrdinal : ordinal };
    else if (frame.connection === undefined && frame.socket !== undefined) frame = { ...frame, connection: frame.socket };
    const data = frame.data ?? frame.text;
    const stream = data && this.venue.streamOf?.(data);
    const lifecycleKey = frame.connection === undefined ? undefined : `${frame.url} ${frame.connection}`;
    const streams = this.venue.subscriptionRouting && lifecycleKey && this.lifecycleStreams.get(lifecycleKey);
    const subscribed = (socket, target) => [...socket.subscriptions].some((topic) => target === topic || target.startsWith(`${topic}.`));
    const find = () => {
      if (this.venue.subscriptionRouting && !stream && lifecycleKey) {
        const bound = this.lifecycleSockets.get(lifecycleKey);
        if (bound && (!bound.closed || frame.type === 'close')) return bound;
      }
      const candidates = this.sockets.filter((socket) => socket.url === frame.url &&
        (!socket.closed || frame.type === 'close') && (socket.opened || ['open', 'close'].includes(frame.type)) &&
        (this.venue.subscriptionRouting && (stream || streams?.size) || frame.connection === undefined || socket.connection === frame.connection));
      if (this.venue.subscriptionRouting && (stream || streams?.size)) {
        const matched = candidates.filter((socket) => stream ? subscribed(socket, stream) : [...streams].some((target) => subscribed(socket, target)));
        if (matched.length > 1) throw new Error(`ambiguous subscribed tape socket ${frame.url} ${stream ?? 'lifecycle'}`);
        const socket = matched[0];
        if (socket && lifecycleKey) this.lifecycleSockets.set(lifecycleKey, socket);
        return socket;
      }
      if (stream) return candidates.find((socket) => socket.opened && [...socket.subscriptions].some((topic) => stream === topic || stream.startsWith(`${topic}.`)));
      if ((this.bitget || this.hyperliquid) && !frame.text && frame.connection === undefined)
        return (frame.type === 'open' && candidates.find((socket) => !socket.opened)) || candidates[0];
      if (this.venue.socketMatches && frame.text !== undefined && frame.connection === undefined) {
        const matched = candidates.filter((socket) => this.venue.socketMatches(socket, frame.text));
        if (matched.length > 1) throw new Error(`ambiguous shared endpoint frame ${frame.url}`);
        return matched[0];
      }
      if (this.venue.socketMatches && frame.type === 'close' && frame.connection === undefined && candidates.length > 1) throw new Error('Ambiguous close on a shared public/private endpoint');
      if (frame.connection !== undefined || candidates.length === 1) return candidates[0];
      if (this.legacyLatest) return candidates.at(-1);
      if (candidates.length > 1 && frame.topic) {
        const matched = candidates.filter((socket) => socket.topics.has(frame.topic));
        if (matched.length === 1) return matched[0];
        throw new Error(`ambiguous legacy tape socket ${frame.url} ${frame.topic}; record connection IDs`);
      }
    };
    const current = find();
    if (current) return current;
    let waiter;
    try {
      const pending = new Promise((resolve) => {
        waiter = () => { const socket = find(); if (socket) resolve(socket); };
        this.waiters.add(waiter);
        waiter();
      });
      const label = `socket did not open: ${frame.url} ${stream ?? 'open'}`;
      return await (this.replayClock?.active && !this.replayClock.startup ? this.replayClock.wait(pending, label, this.timeoutMs) : withTimeout(pending, label, this.timeoutMs));
    } finally { this.waiters.delete(waiter); }
  }

  async lifecycle(frame) {
    const socket = await this.socketFor(frame);
    if (frame.type === 'open') this.openSocket(socket);
    else {
      if (frame.local && !socket.closed) throw new Error(`recorded local close was not requested: ${frame.url}:${frame.connection}`);
      if (!frame.local && socket.closed) throw new Error('socket closed before recorded remote close');
      socket.closed = true;
      this.audit?.closed?.(socket.rawUrl ?? socket.url, socket);
      if (!frame.local) socket.onClose?.(frame.code ?? 1006);
    }
  }

  async deliver(frame, prepared = false) {
    if (typeof prepared === 'string') return frame.onMessage(prepared);
    if (!prepared) frame = { ...frame, url: this.canonicalize(frame.url), connection: frame.connection ?? frame.socket };
    if ((this.bitget || this.hyperliquid) && frame.connection === undefined && frame.type !== 'open') {
      let arg;
      try { arg = JSON.parse(frame.text)?.arg; } catch { /* Literal heartbeat. */ }
      const key = this.bitget && arg && bitgetTopic(arg);
      await this.socketFor({ ...frame, text: undefined });
      const matching = this.sockets.filter((socket) => socket.url === frame.url && socket.opened && !socket.closed && (!key || socket.topics.has(key)));
      if (key && !matching.length) throw new Error(`frame has no subscribed socket: ${frame.url} ${key}`);
      for (const socket of matching) await this.deliver({ ...frame, connection: socket.connection }, true);
      return;
    }
    const socket = await this.socketFor(frame);
    if (this.bitget && frame.text) {
      let arg;
      try { arg = JSON.parse(frame.text)?.arg; } catch { /* Literal heartbeat. */ }
      if (arg && !socket.topics.has(bitgetTopic(arg))) throw new Error(`frame has no subscribed socket: ${frame.url} ${bitgetTopic(arg)}`);
    }
    if (frame.type === 'open' || frame.type === 'close') await this.lifecycle(frame);
    else {
      if (!socket.opened) throw new Error('frame before recorded socket open');
      if (this.venue.rust === 'bitunix' && frame.topic && !socket.url.includes(this.venue.privateSocket)) {
        const deadline = performance.now() + 10_000;
        while (!socket.topics.has(frame.topic) && !socket.closed && performance.now() < deadline)
          await new Promise((r) => realTimeout(r, 5));
      }
      if ((this.venue.validateLogin || this.venue.validateSubscriptions) && frame.text.trimStart().startsWith('{')) {
        const message = JSON.parse(frame.text);
        if (this.venue.validateLogin && message.event === 'login' && String(message.code) === '0' && !socket.loginVerified)
          throw new Error(`Taped login success preceded a valid login on ${frame.url}`);
        this.assertSubscribed(socket, message, frame.subscriptionKey);
      }
      this.validateTopic?.(socket, frame);
      let text = frame.data ?? frame.text;
      if (this.venue.bootstrapPrivateFromTape) {
        const message = JSON.parse(text);
        if (message.req_id === '@latest') {
          const deadline = performance.now() + this.timeoutMs;
          while (!socket.requests.has(message.op)) {
            if (performance.now() >= deadline) throw new Error(`replay timeout: ${message.op} request for acknowledgement`);
            await new Promise((resolve) => realTimeout(resolve, 5));
          }
          text = JSON.stringify({ ...message, req_id: socket.requests.get(message.op) });
        }
      }
      socket.onMessage(text);
    }
  }

  /**
   * Deliver every frame in order, yielding to the event loop between frames.
   * Commands are awaited where they fall. A private tape is paced: each record
   * waits until its time after the account is ready.
   */
  async play(onTick, ready = async () => {}, runCommand = async () => {}, clock, begin = () => {}) {
    this.replayClock = clock;
    if (clock) clock.startupTimeoutMs = this.timeoutMs;
    const logicalStartup = Boolean(clock?.startup);
    if (logicalStartup) clock.startupDeadline = performance.now() + this.timeoutMs;
    let initialized;
    let startupReady = false;
    let fallbackReady, fallbackDone = false, fallbackError;
    let startupCeiling = Infinity;
    let expectedStartupTopics, expectedBitunixTopics;
    const complete = () => {
      if (this.readiness) {
        if (!this.readiness(!this.venue.bootstrapPrivateFromTape)) return false;
      } else {
        if (fallbackError) throw fallbackError;
        if (!fallbackDone) return false;
      }
      if ((this.audit && !this.audit.ready(!this.venue.bootstrapPrivateFromTape)) || this.pendingStartupSubscriptions) return false;
      if (this.venue.startupTopics) {
        expectedStartupTopics ??= this.venue.startupTopics(this.records, this.books).map((arg) => JSON.stringify(stable(arg)));
        if (!expectedStartupTopics.every((topic) => this.sockets.some((socket) => socket.opened && !socket.closed && socket.topics.has(topic)))) return false;
      }
      if (this.venue.rust === 'bitunix') {
        expectedBitunixTopics ??= 2 * JSON.parse(this.records.find((r) => r.type === 'http' && r.url.includes('/market/trading_pairs')).body).data
          .filter((m) => apiSupported(m.isApiSupported) && m.symbolStatus === 'OPEN' && m.quote === 'USDT').length;
        const active = this.sockets.filter((s) => s.opened && !s.closed).reduce((sum, s) => sum + s.publicTopics.size, 0);
        if (active < expectedBitunixTopics) return false;
      }
      return true;
    };
    if (logicalStartup) clock.observeStartup = () => {
      if (this.prepareReadiness?.()) return true;
      if (!this.readiness && !fallbackReady) {
        fallbackReady = Promise.resolve().then(() => ready(!this.venue.bootstrapPrivateFromTape))
          .then(() => { fallbackDone = true; }, (error) => { fallbackError = error; });
        return true;
      }
      return false;
    };
    const ensureReady = () => initialized ??= withTimeout((async () => {
      if (logicalStartup) {
        await clock.until(complete, 'Logical replay startup', this.timeoutMs,
          startupCeiling);
        return;
      }
      if (this.venue.bootstrapFrame && !this.liveSmoke) for (const frame of this.frames.filter((record) => this.bootstrapFrame(record))) {
        if (!['open', 'close', 'ws'].includes(frame.type)) throw new Error('Invalid bootstrap record');
        await this.deliver(frame, true);
        this.startupDelivered.add(frame);
        await new Promise(setImmediate);
      }
      await ready(!this.venue.bootstrapPrivateFromTape);
      while (this.audit && !this.audit.ready(!this.venue.bootstrapPrivateFromTape)) await new Promise((r) => realTimeout(r, 5));
      while (this.pendingStartupSubscriptions) await new Promise((resolve) => realTimeout(resolve, 1));
      if (this.venue.startupTopics) {
        const expected = this.venue.startupTopics(this.records, this.books).map((arg) => JSON.stringify(stable(arg)));
        const complete = () => expected.every((topic) => this.sockets.some((socket) => socket.opened && !socket.closed && socket.topics.has(topic)));
        while (!complete()) await new Promise((r) => realTimeout(r, 10));
      }
      if (this.venue.rust !== 'bitunix') return;
      const symbols = JSON.parse(this.records.find((r) => r.type === 'http' && r.url.includes('/market/trading_pairs')).body).data
        .filter((m) => apiSupported(m.isApiSupported) && m.symbolStatus === 'OPEN' && m.quote === 'USDT').length;
      const active = () => this.sockets.filter((s) => s.opened && !s.closed).reduce((sum, s) => sum + s.publicTopics.size, 0);
      const deadline = performance.now() + 10_000;
      while (active() < symbols * 2) {
        if (performance.now() > deadline) throw new Error(`active startup topics missing: expected ${symbols * 2}, got ${active()}`);
        await new Promise((r) => realTimeout(r, 5));
      }
    })(), 'Replay startup', this.timeoutMs).then(() => { startupReady = true; });
    const recordedLifecycle = this.records.some((r) => r.type === 'connect');
    const inlineStartup = logicalStartup || this.venue.rust === 'bitunix' || recordedLifecycle || this.liveSmoke;
    if (!inlineStartup) await ensureReady();
    else if (!logicalStartup) {
      // Explicit opens can be prerequisites of readiness. Keep startup timers
      // live while consuming that prelude, then move both venues to tape time.
      ensureReady().catch(() => {}); // The same promise is awaited below.
    }
    begin();
    this.playing = true;
    this.replayClock = clock;
    if (!inlineStartup) clock?.start();
    this.peakBytes = Math.max(this.peakBytes ?? 0, this.sampleMemory?.() ?? 0);
    const start = performance.now();
    let delivered = 0;
    const warm = Math.floor(this.frames.length / 4);
    for (const frame of this.frames) {
      startupCeiling = frame.t;
      if (delivered === warm) this.warmMark = { thread: process.threadCpuUsage(), wall: performance.now() };
      if (this.isPrivate && !this.venue.polling) {
        const wait = frame.t - (performance.now() - start);
        if (wait > 0) await new Promise((resolve) => realTimeout(resolve, wait));
      }
      this.time = frame.t;
      if (clock && !clock.active && startupReady) clock.start(frame.t);
      await clock?.advance(frame.t, this.synchronousTicks === true);
      if (logicalStartup && clock.startup) clock.invalidateStartup();
      if (frame.type === 'http') {
        // Polling read anchors advance time; only real requests select replies.
      } else if (frame.type === 'wait') {
        this.venue.validateWait?.(frame, this);
      } else if (frame.type === 'command') {
        await ensureReady();
        if (logicalStartup && clock.startup) clock.start();
        if (this.venue.bootstrapPrivateFromTape) {
          const account = ready(true);
          await (clock?.active ? clock.wait(account, 'account readiness', this.timeoutMs) : withTimeout(account, 'account readiness', this.timeoutMs));
        }
        this.results.push(await this.executeCommand(frame, runCommand).then((ids) => ({ ok: ids }), (error) => {
          if (String(error?.message).startsWith('replay timeout:')) throw error;
          return { error: String(error?.message ?? error) };
        }));
      } else if (!this.startupDelivered.has(frame)) {
        if (logicalStartup && clock.startup) {
          let delivered = false, error;
          this.deliver(frame, true).then(() => { delivered = true; }, (failure) => { error = failure; delivered = true; });
          await clock.until(() => delivered, 'Startup frame delivery', this.timeoutMs, frame.t);
          if (error) throw error;
        } else await this.deliver(frame, true);
      }
      if (logicalStartup && clock.startup) {
        if (!clock.startupSettled) await clock.until(() => true, 'Startup record settlement', this.timeoutMs, frame.t);
        if (complete()) {
          startupReady = true;
          initialized = Promise.resolve();
          clock.start(frame.t);
        }
      }
      delivered++;
      if (delivered % 64 === 0) this.peakBytes = Math.max(this.peakBytes, this.sampleMemory?.() ?? 0);
      onTick?.(frame.t);
      if (logicalStartup && clock.startup && !this.synchronousTicks) clock.invalidateStartup();
      await new Promise(setImmediate);
    }
    startupCeiling = Infinity;
    await ensureReady();
    if (logicalStartup && clock.startup) clock.start();
    if (clock && !clock.active) clock.start(this.time);
    await this.finishPolling(clock);
    await this.finishResponses();
    return delivered;
  }

  results = [];
  sentRequests = new Set();
  sent(url, text) {
    if (typeof url !== 'string') return this.send(url, text);
    if (!this.venue.requestBodies || text === 'ping' || text === 'pong') return;
    let message;
    try {
      message = JSON.parse(text);
    } catch {
      return;
    }
    if (message.op === 'subscribe' || message.op === 'unsubscribe') {
      for (const topic of message.args ?? [])
        this.sentRequests.add(JSON.stringify(stable({ url, op: message.op, topic })));
    } else if (message.op === 'login') {
      const args = message.args.map(({ timestamp, nonce, sign, ...credentials }) => credentials);
      this.sentRequests.add(JSON.stringify(stable({ url, op: 'login', args })));
    }
  }
}

// Factory compatibility uses the production transport, with venue defaults.
export const createTape = (records, venue = {}, isPrivate = venue.isPrivate ?? false, {
  impl, books = [], socketTimeoutMs = 30_000, commandTimeoutMs = 15_000, credentials = venue.replayCredentials, options,
} = {}) => {
  const defaults = venue.rust === 'mexc' ? { canonicalIntegerStrings: true, ignoredBodyFields: ['externalOid', 'requestId'], rollingListWindows: true, matchRequestBody: true }
    : venue.rust === 'cryptocom' ? { requestBodyParams: true }
    : ['bingx', 'blofin'].includes(venue.rust) ? { requestBodies: true } : {};
  const tape = new Tape(records, { ...defaults, ...venue, replayCredentials: credentials, replayOptions: options ?? venue.replayOptions, commandTimeoutMs }, isPrivate, socketTimeoutMs, impl, books);
  tape.subscriptions = tape.protocolSubscriptions;
  tape.normalizedSubscriptions = tape.nativeSubscriptions;
  return tape;
};
