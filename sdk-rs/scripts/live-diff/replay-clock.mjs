import http from 'node:http';
import https from 'node:https';
import { setTimeout as realTimeout, clearTimeout as realClear, setImmediate as turn } from 'node:timers';

// A child-process host adapter, installed before constructing the real exchange.
// Startup keeps real scheduling with fixed clock inputs. At begin, outstanding
// timers retain their original logical deadlines; replay drives every deadline.
export function replayClock(epoch) {
  const NativeDate = Date;
  let at = 0,
    active = false,
    sequence = 0;
  const timers = new Map();
  globalThis.Date = class extends NativeDate {
    constructor(...args) {
      super(...(args.length ? args : [Math.floor(epoch + at)]));
    }
    static now() {
      return Math.floor(epoch + at);
    }
  };
  Object.defineProperty(globalThis.performance, 'now', { configurable: true, value: () => at });
  const clear = (handle) => {
    const id = Number(handle),
      timer = timers.get(id);
    if (timer) realClear(timer.real);
    timers.delete(id);
  };
  function schedule(callback, delay, interval, args) {
    const id = ++sequence;
    if (typeof callback !== 'function') throw new TypeError('Timer callback must be a function');
    const requested = Number(delay);
    const ms = requested >= 1 && requested <= 2147483647 ? Math.trunc(requested) : 1;
    const timer = { id, at: at + ms, callback, args, interval: interval ? ms : 0 };
    const handle = {
      [Symbol.toPrimitive]: () => id,
      ref() {
        timer.real?.ref();
        return this;
      },
      unref() {
        timer.real?.unref();
        return this;
      },
      hasRef() {
        return timer.real?.hasRef() ?? true;
      },
      refresh() {
        timer.at = at + ms;
        return this;
      },
    };
    timers.set(id, timer);
    const startup = () => {
      if (active || !timers.has(id)) return;
      if (!interval) timers.delete(id);
      else timer.real = realTimeout(startup, ms);
      callback(...args);
    };
    if (!active) timer.real = realTimeout(startup, ms);
    return handle;
  }
  globalThis.setTimeout = (callback, delay, ...args) => schedule(callback, delay, false, args);
  globalThis.setInterval = (callback, delay, ...args) => schedule(callback, delay, true, args);
  globalThis.clearTimeout = globalThis.clearInterval = clear;
  return {
    begin() {
      active = true;
      for (const timer of timers.values()) realClear(timer.real);
    },
    async advance(target) {
      if (!active || !Number.isFinite(target) || target < at)
        throw new Error('Replay time must be finite and monotonic');
      let turns = 0;
      while (true) {
        const next = [...timers.values()]
          .filter((timer) => timer.at <= target)
          .sort((a, b) => a.at - b.at || a.id - b.id)[0];
        if (!next) break;
        if (++turns > 100000) throw new Error('Replay timer loop did not quiesce');
        at = Math.max(at, next.at);
        if (next.interval) next.at = at + next.interval;
        else timers.delete(next.id);
        next.callback(...next.args);
        await new Promise(turn);
      }
      at = target;
      await new Promise(turn);
    },
    now: () => at,
  };
}

// Native global WebSocket dispatches listeners synchronously. The microtask
// after dispatch provides an independent TS completion receipt for that socket.
export function socketReceipts() {
  const NativeSocket = globalThis.WebSocket,
    receipts = [];
  globalThis.WebSocket = class extends NativeSocket {
    constructor(url, ...args) {
      super(url, ...args);
      const receipt = { url: String(url), consumed: 0, processed: 0, errors: 0, closeConsumed: 0, closeProcessed: 0 };
      receipts.push(receipt);
      super.addEventListener('close', () => {
        receipt.closeConsumed++;
        queueMicrotask(() => receipt.closeProcessed++);
      });
      super.addEventListener('message', () => {
        receipt.consumed++;
        queueMicrotask(() => receipt.processed++);
      });
    }
  };
  return () => structuredClone(receipts);
}

// Axios uses these actual terminal protocols. Hold the replay clock through the
// response's Promise turn so frame-triggered book snapshots cannot race ahead
// into the next frame merely because loopback HTTP completed at a different speed.
export function httpReceipts() {
  let pending = 0;
  for (const protocol of [http, https]) {
    const request = protocol.request;
    protocol.request = function (...args) {
      pending++;
      let finished = false,
        responded = false;
      const finish = () => {
        if (!finished) {
          finished = true;
          pending--;
        }
      };
      let outgoing;
      try {
        outgoing = request.apply(this, args);
      } catch (error) {
        finish();
        throw error;
      }
      outgoing.once('response', (response) => {
        responded = true;
        response.once('end', () => turn(finish));
        response.once('close', () => turn(finish));
      });
      outgoing.once('error', () => turn(finish));
      outgoing.once('upgrade', () => turn(finish));
      outgoing.once('close', () => {
        if (!responded) turn(finish);
      });
      return outgoing;
    };
  }
  return () => pending;
}
