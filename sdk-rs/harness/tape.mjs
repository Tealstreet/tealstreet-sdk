// Shared tape transport: matching is strict except documented generated fields.
import { setTimeout as realTimeout, clearTimeout as realClearTimeout } from 'node:timers';
import { LOADED_FLAG } from '../hosts/wasm/js/wire.mjs';

export const replayStoreReady = (loaded, isPrivate) => {
  const publicMask = LOADED_FLAG.MARKETS | LOADED_FLAG.TICKERS;
  const required = isPrivate ? publicMask | LOADED_FLAG.BALANCE | LOADED_FLAG.ORDERS | LOADED_FLAG.POSITIONS : publicMask;
  return (loaded & required) === required;
};
/** Per-request values: clocks, signatures and generated client ids. */
const VOLATILE = new Set(['timestamp', 'recvWindow', 'signature', 'newClientOrderId', 'clientAlgoId', 'requestId']);
export const canonicalUrl = (url) => {
  const parsed = new URL(url);
  // Active trigger lists use a rolling lookback ending at request time.
  // Keep its width exact while allowing the two implementations' clock skew.
  if (
    /\/private\/(planorder|stoporder)\/list\/orders$/.test(parsed.pathname) &&
    parsed.searchParams.has('start_time') &&
    parsed.searchParams.has('end_time')
  ) {
    const end = Number(parsed.searchParams.get('end_time'));
    parsed.searchParams.set('start_time', String(Number(parsed.searchParams.get('start_time')) - end));
    parsed.searchParams.set('end_time', 'now');
  }
  const query = [...parsed.searchParams]
    .filter(([name]) => !VOLATILE.has(name))
    .map(([name, value]) => [name, value.replace(/"(newClientOrderId|clientAlgoId)":"[^"]*",?/g, '')])
    .sort(([a], [b]) => a.localeCompare(b));
  return `${parsed.origin}${parsed.pathname}?${new URLSearchParams(query)}`;
};

// JSON object order is immaterial; snowflake ids must never pass through f64.
export const canonicalBody = (body) => {
  if (body === undefined || body === null || body === '') return '';
  if (body instanceof Uint8Array) body = new TextDecoder().decode(body);
  let parsed = body;
  if (typeof body === 'string') {
    try {
      parsed = JSON.parse(body, (_key, value, context) =>
        typeof value === 'number' && Number.isInteger(value) && !Number.isSafeInteger(value) ? context.source : value,
      );
    } catch {
      return body;
    }
  }
  const sorted = (value) =>
    Array.isArray(value)
      ? value.map(sorted)
      : value && typeof value === 'object'
        ? Object.fromEntries(
            Object.keys(value)
              .filter((key) => key !== 'externalOid' && key !== 'requestId')
              .sort()
              .map((key) => [key, sorted(value[key])]),
          )
        : value;
  return JSON.stringify(sorted(parsed));
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
    const target = this.epoch + t;
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
  restore() {
    for (const timer of this.timers.values()) this.original.clearTimeout(timer.handle);
    this.timers.clear();
    Date.now = this.original.now;
    for (const name of ['setTimeout', 'clearTimeout', 'setInterval', 'clearInterval'])
      globalThis[name] = this.original[name];
  }
}


import { createHmac, timingSafeEqual } from 'node:crypto';
import { parseRequestJson } from './request-json.mjs';
import { Responses } from './responses.mjs';
import { requestKey as nativeRequestKey, subscriptionKeys } from './requests.mjs';
export class ReplayTimeout extends Error {}
export const within = async (promise, ms, label) => {
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

export const createTape = (records, venue, isPrivate, { impl, books = [], socketTimeoutMs = 30_000 } = {}) => {
  const KEY = 'replay-key',
    SECRET = 'replay-secret',
    PASSPHRASE = 'replay-passphrase';
  const callerIds = new Set();
  const collectCallerIds = (value) => {
    if (!value || typeof value !== 'object') return;
    if (typeof value.clientOrderId === 'string') callerIds.add(value.clientOrderId);
    for (const nested of Object.values(value)) collectCallerIds(nested);
  };
  for (const record of records) if (record.type === 'command') collectCallerIds(record);
  /** Per-request values: clocks, signatures, request ids and generated client ids. */
  const VOLATILE = new Set(['timestamp', 'recvWindow', 'signature', 'newClientOrderId', 'clientAlgoId', 'requestId']);
  const urlKeys = new Map();
  const bitget = venue.rust.startsWith('bitget');
  const mexc = venue.rust === 'mexc';
  const canonicalUrl = (url) => {
    if (urlKeys.has(url)) return urlKeys.get(url);
    const parsed = new URL(url);
    if (
      mexc && /\/private\/(planorder|stoporder)\/list\/orders$/.test(parsed.pathname) &&
      parsed.searchParams.has('start_time') && parsed.searchParams.has('end_time')
    ) {
      const end = Number(parsed.searchParams.get('end_time'));
      parsed.searchParams.set('start_time', String(Number(parsed.searchParams.get('start_time')) - end));
      parsed.searchParams.set('end_time', 'now');
    }
    const query = [...parsed.searchParams]
      .filter(([name, value]) => !VOLATILE.has(name) || callerIds.has(value))
      .map(([name, value]) => [name, /^[\[{]/.test(value) ? canonicalBody(value, url) : value])
      .sort(([a], [b]) => a.localeCompare(b));
    const key = `${parsed.origin}${parsed.pathname}?${new URLSearchParams(query)}`;
    urlKeys.set(url, key);
    return key;
  };

  const generatedAliases = new Map();
  const mintedField = (key, url) => {
    const path = new URL(url).pathname;
    return key === 'newClientOid'
      ? /\/modify-order$/.test(path)
      : key === 'clientOid' &&
          /\/(?:batch-place-order|place-order|place-tpsl-order|place-plan-order|place-strategy-order)$/.test(path);
  };
  const canonicalBody = (body, url) => {
    const classicBatch = bitget && new URL(url).pathname === '/api/v2/mix/order/batch-place-order';
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
    else if (venue.rust === 'cryptocom' && parsed?.method && parsed?.params) parsed = parsed.params;
    const sorted = (value) => {
      if (mexc && typeof value === 'bigint') return String(value);
      if (Array.isArray(value)) return value.map(sorted);
      if (value === null || typeof value !== 'object') return value;
      const keys = Object.keys(value)
        .filter(
          (key) =>
            value[key] !== undefined &&
            !(mexc && ['externalOid', 'requestId'].includes(key) && !callerIds.has(String(value[key]))) &&
            !(venue.volatileBodyFields && VOLATILE.has(key) && !callerIds.has(String(value[key]))),
        )
        .sort();
      return Object.fromEntries(
        keys.map((key) => [
          key,
          ((bitget && mintedField(key, url) && typeof value[key] === 'string' && value[key].length) ||
            venue.generatedBodyId?.(key, value[key])) &&
          !callerIds.has(value[key])
            ? '<generated>'
            : classicBatch && key === 'marginMode' && value[key] === 'cross'
              ? 'crossed'
              : bitget && key === 'clientOid' && typeof value[key] === 'string'
                ? (generatedAliases.get(value[key]) ?? value[key])
                : sorted(value[key]),
        ]),
      );
    };
    return JSON.stringify(sorted(parsed), (_key, value) =>
      typeof value === 'bigint' ? { integer: String(value) } : value,
    );
  };
  const topicKey = (arg) => `${arg.instType ?? ''}|${arg.channel ?? arg.topic}|${arg.instId ?? arg.symbol ?? ''}`;
  const requestKey = (method, url, body) =>
    ['bingx', 'blofin'].includes(venue.rust)
      ? nativeRequestKey(method, url, body, callerIds, venue)
      : `${method} ${canonicalUrl(url)}${(bitget || venue.matchRequestBody) && method !== 'GET' ? ` ${canonicalBody(body, url)}` : ''}`;

  /** Serves recorded HTTP responses and websocket frames in tape order. */
  class Tape {
    constructor() {
      this.audit = venue.createSocketAudit?.(impl, {
        isPrivate,
        books,
        tickers: venue.publicTickers?.(records),
      });
      this.responses = new Responses(
        records,
        requestKey,
        venue.checkRequest,
        venue.isWrite,
        bitget || Boolean(venue.matchRequestBody || venue.isWrite),
      );
      this.frames = records.filter((r) => ['ws', 'command', 'open', 'close'].includes(r.type));
      this.open = new Map();
      this.waiters = [];
      this.commandIndex = 0;
      this.time = 0;
      this.generated = new Set();
      this.sentRequests = new Set();
      this.subscriptions = new Set();
      this.startupSubscriptions = new Map(
        Array.from(venue.startupSubscriptions?.(records, books) ?? [], (key) => [key, false]),
      );
      this.pendingStartupSubscriptions = this.startupSubscriptions.size;
      this.generations = new Map();
    }

    respond(method, url, body, headers) {
      venue.validateRequest?.({ method, url, body, headers });
      return this.responses.respond(
        method,
        url,
        body,
        this.commandIndex,
        (record) => {
          if (method !== 'GET' && venue.rust.startsWith('bitget') && /(?:place|modify)-/.test(new URL(url).pathname)) {
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
            if (String(reply.code) !== '40009')
              for (const id of ids) {
                if (this.generated.has(id)) throw new Error(`generated client ID reused: ${id}`);
                this.generated.add(id);
              }
          }
          if (bitget && method !== 'GET') {
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
        this.replayClock?.active ? this.replayClock.time - this.replayClock.epoch : this.time,
      );
    }

    connect(url, onOpen, onMessage, onClose) {
      const key = canonicalUrl(url);
      const generation = (this.generations.get(key) ?? 0) + 1;
      this.generations.set(key, generation);
      const socket = { url, generation, ordinal: generation - 1, onMessage, onClose, closed: false, loginVerified: false, topics: new Map() };
      setImmediate(() => {
        if (socket.closed) return;
        // Bybit adds a generated requestId to each connection URL.
        if (!this.open.has(canonicalUrl(url))) this.open.set(canonicalUrl(url), new Set());
        this.open.get(canonicalUrl(url)).add(socket);
        onOpen();
        this.waiters = this.waiters.filter((waiter) => !waiter());
      });
      return socket;
    }

    send(socket, text) {
      if (venue.requestBodies && text !== 'ping' && text !== 'pong') {
        let message;
        try {
          message = JSON.parse(text);
        } catch {
          /* Literal heartbeats carry no request body. */
        }
        const sorted = (value) =>
          Array.isArray(value)
            ? value.map(sorted)
            : value && typeof value === 'object'
              ? Object.fromEntries(Object.entries(value).sort(([a], [b]) => a.localeCompare(b))
                  .map(([key, item]) => [key, sorted(item)]))
              : value;
        if (message?.op === 'subscribe' || message?.op === 'unsubscribe') {
          for (const topic of message.args ?? [])
            this.sentRequests.add(JSON.stringify({ url: socket.url, op: message.op, topic: sorted(topic) }));
        } else if (message?.op === 'login') {
          const args = message.args.map(({ timestamp, nonce, sign, ...credentials }) => sorted(credentials));
          this.sentRequests.add(JSON.stringify({ url: socket.url, op: 'login', args }));
        }
      }
      for (const key of subscriptionKeys(socket.url, text)) {
        this.subscriptions.add(`${socket.generation} ${key}`);
        if (this.startupSubscriptions.get(key) === false) {
          this.startupSubscriptions.set(key, true);
          this.pendingStartupSubscriptions--;
        }
      }
      this.audit?.sent(socket.url, text);
      if (venue.respondToPing && text === 'ping') {
        setImmediate(() => {
          if (!socket.closed) socket.onMessage('pong');
        });
        return;
      }
      // OKX's opt-in auth validation also uses its v5 subscription argument model.
      const validateTopics = bitget || (venue.validateSubscriptions ?? venue.validateLogin);
      if (!venue.validateLogin && !validateTopics) return;
      if (text === 'ping') return;
      const message = JSON.parse(text);
      if (validateTopics && ['subscribe', 'unsubscribe'].includes(message.op)) {
        if (!Array.isArray(message.args)) throw new Error(`Invalid subscription on ${socket.url}`);
        for (const topic of message.args) {
          if (!topic || typeof (topic.channel ?? topic.topic) !== 'string' || !(topic.channel ?? topic.topic))
            throw new Error(`Invalid subscription topic on ${socket.url}`);
          const key = bitget
            ? topicKey(topic)
            : JSON.stringify(Object.fromEntries(Object.entries(topic).sort(([a], [b]) => a.localeCompare(b))));
          if (message.op === 'subscribe') socket.topics.set(key, topic);
          else socket.topics.delete(key);
        }
        return;
      }
      if (!venue.validateLogin || !/\/(private|business)$/.test(new URL(socket.url).pathname)) return;
      if (message.op !== 'login') return;
      const arg = message.args?.length === 1 ? message.args[0] : undefined;
      const timestamp = String(arg?.timestamp ?? '');
      if (
        arg?.apiKey !== KEY ||
        arg?.passphrase !== PASSPHRASE ||
        !/^\d+$/.test(timestamp) ||
        !Number.isSafeInteger(Number(timestamp)) ||
        Math.abs(Number(timestamp) - Date.now() / 1000) > 30
      ) {
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
      if (
        !(venue.validateSubscriptions ?? venue.validateLogin) ||
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
      const missing = this.responses.unusedWrites();
      if (missing) throw new Error(`${missing} taped write(s) were not sent`);
    }

    close(socket, code = 1000) {
      if (socket.closed) return;
      socket.closed = true;
      socket.onClose?.(code);
    }

    sockets(url, ordinal) {
      return [...(this.open.get(canonicalUrl(url)) ?? [])].filter(
        (socket) => !socket.closed && (ordinal === undefined || socket.ordinal === ordinal),
      );
    }

    sent(socket, text) {
      this.send(socket, text);
    }

    async socketFor(url, ordinal) {
      while (!this.sockets(url, ordinal).length) {
        let waiter;
        const opened = new Promise((resolve) => {
          waiter = () => (this.sockets(url, ordinal).length ? (resolve(), true) : false);
          this.waiters.push(waiter);
        });
        try {
          await within(opened, socketTimeoutMs, `socket did not open: ${canonicalUrl(url)} #${ordinal ?? 'latest'}`);
        } finally {
          this.waiters = this.waiters.filter((queued) => queued !== waiter);
        }
      }
      return this.sockets(url, ordinal).at(-1);
    }

    async deliver(frame) {
      const latest = await this.socketFor(frame.url, frame.socket);
      const sockets = bitget && frame.socket === undefined ? this.sockets(frame.url) : [latest];
      if (frame.type === 'close') {
        for (const socket of sockets) this.close(socket, frame.code ?? 1006);
        return;
      }
      // connect() already fired onOpen. Metadata only waits for its exact socket.
      if (frame.type === 'open') return;
      let arg;
      try {
        arg = JSON.parse(frame.text)?.arg;
      } catch {
        /* literal heartbeat */
      }
      const key = bitget && arg && topicKey(arg);
      // Bitget has three sockets at the same URL; recorded frames belong to
      // their subscribed topics, not whichever socket connected last.
      const matching = key ? sockets.filter((socket) => socket.topics.has(key)) : sockets;
      if (key && !matching.length) throw new Error(`frame has no subscribed socket: ${frame.url} ${key}`);
      this.audit?.received(frame.url, frame.text);
      for (const socket of matching) {
        if ((venue.validateLogin || venue.validateSubscriptions) && frame.text.trimStart().startsWith('{')) {
          const message = JSON.parse(frame.text);
          if (venue.validateLogin && message.event === 'login' && String(message.code) === '0' && !socket.loginVerified)
            throw new Error(`Taped login success preceded a valid login on ${frame.url}`);
          this.assertSubscribed(socket, message);
        }
        socket.onMessage(frame.text);
      }
    }

    /**
     * Deliver every frame in order, yielding to the event loop between frames.
     * Commands are awaited where they fall. A private tape is paced: each record
     * waits until its time after the account is ready.
     */
    async play(onTick, ready = async () => {}, runCommand = async () => {}, clock, begin = () => {}) {
      await within(
        (async () => {
          await ready();
          while (this.pendingStartupSubscriptions) await new Promise((resolve) => setTimeout(resolve, 1));
        })(),
        15_000,
        'Replay startup',
      );
      begin();
      clock?.start();
      this.replayClock = clock;
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
        await clock?.advance(frame.t);
        if (frame.type === 'command') {
          this.commandIndex++;
          this.results.push(
            await within(runCommand(frame), 15_000, `Command ${frame.op}`).then(
              (ids) => ({ ok: ids }),
              (error) => {
                if (error instanceof ReplayTimeout) throw error;
                return { error: String(error?.message ?? error) };
              },
            ),
          );
        } else {
          await this.deliver(frame);
        }
        delivered++;
        onTick?.(frame.t);
        await new Promise(setImmediate);
      }
      return delivered;
    }

    get unusedWrites() {
      return this.responses.unusedWrites();
    }

    get unserved() {
      return this.responses.unserved;
    }

    results = [];
  }

  return new Tape();
};
