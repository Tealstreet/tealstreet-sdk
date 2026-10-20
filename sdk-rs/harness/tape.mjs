import { createHmac, timingSafeEqual } from 'node:crypto';
import { canonicalUrl as strictUrl, requestKey, callerIdsOf, isWriteRequest, subscriptionKeys, subscriptionMessage } from './requests.mjs';
import { parseRequestJson } from './request-json.mjs';
import { Responses } from './responses.mjs';
import { setTimeout as realTimeout, clearTimeout as realClearTimeout } from 'node:timers';
import { LOADED_FLAG } from '../hosts/wasm/js/wire.mjs';
export class ReplayTimeout extends Error {}
export const withTimeout = async (promise, label, ms = 30000) => {
  let timer;
  try {
    return await Promise.race([
      promise,
      new Promise((_, reject) => {
        timer = realTimeout(() => reject(new ReplayTimeout(`replay timeout: ${label}`)), ms);
      }),
    ]);
  } finally {
    realClearTimeout(timer);
  }
};
export const within = (promise, ms, label) => withTimeout(promise, label, ms);
export const canonicalUrl = (url, callerIds = new Set(), venue = {}) => strictUrl(venue.requestUrl?.(url) ?? url, callerIds);
export const canonicalBody = (body, callerIds = new Set(), venue = {}) => {
  const url = 'https://api.mexc.com/api/v1/private/order/cancel';
  const prefix = requestKey('POST', url, undefined, callerIds, venue);
  return requestKey('POST', url, body, callerIds, venue).slice(prefix.length + 1);
};
export const stable = (value) =>
  Array.isArray(value)
    ? value.map(stable)
    : value && typeof value === 'object'
      ? Object.fromEntries(
          Object.keys(value)
            .sort()
            .map((key) => [key, stable(value[key])]),
        )
      : value;
export const sameCommandResults = (a, b) => JSON.stringify(stable(a)) === JSON.stringify(stable(b));
export const deliverSocketFrame = (socket, text, audit, validate) => {
  if (!socket.auditInSocket) audit?.received(socket.url, text, socket.generation);
  validate?.(socket, text);
  socket.onMessage(text);
};
export const replayStoreReady = (loaded, isPrivate) => {
  const publicMask = LOADED_FLAG.MARKETS | LOADED_FLAG.TICKERS;
  return (
    (loaded &
      (isPrivate ? publicMask | LOADED_FLAG.BALANCE | LOADED_FLAG.ORDERS | LOADED_FLAG.POSITIONS : publicMask)) ===
    (isPrivate ? publicMask | LOADED_FLAG.BALANCE | LOADED_FLAG.ORDERS | LOADED_FLAG.POSITIONS : publicMask)
  );
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

export class Tape {
  constructor(records, venue = {}, isPrivate = venue.isPrivate ?? false, timeoutMs = 30000, impl, books = [], options) {
    if (impl !== undefined && !['ts', 'rust'].includes(impl)) throw new Error('Unknown replay implementation');
    records = records.filter((record) => {
      if (record.type !== 'http' || record.impl === undefined) return true;
      if (!['ts', 'rust'].includes(record.impl)) throw new Error('Unknown HTTP record implementation');
      if (impl === undefined) throw new Error('Tagged HTTP records require a replay implementation');
      return record.impl === impl;
    });
    this.records =
      impl === 'ts'
        ? records.map((record) =>
            record.tsRequestBody === undefined ? record : { ...record, requestBody: record.tsRequestBody },
          )
        : records;
    this.venue = venue;
    this.isPrivate = isPrivate;
    this.timeoutMs = timeoutMs;
    this.options = options ?? {
      key: 'replay-key',
      secret: 'replay-secret',
      passphrase: 'replay-passphrase',
      ...venue.replayCredentials,
    };
    this.callerIds = callerIdsOf(records);
    this.bookSymbols = books;
    this.bitget = venue.rust?.startsWith('bitget') ?? false;
    this.generated = new Set();
    this.generatedAliases = new Map();
    this.identityOptions = this.bitget
      ? { ...venue, requestBody: (body, context) => this.bitgetBody(body, context) }
      : venue;
    this.keyFor = (method, url, body) => requestKey(method, url, body, this.callerIds, this.identityOptions);
    this.wireAliases = new Map();
    if (impl === 'ts')
      for (const record of records)
        if (record.tsRequestBody !== undefined) {
          this.wireAliases.set(
            this.keyFor(record.method, record.url, record.tsRequestBody),
            this.keyFor(record.method, record.url, record.requestBody),
          );
        }
    this.expectedWrites = records
      .filter(
        (record) =>
          record.type === 'http' &&
          (venue.isWrite?.(record.method, record.url) ?? isWriteRequest(record.method, record.url)),
      )
      .map((record) => this.keyFor(record.method, record.url, record.requestBody));
    this.publicTickers = this.venue.publicTickers?.(this.records) ?? [];
    this.audit = this.venue.createSocketAudit?.(impl, {
      isPrivate: this.isPrivate,
      books: this.bookSymbols,
      tickers: this.publicTickers,
    });
    this.responses = new Responses(
      this.records,
      this.keyFor,
      this.venue.checkRequest,
      (method, url) => this.venue.isWrite?.(method, url) ?? isWriteRequest(method, url),
      true,
      impl,
    );
    this.frames = this.records.filter(
      (r) => r.type === 'ws' || r.type === 'command' || r.type === 'close' || r.type === 'open',
    );
    this.open = new Map();
    this.waiters = [];
    this.commandIndex = 0;
    this.time = 0;
    this.subscriptions = new Set();
    // Retain expected keys/backing storage across both memory samples: deleting
    // startup-only harness state would subtract it from workload retained memory.
    this.startupSubscriptions = new Map(
      Array.from(this.venue.startupSubscriptions?.(this.records, this.bookSymbols) || [], (key) => [key, false]),
    );
    this.pendingStartupSubscriptions = this.startupSubscriptions.size;
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
  }

  get unserved() {
    return this.responses.unserved;
  }

  mintedField(key, url) {
    const path = new URL(url).pathname;
    return key === 'newClientOid'
      ? /\/modify-order$/.test(path)
      : key === 'clientOid' &&
          /\/(?:batch-place-order|place-order|place-tpsl-order|place-plan-order|place-strategy-order)$/.test(path);
  }

  bitgetBody(body, { url }) {
    const visit = (value) => {
      if (JSON.isRawJSON(value)) return value;
      if (Array.isArray(value)) return value.map(visit);
      if (!value || typeof value !== 'object') return value;
      return Object.fromEntries(
        Object.entries(value).map(([key, item]) => [
          key,
          this.mintedField(key, url) && typeof item === 'string' && item.length && !this.callerIds.has(item)
            ? '<generated>'
            : key === 'clientOid' && typeof item === 'string'
              ? (this.generatedAliases.get(item) ?? item)
              : visit(item),
        ]),
      );
    };
    return visit(body);
  }

  validateBitgetMatch(record, method, url, body) {
    if (!this.bitget || method === 'GET') return;
    const decode = (value) => {
      if (ArrayBuffer.isView(value) || value instanceof ArrayBuffer) value = new TextDecoder().decode(value);
      return typeof value === 'string' ? parseRequestJson(value) : value;
    };
    const sent = decode(body);
    const ids = [];
    const visit = (value) => {
      if (!value || typeof value !== 'object') return;
      for (const [key, item] of Object.entries(value)) {
        if (this.mintedField(key, url) && (key === 'newClientOid' || !this.callerIds.has(item))) {
          if (
            typeof item !== 'string' ||
            !/^[.A-Za-z0-9:/_-]{1,32}$/.test(item) ||
            (key === 'newClientOid' && this.callerIds.has(item))
          )
            throw new Error(`invalid generated ${key}`);
          ids.push(item);
        } else visit(item);
      }
    };
    visit(sent);
    const retry = String(JSON.parse(record.body).code) === '40009';
    if (!retry) {
      if (new Set(ids).size !== ids.length) throw new Error('generated client ID reused within batch');
      for (const id of ids) if (this.generated.has(id)) throw new Error(`generated client ID reused: ${id}`);
      for (const id of ids) this.generated.add(id);
    }
    const bind = (expected, actual) => {
      if (!expected || !actual || typeof expected !== 'object' || typeof actual !== 'object') return;
      for (const key of Object.keys(expected)) {
        if (
          this.mintedField(key, url) &&
          typeof expected[key] === 'string' &&
          typeof actual[key] === 'string' &&
          !this.callerIds.has(actual[key])
        )
          this.generatedAliases.set(actual[key], expected[key]);
        else bind(expected[key], actual[key]);
      }
    };
    bind(decode(record.requestBody), sent);
  }

  topicKey(topic) {
    return `${topic.instType ?? ''}|${topic.channel ?? topic.topic}|${topic.instId ?? topic.symbol ?? ''}`;
  }

  verifySignature(verify, value, label) {
    try {
      if (verify(value)) return;
      throw new Error(label);
    } catch (error) {
      // Persist both a rejected signature and a verifier that cannot parse it.
      this.invalidSignatures++;
      throw error;
    }
  }

  respond(method, url, body, headers) {
    try {
      this.venue.validateRequest?.({ method, url, body, headers });
    } catch (error) {
      this.invalidSignatures++;
      throw error;
    }
    if (this.isPrivate && this.venue.verifyRequest) {
      this.verifySignature(
        this.venue.verifyRequest,
        { method, url, body, headers },
        `invalid replay signature: ${method} ${url}`,
      );
    }
    const actualKey = this.keyFor(method, url, body);
    const key = this.wireAliases.get(actualKey) ?? actualKey;
    this.requests.add(key);
    if (this.responses.isWrite(method, url)) this.writes.push(key);
    if (this.responses.isWrite(method, url) && this.venue.compareWriteOrder && this.expectedWrites[0] !== key) {
      if (process.env.RUST_REQUEST_TRACE) console.error('unexpected write', key, 'expected', this.expectedWrites[0]);
      this.responses.unserved++;
      return { status: 599, body: '' };
    }
    const elapsed = this.replayClock?.active ? this.replayClock.time - this.replayClock.epoch : this.time;
    const response = this.bitget || this.venue.validateMatchedRequest
      ? this.responses.respond(
          method,
          url,
          body,
          this.commandIndex,
          (record) => {
            this.validateBitgetMatch(record, method, url, body);
            this.venue.validateMatchedRequest?.(record, { method, url, body, headers, commandIndex: this.commandIndex });
          },
          elapsed,
        )
      : this.responses.respond(method, url, body, this.commandIndex, elapsed);
    if (this.responses.isWrite(method, url) && response.status !== 599) this.expectedWrites.shift();
    return response;
  }

  connect(url, onOpen, onMessage, onClose) {
    const key = canonicalUrl(url, this.callerIds);
    const generation = (this.generations.get(key) ?? 0) + 1;
    this.generations.set(key, generation);
    const socket = {
      url, ordinal: generation - 1, generation, onClose, closed: false, loginVerified: false,
      topics: new Map(), subscriptions: new Set(), requests: new Map(), requestIds: new Set(),
      requestCounts: new Map(), mappedRequests: new Map(), responseIds: new Set(),
      auditInSocket: Boolean(this.venue.socketResponse),
      onMessage: this.venue.socketResponse ? (text) => {
        const correlated = this.correlateSocketResponse(socket, text);
        this.audit?.received(url, correlated, generation);
        onMessage(correlated);
      } : onMessage,
    };
    this.audit?.connected?.(url, generation);
    setImmediate(() => {
      if (socket.closed) return;
      // Bybit adds a generated requestId to each connection URL.
      if (!this.open.has(key)) this.open.set(key, new Set());
      this.open.get(key).add(socket);
      onOpen();
      this.waiters = this.waiters.filter((waiter) => !waiter());
    });
    return socket;
  }

  send(socket, text) {
    this.venue.validateSocketMessage?.(socket, text);
    let message;
    try {
      message = JSON.parse(text);
    } catch {
      /* Plain-text heartbeat. */
    }
    if (this.venue.bootstrapPrivateFromTape && message?.op && message?.req_id)
      socket.requests.set(message.op, message.req_id);
    if (message && this.isPrivate && this.venue.verifySubscribe) {
      this.verifySignature(this.venue.verifySubscribe, message, 'invalid replay websocket signature');
    }
    if (message) {
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
    }
    if (Array.isArray(message?.params)) for (const topic of message.params) {
      if (message.method === 'SUBSCRIBE') socket.subscriptions.add(topic);
      if (message.method === 'UNSUBSCRIBE') socket.subscriptions.delete(topic);
    }
    this.venue.onSocketSend?.(socket, text);
    // Authentication may classify a shared-URL socket after its open callback.
    this.waiters = this.waiters.filter((waiter) => !waiter());
    this.sent(socket.url, text);
    this.audit?.sent(socket.url, text, socket.generation);
    const compared = this.venue.marketFilter && Array.isArray(message?.params)
      ? { ...message, params: message.params.filter((topic) => topic.startsWith('account.') || this.venue.marketFilter({ symbol: topic.split('.').at(-1) })) } : text;
    for (const key of subscriptionKeys(socket.url, compared, this.callerIds, this.venue)) {
      this.subscriptions.add(this.venue.subscriptionGenerations ? `${socket.generation} ${key}` : key);
      if (this.startupSubscriptions.get(key) === false) {
        this.startupSubscriptions.set(key, true);
        this.pendingStartupSubscriptions--;
      }
    }
    if (this.venue.pingReply) {
      const reply = this.venue.pingReply(text);
      if (reply) setImmediate(() => { if (!socket.closed) this.deliver(socket, reply); });
    }
    if (this.venue.respondToPing && text === 'ping') {
      setImmediate(() => {
        if (!socket.closed) socket.onMessage('pong');
      });
      return;
    }
    // OKX's opt-in auth validation also uses its v5 subscription argument model.
    const validateTopics = this.bitget || (this.venue.validateSubscriptions ?? this.venue.validateLogin);
    if (!this.venue.validateLogin && !validateTopics) return;
    if (text === 'ping' || text === 'pong') return;
    message = JSON.parse(text);
    if (validateTopics && ['subscribe', 'unsubscribe'].includes(message.op)) {
      if (!Array.isArray(message.args)) throw new Error(`Invalid subscription on ${socket.url}`);
      for (const topic of message.args) {
        if (
          !topic ||
          typeof (topic.channel ?? (this.bitget ? topic.topic : undefined)) !== 'string' ||
          !(topic.channel ?? topic.topic)
        )
          throw new Error(`Invalid subscription topic on ${socket.url}`);
        const key = this.bitget
          ? this.topicKey(topic)
          : JSON.stringify(Object.fromEntries(Object.entries(topic).sort(([a], [b]) => a.localeCompare(b))));
        if (message.op === 'subscribe') socket.topics.set(key, topic);
        else socket.topics.delete(key);
      }
      return;
    }
    if (!this.venue.validateLogin || !/\/(private|business)$/.test(new URL(socket.url).pathname)) return;
    if (message.op !== 'login') return;
    const arg = message.args?.length === 1 ? message.args[0] : undefined;
    const timestamp = String(arg?.timestamp ?? '');
    if (
      arg?.apiKey !== this.options.key ||
      arg?.passphrase !== this.options.passphrase ||
      !/^\d+$/.test(timestamp) ||
      !Number.isSafeInteger(Number(timestamp)) ||
      Math.abs(Number(timestamp) - Date.now() / 1000) > 30
    ) {
      throw new Error(`Invalid OKX login credentials/timestamp on ${socket.url}`);
    }
    const expected = createHmac('sha256', this.options.secret)
      .update(`${timestamp}GET/users/self/verify`)
      .digest('base64');
    const supplied = Buffer.from(String(arg.sign ?? ''));
    const expectedBytes = Buffer.from(expected);
    if (supplied.length !== expectedBytes.length || !timingSafeEqual(supplied, expectedBytes)) {
      throw new Error(`Invalid OKX login signature on ${socket.url}`);
    }
    socket.loginVerified = true;
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
    // The host may catch a transport exception and retain already-loaded state.
    // A failed verifier remains fatal at the final replay check.
    if (this.invalidSignatures) throw new Error(`${this.invalidSignatures} invalid replay signature(s)`);
    const missing = this.responses.unusedWrites();
    if (missing) throw new Error(`${missing} taped write(s) were not sent`);
  }

  close(socket, code = 1000) {
    if (socket.closed) return;
    socket.closed = true;
    this.audit?.closed?.(socket.url, socket.generation);
    socket.onClose?.(code);
  }

  sockets(url, ordinal) {
    return [...(this.open.get(canonicalUrl(url, this.callerIds)) ?? [])].filter((socket) =>
      !socket.closed && (ordinal === undefined || socket.ordinal === ordinal));
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

  async socketFor(url, text, ordinal, openingOnly = false) {
    if (typeof text === 'number') { ordinal = text; text = undefined; }
    const stream = !openingOnly && text && this.venue.streamOf?.(text);
    const active = () => this.sockets(url, ordinal).filter((socket) =>
      (!stream || [...socket.subscriptions].some((topic) => stream === topic || stream.startsWith(`${topic}.`)))
      && (openingOnly || ordinal !== undefined || text === undefined || !this.venue.socketMatches
        || this.venue.socketMatches(socket, text)));
    if (!active().length) {
      let waiter;
      const pending = new Promise((resolve) => {
        waiter = () => (active().length ? (resolve(), true) : false);
        this.waiters.push(waiter);
        waiter();
      });
      try {
        await (this.replayClock?.active
          ? this.replayClock.wait(pending, `socket ${url} ${stream ?? 'open'}`, this.timeoutMs)
          : withTimeout(pending, `socket ${url} ${stream ?? 'open'}`, this.timeoutMs));
      } finally {
        this.waiters = this.waiters.filter((entry) => entry !== waiter);
      }
    }
    if (openingOnly || stream || ordinal !== undefined || this.venue.socketMatches || this.venue.rust !== 'hyperliquid') return active().at(-1);
    // Legacy tapes without a stream discriminator deliver to every listener,
    // including Hyperliquid's separate public/private listeners on one URL.
    return {
      onClose: (code) => {
        for (const socket of active()) this.close(socket, code);
      },
      onMessage: (text) => {
        for (const socket of active()) this.deliver(socket, text);
      },
    };
  }

  deliver(socket, text) {
    if (text === undefined && socket.text !== undefined) return this.deliverFrame(socket);
    if ((this.venue.validateLogin || this.venue.validateSubscriptions) && text.trimStart().startsWith('{')) {
      const message = JSON.parse(text);
      if (
        this.venue.validateLogin &&
        message.event === 'login' &&
        String(message.code) === '0' &&
        !socket.loginVerified
      )
        throw new Error(`Taped login success preceded a valid login on ${socket.url}`);
      this.assertSubscribed(socket, message);
    }
    deliverSocketFrame(socket, text, this.audit, this.venue.validateSocketFrame);
  }

  async deliverFrame(frame) {
    if (frame.type === 'close' && frame.socket === undefined && this.venue.socketMatches && this.sockets(frame.url).length > 1) {
      throw new Error('Ambiguous close on a shared public/private endpoint');
    }
    const latest = await this.socketFor(frame.url, frame.text, frame.socket);
    let text = frame.text;
    if (this.venue.bootstrapPrivateFromTape && frame.type === 'ws') {
      const message = JSON.parse(text);
      if (message.req_id === '@latest') {
        const deadline = performance.now() + this.timeoutMs;
        while (!latest.requests.has(message.op)) {
          if (performance.now() >= deadline)
            throw new ReplayTimeout(`replay timeout: ${message.op} request for acknowledgement`);
          await new Promise((resolve) => realTimeout(resolve, 5));
        }
        text = JSON.stringify({ ...message, req_id: latest.requests.get(message.op) });
      }
    }
    const sockets = this.bitget && frame.socket === undefined
      ? [...(this.open.get(canonicalUrl(frame.url, this.callerIds)) ?? [])].filter((socket) => !socket.closed) : [latest];
    if (frame.type === 'close') {
      for (const socket of sockets) {
        if (socket.url) this.close(socket, frame.code ?? 1006);
        else socket.onClose(frame.code ?? 1006);
      }
      return;
    }
    if (frame.type === 'open') return;
    let arg;
    try {
      arg = JSON.parse(frame.text)?.arg;
    } catch {
      /* Plain heartbeat. */
    }
    const key = this.bitget && arg && this.topicKey(arg);
    const matching = key ? sockets.filter((socket) => socket.topics.has(key)) : sockets;
    if (key && !matching.length) throw new Error(`frame has no subscribed socket: ${frame.url} ${key}`);
    for (const socket of matching) {
      if (socket.url) this.deliver(socket, text);
      else socket.onMessage(text);
    }
  }

  /**
   * Deliver every frame in order, yielding to the event loop between frames.
   * Commands are awaited where they fall. A private tape is paced: each record
   * waits until its time after the account is ready.
   */
  async play(onTick, ready = async () => {}, runCommand = async () => {}, clock, begin = () => {}) {
    const first = this.frames.find((frame) => frame.url);
    const startupDeadline = performance.now() + this.timeoutMs;
    await within((async () => {
      // Fence only the first physical socket; a later command may create the
      // first frame's subscription. Bootstrap auth then selects its own socket.
      if (first) await this.socketFor(first.url, first.text, first.socket, true);
      for (const frame of this.frames.filter((frame) => this.venue.bootstrapFrame?.(frame))) {
        await this.deliverFrame(frame);
        await new Promise(setImmediate);
      }
      await ready(!this.venue.bootstrapPrivateFromTape);
      // Native and TS outboxes batch sends independently of handler registration.
      // Begin the tape only after every seed-derived startup topic reached send.
      while (this.pendingStartupSubscriptions || (this.audit?.ready && !this.audit.ready())) {
        if (performance.now() >= startupDeadline) throw new ReplayTimeout('replay timeout: startup subscriptions');
        await new Promise((resolve) => realTimeout(resolve, 1));
      }
    })(), this.timeoutMs, 'Replay startup');
    begin();
    clock?.start();
    this.replayClock = clock;
    const start = performance.now();
    let delivered = 0;
    const warm = Math.floor(this.frames.reduce((count, frame) => count + (this.venue.bootstrapFrame?.(frame) ? 0 : 1), 0) / 4);
    for (const frame of this.frames) {
      if (this.venue.bootstrapFrame?.(frame)) continue;
      if (delivered === warm) this.warmMark = { thread: process.threadCpuUsage(), wall: performance.now() };
      if (this.isPrivate) {
        const wait = frame.t - (performance.now() - start);
        if (wait > 0) await new Promise((resolve) => setTimeout(resolve, wait));
      }
      this.time = frame.t;
      await clock?.advance(frame.t);
      if (frame.type === 'command') {
        await (clock
          ? clock.wait(ready(true), 'account readiness', this.timeoutMs)
          : withTimeout(ready(true), 'account readiness', this.timeoutMs));
        this.commandIndex++;
        this.results.push(await (clock ? clock.wait(runCommand(frame), `command ${frame.op}`, this.timeoutMs) : withTimeout(runCommand(frame), `command ${frame.op}`, this.timeoutMs)).then((ids) => ({ ok: ids }), (error) => {
          if (error instanceof ReplayTimeout || String(error?.message).startsWith('replay timeout:')) throw error;
          return { error: String(error?.message ?? error) };
        }));
      } else {
        await this.deliverFrame(frame);
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

  get missingWrites() {
    return this.responses.unusedWrites();
  }
  get protocolSubscriptions() {
    return this.subscriptions;
  }
  isWrite(method, url) {
    return this.responses.isWrite(method, url);
  }
  invalidSignatures = 0;
  results = [];
  requests = new Set();
  subscriptions = new Set();
  writes = [];
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
    const capture = (value) =>
      JSON.stringify(value, (_key, entry) =>
        entry && !Array.isArray(entry) && typeof entry === 'object'
          ? Object.fromEntries(Object.entries(entry).sort(([a], [b]) => a.localeCompare(b)))
          : entry,
      );
    if (message.op === 'subscribe' || message.op === 'unsubscribe') {
      for (const topic of message.args ?? []) this.sentRequests.add(capture({ url, op: message.op, topic }));
    } else if (message.op === 'login') {
      const args = message.args.map(({ timestamp, nonce, sign, ...credentials }) => credentials);
      this.sentRequests.add(capture({ url, op: 'login', args }));
    }
  }
}

// Compatibility factory uses the same reviewed transport as actual replay.
export const createTape = (records, venue, isPrivate, settings = {}) =>
  new Tape(
    records,
    venue,
    isPrivate,
    settings.socketTimeoutMs ?? 30000,
    settings.impl,
    settings.books ?? [],
    settings.credentials,
  );
