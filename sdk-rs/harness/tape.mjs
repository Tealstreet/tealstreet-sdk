// Shared tape transport: matching is strict except documented generated fields.
import { setTimeout as realTimeout } from 'node:timers';
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

/** Serves recorded HTTP responses and websocket frames in tape order. */
export class Tape {
  constructor(records, { isPrivate = false, canonicalize = canonicalUrl, responses,
    prepareFrame = (frame) => frame, explicitOpens = false, legacyLatest = true } = {}) {
    this.records = records;
    this.isPrivate = isPrivate;
    this.canonicalize = canonicalize;
    this.explicitOpens = explicitOpens;
    this.legacyLatest = legacyLatest;
    this.responses = responses ?? new Map();
    if (!responses) for (const record of records.filter((r) => r.type === 'http')) {
      const key = `${record.method} ${canonicalize(record.url)}`;
      if (!this.responses.has(key)) this.responses.set(key, []);
      this.responses.get(key).push(record);
    }
    // Share immutable canonical URL strings across every frame and both samples.
    const urls = new Map();
    this.frames = records.filter((r) => ['ws', 'command', 'open', 'close'].includes(r.type)).map((frame) => {
      if (frame.url) {
        if (!urls.has(frame.url)) urls.set(frame.url, canonicalize(frame.url));
        frame = { ...frame, url: urls.get(frame.url), connection: frame.connection ?? frame.socket };
      }
      return prepareFrame(frame);
    });
    // Legacy captures omit connect anchors. Only the open cohort before the
    // first close may bootstrap readiness; later opens still drive reconnects.
    this.socketLifecycle = new Map();
    for (const frame of this.frames) {
      if (!['open', 'close'].includes(frame.type)) continue;
      let lifecycle = this.socketLifecycle.get(frame.url);
      if (!lifecycle) {
        lifecycle = { hasOpen: false, initial: new Set(), pastInitial: false };
        this.socketLifecycle.set(frame.url, lifecycle);
      }
      if (frame.type === 'close') lifecycle.pastInitial = true;
      else {
        lifecycle.hasOpen = true;
        if (!lifecycle.pastInitial) lifecycle.initial.add(frame.connection ?? lifecycle.initial.size);
      }
    }
    this.sockets = [];
    this.open = new Map();
    this.connectionCounts = new Map();
    this.byConnection = new Map();
    this._unserved = 0;
  }
  get unserved() { return this._unserved; }

  get responseTime() {
    return this.replayClock?.active ? this.replayClock.time - this.replayClock.epoch : this.time ?? 0;
  }

  respond(method, url, body) {
    if (typeof this.responses.respond === 'function') return this.responses.respond(method, url, body, this.commandIndex ?? 0, this.responseTime);
    const queue = this.responses.get(`${method} ${canonicalUrl(url)}`);
    const matches = queue?.filter(
      (r) => r.requestBody === undefined || canonicalBody(r.requestBody) === canonicalBody(body),
    );
    if (!matches?.length) {
      this._unserved++;
      console.error(`unserved: ${method} ${url}${body === undefined ? '' : ` body=${body}`}`);
      return { status: 599, body: '' };
    }
    const record = matches[0];
    // Only reads may repeat their final snapshot. A second write needs its
    // own response, so an accidental mutation retry fails the gate.
    if (method !== 'GET' || matches.length > 1) queue.splice(queue.indexOf(record), 1);
    return { status: record.status, headers: record.headers ?? '', body: record.body };
  }

  async waitForResponse(response) {
    // A recorded completion follows any depth frames received while its request
    // was in flight. Startup and private replay keep their existing real timing.
    if (!this.isPrivate && this.replayClock?.active && response.recordedAt > this.responseTime) {
      await new Promise((resolve) => setTimeout(resolve, response.recordedAt - this.responseTime));
    }
    return response;
  }

  connect(url, onOpen, onMessage, onClose) {
    const rawUrl = url;
    url = this.canonicalize(url);
    const connection = this.connectionCounts.get(url) ?? 0;
    this.connectionCounts.set(url, connection + 1);
    const socket = { url, rawUrl, connection, onOpen, onMessage, onClose, closed: false, opened: false, topics: new Set(), publicTopics: new Set(), subscriptionArgs: new Map(), loginVerified: false };
    this.sockets.push(socket);
    this.byConnection.set(`${url}:${connection}`, socket);
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
  }

  close(socket, code = 1000, local = false) {
    if (socket.closed) return;
    socket.closed = true;
    if (!local) socket.onClose?.(code);
  }

  async socketFor(frame, ordinal) {
    if (typeof frame === 'string') frame = { url: this.canonicalize(frame), connection: ordinal };
    else if (frame.connection === undefined && frame.socket !== undefined) frame = { ...frame, connection: frame.socket };
    const deadline = performance.now() + 10_000;
    for (;;) {
      const exact = frame.connection !== undefined && this.byConnection.get(`${frame.url}:${frame.connection}`);
      if (exact && (!exact.closed || frame.type === 'close')) return exact;
      if (performance.now() > deadline) throw new Error(`missing tape socket ${frame.url} connection=${frame.connection}; active=${this.sockets.map((s) => `${s.url}:${s.connection}:${s.closed}`).join()}`);
      const candidates = this.sockets.filter((s) => s.url === frame.url && (!s.closed || frame.type === 'close') &&
        (frame.connection === undefined || s.connection === frame.connection));
      if (frame.connection !== undefined || candidates.length === 1) {
        if (candidates[0]) return candidates[0];
      } else if (this.legacyLatest && candidates.length > 1) return candidates.at(-1);
      else if (candidates.length > 1 && frame.text) {
        const message = JSON.parse(frame.text);
        const topic = frame.topic ?? JSON.stringify(message.ch ? { ch: message.ch, symbol: message.symbol } : { channel: message.channel });
        const matched = candidates.filter((s) => s.topics.has(topic));
        if (matched.length === 1) return matched[0];
        throw new Error(`ambiguous legacy tape socket ${frame.url} ${topic}; record connection IDs`);
      }
      await new Promise((r) => realTimeout(r, 1));
    }
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
    await ready();
    begin();
    this.replayClock = clock;
    clock?.start();
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
        this.results.push(
          await runCommand(frame).then(
            (ids) => ({ ok: ids }),
            (error) => ({ error: String(error?.message ?? error) }),
          ),
        );
      } else {
        const socket = await this.socketFor(frame);
        if (frame.type === 'open' || frame.type === 'close') await this.lifecycle(frame);
        else if (frame.type === 'ws') socket.onMessage(frame.text);
      }
      delivered++;
      onTick?.(frame.t);
      await new Promise(setImmediate);
    }
    return delivered;
  }

  results = [];
}
