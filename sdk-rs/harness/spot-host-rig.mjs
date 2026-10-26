import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { EngineHost } from '../hosts/wasm/js/host.mjs';
import { Frame } from '../hosts/wasm/js/wire.mjs';
import { prepareRecordedHttp } from './recorded-http.mjs';
const require = createRequire(import.meta.url);
const { WasmEngine } = require('../hosts/wasm/pkg/node/safe_cex_wasm.js');
export const spotResponse = (body, status = 200) => ({ status, body: new TextEncoder().encode(JSON.stringify(body)) });
export const spotSettle = async () => { for (let i = 0; i < 10; i++) await new Promise(setImmediate); };
export function spotRig(venue, options, reply) {
  let now = 1792454400000;
  const requests = [], sockets = [], timers = new Set(), frames = [], logs = [];
  const engine = new WasmEngine(venue, JSON.stringify(options));
  const host = new EngineHost(engine, {
    now: () => now,
    prepareHttp: prepareRecordedHttp,
    async request(request) { requests.push(request); return reply(request); },
    connect(url, handlers) {
      const socket = { url, handlers, sent: [], closed: false };
      sockets.push(socket);
      setImmediate(() => { if (!socket.closed) handlers.onOpen(); });
      return { send(text) { socket.sent.push(JSON.parse(text)); }, close() { socket.closed = true; } };
    },
    setTimeout(fn, ms) { const timer = { fn, at: now + ms }; timers.add(timer); return timer; },
    clearTimeout(timer) { timers.delete(timer); },
  }, {
    onFrame(buffer) { frames.push(new Frame(buffer)); },
    onLog(_level, message) { logs.push(message); },
  });
  return {
    host, requests, sockets, frames, logs,
    async start() { host.start(); await spotSettle(); host.flush(); },
    async send(socket, value) { socket.handlers.onMessage(JSON.stringify(value)); await spotSettle(); host.flush(); },
    async advance(ms) {
      const target = now + ms;
      for (;;) {
        const timer = [...timers].filter((t) => t.at <= target).sort((a,b) => a.at-b.at)[0];
        if (!timer) break;
        timers.delete(timer); now = timer.at; timer.fn(); await spotSettle();
      }
      now = target; host.flush();
    },
    async command(command) {
      let done = false;
      const pending = host.command(command);
      pending.then(() => { done = true; }, () => { done = true; });
      // Native write pacing is driven by engine timers, including queued cancels.
      // Pump the same deterministic clock while awaiting a command ACK.
      for (let step = 0; !done && step < 100; step++) {
        await spotSettle();
        if (done) break;
        const timer = [...timers].sort((a,b) => a.at-b.at)[0];
        assert(timer, `No timer can settle ${command.op}: ${logs.join('; ')}`);
        timers.delete(timer); now = Math.max(now, timer.at); timer.fn();
      }
      assert(done, `Command ${command.op} exceeded deterministic timer budget`);
      const result = await pending; host.flush(); return result;
    },
    assertLoaded() {
      assert(frames.some((frame) => frame.markets()?.count > 0), logs.join('; '));
      assert(frames.some((frame) => (frame.status()?.loaded & 31) === 31), logs.join('; '));
    },
    dispose() { host.dispose(); engine.free(); },
  };
}
