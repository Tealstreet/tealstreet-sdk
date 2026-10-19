import { createHash, createHmac, timingSafeEqual } from 'node:crypto';
import { parseRequestJson } from './request-json.mjs';
import { Responses } from './responses.mjs';
import { isRead, subscriptionKeys, requestKey as typedRequestKey } from './requests.mjs';
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
  start(elapsed = 0) {
    const wall = this.original.now();
    for (const timer of this.timers.values()) {
      this.original.clearTimeout(timer.handle);
      timer.at = this.epoch + elapsed + Math.max(0, timer.at - wall);
    }
    this.active = true;
    this.time = this.epoch + elapsed;
  }
  async advance(t) {
    if (!this.active) return;
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
  constructor(records, venue = {}, isPrivate = venue.isPrivate ?? false, timeoutMs = venue.timeoutMs ?? 30000, impl = venue.impl, books = venue.books ?? []) {
    this.records = records;
    this.impl = impl;
    this.canonicalize = venue.canonicalize ?? ((url) => canonicalUrl(url, this.callerIds, venue));
    this.explicitOpens = venue.explicitOpens ?? false;
    this.legacyLatest = venue.legacyLatest ?? true;
    this.venue = venue;
    this.isPrivate = isPrivate;
    this.timeoutMs = timeoutMs;
    this.books = books;
    this.commandIndex = 0;
    this.time = 0;
    this.generations = new Map();
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
    this.responses = venue.responses ?? new Map();
    this.writeOrder = [];
    if (!venue.responses) for (const record of records.filter((r) => r.type === 'http')) {
      // Per-fixture KNOWN_TS_FIXES exception; never a fallback for Rust.
      const expectedBody = impl === 'ts' ? (record.tsRequestBody ?? record.requestBody) : record.requestBody;
      const key = `${record.method} ${canonicalUrl(record.url, this.callerIds, this.venue)}${expectedBody === undefined ? '' : ` BODY ${canonicalBody(expectedBody, this.callerIds, this.venue)}`}`;
      if (!this.responses.has(key)) this.responses.set(key, []);
      this.responses.get(key).push(record);
      if (this.isWrite(record.method, record.url)) this.writeOrder.push(record);
    }
    if (!venue.responses && (venue.checkRequest || venue.requestKey || venue.requestBodies)) {
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
    // Share immutable canonical URL strings across every frame and both samples.
    const urls = new Map();
    this.frames = records.filter((r) => ['ws', 'command', 'open', 'close'].includes(r.type)).map((frame) => {
      if (frame.url) {
        if (!urls.has(frame.url)) urls.set(frame.url, this.canonicalize(frame.url));
        frame = { ...frame, url: urls.get(frame.url), connection: frame.connection ?? frame.socket };
      }
      return venue.prepareFrame ? venue.prepareFrame(frame) : frame;
    });
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
      return await (this.replayClock?.active ? this.replayClock.wait(result, `command ${frame.op}`, this.timeoutMs) : withTimeout(result, `command ${frame.op}`, this.timeoutMs));
    }
    finally { this.activeCommands--; }
  }

  pendingResponses = new Set();
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
  nativeSubscriptions = new Set();
  subscriptions = new Set();
  protocolSubscriptions = new Set();
  isWrite(method, url) {
    return this.venue.isWrite?.(method, url) ?? !isRead(method, url);
  }
  get missingWrites() {
    if (typeof this.responses.unusedWrites === 'function') return this.responses.unusedWrites();
    if (this.responseStore) return this.responseStore.unusedWrites();
    return [...this.responses.values()].flat().filter((record) => this.isWrite(record.method, record.url)).length;
  }
  respond(method, url, body, headers) {
    this.venue.validateRequest?.({ method, url, body, headers });
    if (this.isPrivate && this.venue.verifyRequest && !this.venue.verifyRequest({ method, url, body, headers })) {
      this.invalidSignatures++;
      throw new Error(`invalid replay signature: ${method} ${url}`);
    }
    if (typeof this.responses.respond === 'function') return this.responses.respond(method, url, body, this.commandIndex, this.responseTime);
    const now = this.responseTime;
    if (this.responseStore) {
      const response = this.responseStore.respond(method, url, body, this.commandIndex, now);
      this._unserved = this.responseStore.unserved;
      return response;
    }
    const key = `${method} ${canonicalUrl(url, this.callerIds, this.venue)}`;
    const queue =
      this.responses.get(`${key} BODY ${canonicalBody(body, this.callerIds, this.venue)}`) ?? this.responses.get(key);
    if (!queue?.length) {
      if (this.unserved < 10)
        console.error(`unserved ${method} ${url} ${canonicalBody(body, this.callerIds, this.venue)}`);
      this._unserved++;
      return { status: 599, body: '' };
    }
    // Reconciliation reads may advance to a new fixture stage after a command.
    const eligible = queue.filter(
      (r) => (r.afterCommand ?? 0) <= this.commandIndex && (r.availableAfter ?? r.effectiveAt ?? 0) <= now,
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
      current.some((record) => record.availableAfter !== undefined || record.effectiveAt !== undefined);
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

  connect(url, onOpen, onMessage, onClose) {
    const rawUrl = url;
    url = this.canonicalize(url);
    const connection = this.connectionCounts.get(url) ?? 0;
    this.connectionCounts.set(url, connection + 1);
    const socket = { url, rawUrl, connection, onOpen, onMessage: (text) => { this.audit?.received(rawUrl, text); onMessage(text); }, onClose, closed: false, opened: false, topics: new Set(), publicTopics: new Set(), subscriptionArgs: new Map(), subscriptions: new Set(), generation: connection + 1, ordinal: connection, loginVerified: false };
    this.sockets.push(socket);
    this.byConnection.set(`${url}:${connection}`, socket);
    // A recorded open may already be waiting for this socket to be constructed.
    for (const waiter of [...this.waiters]) waiter();
    // Old synthetic fixtures have no lifecycle records; live tapes drive every
    // open explicitly, including failed startup attempts and reconnects.
    const lifecycle = this.socketLifecycle.get(url);
    const hasLifecycle = lifecycle?.hasOpen;
    const explicit = this.explicitOpens || this.records.some((r) => r.type === 'connect' && this.canonicalize(r.url) === url);
    if (!hasLifecycle || (!explicit && lifecycle.initial.has(connection))) setImmediate(() => this.openSocket(socket));
    return socket;
  }

  openSocket(socket) {
    if (socket.closed || socket.opened) return;
    socket.opened = true;
    this.open.set(`${socket.url}:${socket.connection}`, socket);
    socket.onOpen();
    for (const waiter of [...this.waiters]) waiter();
  }

  close(socket, code = 1000, local = false) {
    if (socket.closed) return;
    socket.closed = true;
    if (!local) socket.onClose?.(code);
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
    this.audit?.sent(socket.url, text);
    if (this.venue.respondToPing && text === 'ping') {
      setImmediate(() => {
        if (!socket.closed) socket.onMessage('pong');
      });
      return;
    }
    try {
      const message = JSON.parse(text);
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
            { this.nativeSubscriptions.add(`${message.method} ${topic}`); if (this.subscriptions instanceof Set) this.subscriptions.add(`${message.method} ${topic}`); }
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
          if (message.op === 'subscribe') socket.subscriptionArgs.set(key, topic);
          else socket.subscriptionArgs.delete(key);
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
    if (this.missingWrites) throw new Error(`${this.missingWrites} taped write(s) were not sent`);
  }

  async socketFor(frame, textOrOrdinal, ordinal) {
    if (typeof frame === 'string') frame = { url: this.canonicalize(frame),
      text: typeof textOrOrdinal === 'string' ? textOrOrdinal : undefined,
      connection: typeof textOrOrdinal === 'number' ? textOrOrdinal : ordinal };
    else if (frame.connection === undefined && frame.socket !== undefined) frame = { ...frame, connection: frame.socket };
    const stream = frame.text && this.venue.streamOf?.(frame.text);
    const find = () => {
      const candidates = this.sockets.filter((socket) => socket.url === frame.url &&
        (!socket.closed || frame.type === 'close') && (socket.opened || ['open', 'close'].includes(frame.type)) &&
        (frame.connection === undefined || socket.connection === frame.connection));
      if (stream) return candidates.find((socket) => socket.opened && [...socket.subscriptions].some((topic) => stream === topic || stream.startsWith(`${topic}.`)));
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
      const label = `socket ${frame.url} ${stream ?? 'open'}`;
      return await (this.replayClock?.active ? this.replayClock.wait(pending, label, this.timeoutMs) : withTimeout(pending, label, this.timeoutMs));
    } finally { this.waiters.delete(waiter); }
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
  async play(onTick, ready = async () => {}, runCommand = async () => {}, clock, begin = () => {}) {
    let initialized;
    let startupReady = false;
    const ensureReady = () => initialized ??= withTimeout((async () => {
      await ready();
      while (this.audit && !this.audit.ready()) await new Promise((r) => realTimeout(r, 5));
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
    const inlineStartup = this.venue.rust === 'bitunix' || recordedLifecycle;
    if (!inlineStartup) await ensureReady();
    else {
      // Explicit opens can be prerequisites of readiness. Keep startup timers
      // live while consuming that prelude, then move both venues to tape time.
      ensureReady().catch(() => {}); // The same promise is awaited below.
    }
    begin();
    this.replayClock = clock;
    if (!inlineStartup) clock?.start();
    this.peakBytes = Math.max(this.peakBytes ?? 0, this.sampleMemory?.() ?? 0);
    const start = performance.now();
    let delivered = 0;
    const warm = Math.floor(this.frames.length / 4);
    for (const frame of this.frames) {
      if (delivered === warm) this.warmMark = { thread: process.threadCpuUsage(), wall: performance.now() };
      if (this.isPrivate) {
        const wait = frame.t - (performance.now() - start);
        if (wait > 0) await new Promise((resolve) => realTimeout(resolve, wait));
      }
      this.time = frame.t;
      if (clock && !clock.active && startupReady) clock.start(frame.t);
      await clock?.advance(frame.t);
      if (frame.type === 'command') {
        await ensureReady();
        this.results.push(await this.executeCommand(frame, runCommand).then((ids) => ({ ok: ids }), (error) => {
          if (String(error?.message).startsWith('replay timeout:')) throw error;
          return { error: String(error?.message ?? error) };
        }));
      } else {
        const socket = await this.socketFor(frame);
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
          socket.onMessage(frame.text);
        }
      }
      delivered++;
      if (delivered % 64 === 0) this.peakBytes = Math.max(this.peakBytes, this.sampleMemory?.() ?? 0);
      onTick?.(frame.t);
      await new Promise(setImmediate);
    }
    await ensureReady();
    if (clock && !clock.active) clock.start(this.time);
    await this.finishResponses();
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
