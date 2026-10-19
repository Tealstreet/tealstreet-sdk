// Shared tape transport: matching is strict except documented generated fields.
import { setTimeout as realTimeout } from 'node:timers';
import { LOADED_FLAG } from '../hosts/wasm/js/wire.mjs';

export const replayStoreReady = (loaded, isPrivate) => {
  const publicMask = LOADED_FLAG.MARKETS | LOADED_FLAG.TICKERS;
  const required = isPrivate ? publicMask | LOADED_FLAG.BALANCE | LOADED_FLAG.ORDERS | LOADED_FLAG.POSITIONS : publicMask;
  return (loaded & required) === required;
};
import { canonicalUrl as strictUrl, requestKey, callerIdsOf } from './requests.mjs';
import { Responses } from './responses.mjs';
import { VENUES } from './venues.mjs';
export const canonicalUrl = (url) => strictUrl(VENUES.mexc.requestUrl(url));
export const canonicalBody = (body, callerIds = new Set()) => {
  const url = 'https://api.mexc.com/api/v1/private/order/cancel';
  const prefix = requestKey('POST', url, undefined, callerIds, VENUES.mexc);
  return requestKey('POST', url, body, callerIds, VENUES.mexc).slice(prefix.length + 1);
};

// Run inbound protocol checks before handing an actual socket its frame.
export const deliverSocketFrame = (socket, text, audit, validate) => {
  audit?.received(socket.url, text);
  validate?.(socket, text);
  socket.onMessage(text);
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

/** Serves recorded HTTP responses and websocket frames in tape order. */
export class Tape {
  constructor(records, { isPrivate = false } = {}) {
    this.isPrivate = isPrivate;
    const callerIds = callerIdsOf(records);
    this.responses = new Responses(records, (method, url, body) =>
      requestKey(method, url, body, callerIds, VENUES.mexc));
    this.frames = records.filter((r) => ['ws', 'command', 'open', 'close'].includes(r.type));
    this.open = new Map();
    this.ordinals = new Map();
    this.commandIndex = 0;
    this.playbackTime = 0;
  }

  respond(method, url, body) {
    return this.responses.respond(method, url, body, this.commandIndex, this.playbackTime);
  }

  get unserved() { return this.responses.unserved; }

  connect(url, onOpen, onMessage, onClose) {
    const key = canonicalUrl(url);
    const ordinal = this.ordinals.get(key) ?? 0;
    this.ordinals.set(key, ordinal + 1);
    const socket = { url, key, ordinal, onMessage, onClose, closed: false };
    setImmediate(() => {
      if (socket.closed) return;
      this.open.set(`${key}\n${ordinal}`, socket);
      onOpen();
    });
    return socket;
  }

  close(socket, code = 1000) {
    if (socket.closed) return;
    socket.closed = true;
    socket.onClose?.(code);
  }

  async socketFor(url, ordinal) {
    const key = canonicalUrl(url);
    // Older tapes lack ordinals; retain their latest-connection behavior.
    const current = () =>
      ordinal === undefined
        ? [...this.open.values()].findLast((socket) => socket.key === key && !socket.closed)
        : this.open.get(`${key}\n${ordinal}`);
    const deadline = performance.now() + 15000;
    while (!current() || current().closed) {
      if (performance.now() > deadline)
        throw new Error(`Tape socket did not open: ${url} #${ordinal}; opened ${[...this.open.keys()].join(', ')}`);
      await new Promise((resolve) => realTimeout(resolve, 5));
    }
    return current();
  }

  /**
   * Deliver every frame in order, yielding to the event loop between frames.
   * Commands are awaited where they fall. A private tape is paced: each record
   * waits until its time after the account is ready.
   */
  async play(onTick, ready = async () => {}, runCommand = async () => {}, clock, begin = () => {}) {
    await ready();
    begin();
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
      this.playbackTime = frame.t;
      await clock?.advance(frame.t);
      if (frame.type === 'command') {
        this.commandIndex++;
        this.results.push(
          await runCommand(frame).then(
            (ids) => ({ ok: ids }),
            (error) => ({ error: String(error?.message ?? error) }),
          ),
        );
      } else {
        const socket = await this.socketFor(frame.url, frame.socket);
        if (frame.type === 'close') this.close(socket, frame.code);
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
