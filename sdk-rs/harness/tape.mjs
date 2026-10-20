import { createHmac, timingSafeEqual } from 'node:crypto';
import { parseRequestJson } from './request-json.mjs';
import { Responses } from './responses.mjs';
import { isRead, subscriptionKeys, subscriptionMessage, requestKey as typedRequestKey } from './requests.mjs';
import { setTimeout as realTimeout, clearTimeout as realClearTimeout } from 'node:timers';
import { LOADED_FLAG } from '../hosts/wasm/js/wire.mjs';

export const replayStoreReady = (loaded, isPrivate) => {
  const publicMask = LOADED_FLAG.MARKETS | LOADED_FLAG.TICKERS;
  const required = isPrivate
    ? publicMask | LOADED_FLAG.BALANCE | LOADED_FLAG.ORDERS | LOADED_FLAG.POSITIONS
    : publicMask;
  return (loaded & required) === required;
};
/** Per-request values: clocks, signatures and generated client ids. */
const VOLATILE = new Set(['timestamp', 'recvWindow', 'signature', 'newClientOrderId', 'clientAlgoId', 'requestId']);
export const canonicalUrl = (url, callerIds = new Set(), venue = {}) => {
  const parsed = new URL(url);
  // Active trigger lists use a rolling lookback ending at request time.
  // Keep its width exact while allowing the two implementations' clock skew.
  if (
    venue.rollingListWindows &&
    /\/private\/(planorder|stoporder)\/list\/orders$/.test(parsed.pathname) &&
    parsed.searchParams.has('start_time') &&
    parsed.searchParams.has('end_time')
  ) {
    const end = Number(parsed.searchParams.get('end_time'));
    parsed.searchParams.set('start_time', String(Number(parsed.searchParams.get('start_time')) - end));
    parsed.searchParams.set('end_time', 'now');
  }

  const query = [...parsed.searchParams]
    .filter(([name, value]) => !VOLATILE.has(name) || callerIds.has(value))
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

export const canonicalBody = (body, callerIds = new Set(), venue = {}) => {
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
                  VOLATILE.has(k) &&
                  !callerIds.has(String(v))) ||
                (k === 'clientId' && !callerIds.has(String(v))) ||
                (venue.ignoredBodyFields?.includes(k) && !callerIds.has(String(v)))
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
    if (!this.timers.has(timer)) return;
    if (timer.repeat) {
      timer.at = Date.now() + timer.delay;
      if (!this.active) timer.handle = this.original.setTimeout(() => this.fire(timer), timer.delay);
    } else this.timers.delete(timer);
    timer.fn(...timer.args);
  }
  start() {
    const wall = this.original.now();
    for (const timer of this.timers.values()) {
      this.original.clearTimeout(timer.handle);
      timer.at = this.epoch + Math.max(0, timer.at - wall);
    }
    this.active = true;
    this.time = this.epoch;
  }
  async advance(t) {
    const target = Math.max(this.time, this.epoch + t);
    for (;;) {
      let next;
      for (const timer of this.timers.values()) if (timer.at <= target && (!next || timer.at < next.at)) next = timer;
      if (!next) break;
      this.time = next.at;
      this.fire(next);
      await new Promise(setImmediate);
    }
    this.time = target;
  }
  async wait(promise, label, timeoutMs) {
    let done = false;
    const result = Promise.resolve(promise).finally(() => {
      done = true;
    });
    const pump = (async () => {
      while (!done) {
        await new Promise(setImmediate);
        if (done) break;
        let next;
        for (const timer of this.timers.values()) if (!next || timer.at < next.at) next = timer;
        if (next) {
          this.time = Math.max(this.time, next.at);
          this.fire(next);
        }
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
    for (const timer of this.timers.values()) this.original.clearTimeout(timer.handle);
    this.timers.clear();
    Date.now = this.original.now;
    for (const name of ['setTimeout', 'clearTimeout', 'setInterval', 'clearInterval'])
      globalThis[name] = this.original[name];
  }
}

/** Serves recorded HTTP responses and websocket frames in tape order. */
export class Tape {
  constructor(records, venue = {}, isPrivate = venue.isPrivate ?? false, timeoutMs = 30000, impl, books = []) {
    this.venue = venue;
    this.impl = impl;
    this.isPrivate = isPrivate;
    this.timeoutMs = timeoutMs;
    this.commandIndex = 0;
    this.time = 0;
    this.generations = new Map();
    this.recordedSocketRequests = new Map();
    this.recordedSocketQueues = new Map();
    for (const record of records.filter((r) => r.type === 'ws-send')) {
      const request = venue.socketRequest?.(JSON.parse(record.text));
      if (!request) continue;
      const key = `${canonicalUrl(record.url)} ${record.socket ?? 0} ${request.id}`;
      if (this.recordedSocketRequests.has(key)) throw new Error(`duplicate taped websocket request ${key}`);
      this.recordedSocketRequests.set(key, request.key);
      const queueKey = `${canonicalUrl(record.url)} ${record.socket ?? 0} ${request.key}`;
      if (!this.recordedSocketQueues.has(queueKey)) this.recordedSocketQueues.set(queueKey, []);
      this.recordedSocketQueues.get(queueKey).push(request.id);
    }
    this.startupSubscriptions = new Map(
      Array.from(venue.startupSubscriptions?.(records, books) ?? [], (key) => [key, false]),
    );
    this.pendingStartupSubscriptions = this.startupSubscriptions.size;
    this.audit = venue.createSocketAudit?.(impl, { isPrivate, books, tickers: venue.publicTickers?.(records) });
    this.callerIds = new Set();
    const collectCallerIds = (value) => {
      if (!value || typeof value !== 'object') return;
      if (typeof value.clientOrderId === 'string') this.callerIds.add(value.clientOrderId);
      for (const nested of Object.values(value)) collectCallerIds(nested);
    };
    for (const record of records) if (record.type === 'command') collectCallerIds(record);
    this.responses = new Map();
    this.writeOrder = [];
    for (const record of records.filter((r) => r.type === 'http')) {
      // Per-fixture KNOWN_TS_FIXES exception; never a fallback for Rust.
      const expectedBody = impl === 'ts' ? (record.tsRequestBody ?? record.requestBody) : record.requestBody;
      const key = `${record.method} ${canonicalUrl(record.url, this.callerIds, this.venue)}${expectedBody === undefined ? '' : ` BODY ${canonicalBody(expectedBody, this.callerIds, this.venue)}`}`;
      if (!this.responses.has(key)) this.responses.set(key, []);
      this.responses.get(key).push(record);
      if (this.isWrite(record.method, record.url)) this.writeOrder.push(record);
    }
    if (venue.checkRequest || venue.requestKey || venue.requestBodies) {
      const responseKey = (method, url, body) => {
        if (venue.requestKey) return venue.requestKey(method, url, body, this.callerIds);
        if (venue.requestBodies) return typedRequestKey(method, url, body, this.callerIds, venue);
        const base = `${method} ${canonicalUrl(url, this.callerIds, this.venue)}`;
        const exact = `${base} BODY ${canonicalBody(body, this.callerIds, venue)}`;
        return this.responses.has(exact) ? exact : base;
      };
      this.responseStore = new Responses(
        records.map((record) =>
          impl === 'ts' && record.tsRequestBody !== undefined
            ? { ...record, requestBody: record.tsRequestBody }
            : record,
        ),
        responseKey,
        venue.checkRequest,
        this.isWrite.bind(this),
      );
    }
    this.frames = records.filter(
      (r) => r.type === 'ws' || r.type === 'command' || r.type === 'close' || r.type === 'open',
    );
    this.open = new Map();
    this.waiters = new Set();
    this.unserved = 0;
  }

  invalidSignatures = 0;
  subscriptions = new Set();
  protocolSubscriptions = new Set();
  isWrite(method, url) {
    return this.venue.isWrite?.(method, url) ?? !isRead(method, url);
  }
  get missingWrites() {
    if (this.responseStore) return this.responseStore.unusedWrites();
    return [...this.responses.values()].flat().filter((record) => this.isWrite(record.method, record.url)).length;
  }
  respond(method, url, body, headers) {
    this.venue.validateRequest?.({ method, url, body, headers });
    if (this.isPrivate && this.venue.verifyRequest && !this.venue.verifyRequest({ method, url, body, headers })) {
      this.invalidSignatures++;
      throw new Error(`invalid replay signature: ${method} ${url}`);
    }
    const now = this.replayClock?.active ? this.replayClock.time - this.replayClock.epoch : this.time;
    if (this.responseStore) {
      const write = this.isWrite(method, url);
      if (write && this.venue.compareWriteOrder) {
        const next = this.writeOrder[0];
        const expectedBody = this.impl === 'ts' ? (next?.tsRequestBody ?? next?.requestBody) : next?.requestBody;
        if (
          !next ||
          this.responseStore.key(method, url, body) !== this.responseStore.key(next.method, next.url, expectedBody)
        ) {
          if (this.unserved < 10) console.error('out-of-order write', { actual: this.responseStore.key(method, url, body), expected: next && this.responseStore.key(next.method, next.url, expectedBody) });
          this.unserved = ++this.responseStore.unserved;
          return { status: 599, body: '' };
        }
      }
      const before = write ? this.responseStore.unusedWrites() : 0;
      const response = this.responseStore.respond(method, url, body, this.commandIndex, now);
      if (write && this.responseStore.unusedWrites() < before) this.writeOrder.shift();
      this.unserved = this.responseStore.unserved;
      return response;
    }
    const key = `${method} ${canonicalUrl(url, this.callerIds, this.venue)}`;
    const queue =
      this.responses.get(`${key} BODY ${canonicalBody(body, this.callerIds, this.venue)}`) ?? this.responses.get(key);
    if (!queue?.length) {
      if (this.unserved < 10)
        console.error(`unserved ${method} ${url} ${canonicalBody(body, this.callerIds, this.venue)}`);
      this.unserved++;
      return { status: 599, body: '' };
    }
    // Reconciliation reads may advance to a new fixture stage after a command.
    const eligible = queue.filter(
      (r) => (r.afterCommand ?? 0) <= this.commandIndex && (r.availableAfter ?? r.effectiveAt ?? 0) <= now,
    );
    if (!eligible.length) {
      this.unserved++;
      return { status: 599, body: '' };
    }
    const stage = Math.max(...eligible.map((r) => r.afterCommand ?? 0));
    const current = eligible.filter((r) => (r.afterCommand ?? 0) === stage);
    const timed = current.filter((record) => (record.availableAfter ?? record.effectiveAt ?? 0) <= now);
    const timedRead =
      !this.isWrite(method, url) &&
      current.some((record) => record.availableAfter !== undefined || record.effectiveAt !== undefined);
    const record = timedRead ? timed.at(-1) : current[0];
    if (!record) {
      this.unserved++;
      return { status: 599, body: '' };
    }
    if (this.isWrite(method, url) && this.venue.compareWriteOrder && this.writeOrder[0] !== record) {
      this.unserved++;
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
    return { status: record.status, headers: record.headers ?? '', body: responseBody };
  }

  connect(url, onOpen, onMessage, onClose) {
    const key = canonicalUrl(url, this.callerIds, this.venue);
    const generation = (this.generations.get(key) ?? 0) + 1;
    this.generations.set(key, generation);
    const socket = {
      url,
      generation,
      ordinal: generation - 1,
      onClose,
      onMessage: (text) => {
        const correlated = this.correlateSocketResponse(socket, text);
        this.audit?.received(url, correlated, generation);
        onMessage(correlated);
      },
      closed: false,
      subscriptions: new Set(),
      loginVerified: false,
      topics: new Map(),
      requests: new Map(),
      requestIds: new Set(),
      requestCounts: new Map(),
      mappedRequests: new Map(),
      responseIds: new Set(),
    };
    this.audit?.connected?.(url, generation);
    setImmediate(() => {
      if (socket.closed) return;
      const key = canonicalUrl(url, this.callerIds, this.venue);
      if (!this.open.has(key)) this.open.set(key, []);
      this.open.get(key).push(socket);
      onOpen();
      for (const waiter of [...this.waiters]) waiter();
    });
    return socket;
  }

  send(socket, text) {
    this.sent(socket.url, text);
    for (const key of subscriptionKeys(socket.url, text)) {
      this.protocolSubscriptions.add(`${socket.generation} ${key}`);
      if (this.startupSubscriptions.get(key) === false) {
        this.startupSubscriptions.set(key, true);
        this.pendingStartupSubscriptions--;
      }
    }
    this.audit?.sent(socket.url, text, socket.generation);
    if (this.venue.respondToPing && text === 'ping') {
      setImmediate(() => {
        if (!socket.closed) socket.onMessage('pong');
      });
      return;
    }
    try {
      const message = JSON.parse(text);
      const request = this.venue.socketRequest?.(message);
      if (request) {
        if (socket.requestIds.has(request.id)) throw new Error(`duplicate websocket request ${request.key}`);
        if (this.recordedSocketRequests.size || this.venue.requireSocketRequestRecords) {
          const queueKey = `${canonicalUrl(socket.url)} ${socket.ordinal} ${request.key}`;
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
      if (message.event) {
        const subscription = subscriptionMessage(message);
        for (const topic of subscription?.topics ?? []) {
          if (typeof topic !== 'string' || !topic) throw new Error('invalid websocket subscription topic');
          if (subscription.op === 'subscribe') socket.subscriptions.add(topic);
          else socket.subscriptions.delete(topic);
        }
      }
      if (this.isPrivate && this.venue.verifySubscribe && !this.venue.verifySubscribe(message)) {
        this.invalidSignatures++;
        throw new Error('invalid replay websocket signature');
      }
      for (const topic of Array.isArray(message.params) ? message.params : []) {
        if (message.method === 'SUBSCRIBE') {
          socket.subscriptions.add(topic);
          if (
            !this.venue.marketFilter ||
            topic.startsWith('account.') ||
            this.venue.marketFilter({ symbol: topic.split('.').at(-1) })
          )
            this.subscriptions.add(`${message.method} ${topic}`);
        }
        if (message.method === 'UNSUBSCRIBE') socket.subscriptions.delete(topic);
      }
      for (const waiter of [...this.waiters]) waiter();
      // OKX's opt-in auth validation also uses its v5 subscription argument model.
      const validateTopics = this.venue.validateSubscriptions ?? this.venue.validateLogin;
      if (validateTopics && ['subscribe', 'unsubscribe'].includes(message.op)) {
        if (!Array.isArray(message.args)) throw new Error(`Invalid subscription on ${socket.url}`);
        for (const topic of message.args) {
          if (!topic || typeof topic.channel !== 'string' || !topic.channel)
            throw new Error(`Invalid subscription topic on ${socket.url}`);
          const key = JSON.stringify(Object.fromEntries(Object.entries(topic).sort(([a], [b]) => a.localeCompare(b))));
          if (message.op === 'subscribe') socket.topics.set(key, topic);
          else socket.topics.delete(key);
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
    const topics = [...socket.topics.values()].filter((topic) => topic.channel === message.arg.channel);
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
    if (this.missingWrites) throw new Error(`${this.missingWrites} taped write(s) were not sent`);
  }

  close(socket, code = 1000) {
    if (socket.closed) return;
    socket.closed = true;
    this.audit?.closed?.(socket.url, socket.generation);
    socket.onClose?.(code);
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
    const scope = `${canonicalUrl(socket.url)} ${socket.ordinal}`;
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

  async socketFor(url, text, ordinal) {
    const stream = text && this.venue.streamOf?.(text);
    const find = () => {
      const sockets = (this.open.get(canonicalUrl(url, this.callerIds, this.venue)) ?? []).filter(
        (socket) => !socket.closed && (ordinal === undefined || socket.ordinal === ordinal),
      );
      return stream
        ? sockets.find((s) => [...s.subscriptions].some((topic) => stream === topic || stream.startsWith(`${topic}.`)))
        : sockets.at(-1);
    };
    const ready = find();
    if (ready) return ready;
    let waiter;
    try {
      const pending = new Promise((resolve) => {
        waiter = () => {
          const socket = find();
          if (socket) resolve(socket);
        };
        this.waiters.add(waiter);
        waiter();
      });
      const label = `socket ${url} ${stream ?? 'open'}`;
      return await (this.replayClock?.active
        ? this.replayClock.wait(pending, label, this.timeoutMs)
        : withTimeout(pending, label, this.timeoutMs));
    } finally {
      this.waiters.delete(waiter);
    }
  }

  /**
   * Deliver every frame in order, yielding to the event loop between frames.
   * Commands are awaited where they fall. A private tape is paced: each record
   * waits until its time after the account is ready.
   */
  async play(onTick, ready = async () => {}, runCommand = async () => {}, clock, begin = () => {}) {
    const urls = [...new Set(this.frames.filter((f) => f.url).map((f) => f.url))];
    if (urls.length) await this.socketFor(urls[0]);
    await withTimeout(
      (async () => {
        await ready();
        while ((this.audit && !this.audit.ready()) || this.pendingStartupSubscriptions)
          await new Promise((resolve) => setTimeout(resolve, 5));
      })(),
      'account/books readiness',
      this.timeoutMs,
    );
    begin();
    clock?.start();
    this.replayClock = clock;
    const start = performance.now();
    let delivered = 0;
    const warm = Math.floor(this.frames.length / 4);
    for (const frame of this.frames) {
      if (delivered === warm) this.warmMark = { thread: process.threadCpuUsage(), wall: performance.now() };
      if (this.isPrivate) {
        const wait = frame.t - (performance.now() - start);
        if (wait > 0) await new Promise((resolve) => setTimeout(resolve, wait));
      }
      this.time = frame.t;
      await clock?.advance(frame.t);
      if (frame.type === 'command') {
        this.commandIndex++;
        this.results.push(
          await (
            clock
              ? clock.wait(runCommand(frame), `command ${frame.op}`, this.timeoutMs)
              : withTimeout(runCommand(frame), `command ${frame.op}`, this.timeoutMs)
          ).then(
            (ids) => ({ ok: ids }),
            (error) => {
              if (String(error?.message).startsWith('replay timeout:')) throw error;
              return { error: String(error?.message ?? error) };
            },
          ),
        );
      } else {
        const socket = await this.socketFor(frame.url, frame.text, frame.socket);
        if (frame.type === 'close') {
          this.close(socket, frame.code ?? 1006);
        } else if (frame.type === 'ws') {
          if (
            (this.venue.validateLogin || this.venue.validateSubscriptions) &&
            frame.text.trimStart().startsWith('{')
          ) {
            const message = JSON.parse(frame.text);
            if (
              this.venue.validateLogin &&
              message.event === 'login' &&
              String(message.code) === '0' &&
              !socket.loginVerified
            ) {
              throw new Error(`Taped login success preceded a valid login on ${frame.url}`);
            }
            this.assertSubscribed(socket, message);
          }
          socket.onMessage(frame.text);
        }
      }
      delivered++;
      onTick?.(frame.t);
      await new Promise(setImmediate);
    }
    return delivered;
  }

  results = [];
  sentRequests = new Set();
  sent(url, text) {
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
