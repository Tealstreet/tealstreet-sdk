// Compiled venue frames must preserve every DXtrade lot in the product Store.
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import test from 'node:test';
import { EngineHost } from '../hosts/wasm/js/host.mjs';
import { prepareRecordedHttp } from './recorded-http.mjs';
import type { HttpResponse, Transport } from '../hosts/wasm/js/host.mjs';
import { Frame } from '../hosts/wasm/js/wire.mjs';
import { FrameApplier } from '../../packages/safe-cex/src/store/frameApplier.ts';
import { DefaultStore } from '../../packages/safe-cex/src/store/store.base.ts';

const require = createRequire(import.meta.url);
const { WasmEngine } = require('../hosts/wasm/pkg/node/safe_cex_wasm.js');
const encoder = new TextEncoder();
const account = 'default:00002';
const session = 'documented-replay-session';
const login = { sessionToken: session, timeout: '00:30:00' };
const user = { userDetails: [{ login: 'documented-user', domain: 'default', accounts: [{ account, accountStatus: 'FULL_TRADING' }] }] };
const instruments = { instruments: [{ symbol: 'ETHUSD', priceIncrement: 0.01, lotSize: 1 }] };
const details = { instrumentDetails: [{ account, symbol: 'ETHUSD', tradingStatus: 'FULL', minOrderSize: 0.01,
  minOrderSizeIncrement: 0.01, maxOrderSize: { value: 724, limitType: 'LOTS' },
  marginRate: { rateType: 'FLAT', tiers: [{ endVolume: 'Infinity', value: 0.2 }] } }] };
const quotes = { events: [{ symbol: 'ETHUSD', type: 'Quote', ask: 3656.02, bid: 3656.01, time: '2024-12-11T07:55:11Z' }] };
const portfolio = { portfolios: [{ account, version: 86,
  balances: [{ account, version: 86, currency: 'USD', value: 4999.96 }], orders: [],
  positions: [
    { account, version: 85, positionCode: '76515503', symbol: 'ETHUSD', side: 'BUY', quantity: 0.01,
      quantityNotional: 0.01, openPrice: 3656.01, marginRate: 0.2, lastUpdateTime: '2024-12-11T06:00:49.087Z' },
    { account, version: 82, positionCode: '76515542', symbol: 'ETHUSD', side: 'BUY', quantity: 0.01,
      quantityNotional: 0.01, openPrice: 3660, marginRate: 0.2, lastUpdateTime: '2024-12-11T05:56:26.400Z' },
  ],
}] };

type Request = Parameters<Transport['request']>[0];
type Timer = { at: number; fn: () => void };
const response = (value: unknown, status = 200): HttpResponse => ({ status, body: encoder.encode(JSON.stringify(value)) });
const settle = async () => { for (let i = 0; i < 10; i++) await new Promise(setImmediate); };

function rig(reply?: (request: Request) => Promise<HttpResponse> | HttpResponse, options: Record<string, unknown> = {}) {
  let now = Date.UTC(2024, 11, 11, 9);
  const timers = new Set<Timer>();
  const requests: Request[] = [];
  const logs: string[] = [];
  let deactivated = 0;
  let frames = 0;
  const store = new DefaultStore();
  const applier = new FrameApplier(store);
  const engine = new WasmEngine('breakoutprop', JSON.stringify({ key: 'documented-user', secret: 'documented-password', ...options }));
  const host = new EngineHost(engine, {
    now: () => now,
    connect() { assert.fail('BreakoutProp must never open an exchange socket'); },
    // Login and reads are managed: canned replies need an immediate route.
    prepareHttp: prepareRecordedHttp,
    async request(request) {
      requests.push(request);
      if (reply) return reply(request);
      const path = new URL(request.url).pathname;
      if (path.endsWith('/login')) {
        assert.equal(request.method, 'POST');
        assert.deepEqual(JSON.parse(request.body!), { domain: 'default', username: 'documented-user', password: 'documented-password' });
        return response(login);
      }
      assert.equal(request.headers.Authorization, `DXAPI ${session}`);
      if (path.endsWith('/users/documented-user')) return response(user);
      if (path.endsWith(`/accounts/${encodeURIComponent(account)}/instruments/query`)) return response(details);
      if (path.endsWith('/instruments/query')) return response(instruments);
      if (path.endsWith('/marketdata')) {
        assert.deepEqual(JSON.parse(request.body!), { eventTypes: [{ type: 'Quote' }], symbols: ['ETHUSD'], account });
        return response(quotes);
      }
      if (path.endsWith(`/accounts/${encodeURIComponent(account)}/portfolio`)) return response(portfolio);
      assert.fail(`Unexpected documented replay request: ${request.method} ${path}`);
    },
    setTimeout(fn, ms) { const timer = { fn, at: now + ms }; timers.add(timer); return timer; },
    clearTimeout(timer) { timers.delete(timer as Timer); },
  }, {
    onFrame(buffer) { frames++; applier.apply(new Frame(buffer)); },
    onLog(_level, message) { logs.push(message); },
    onDeactivate() { deactivated++; },
  });
  return {
    host, store, requests, logs, timers,
    get now() { return now; },
    get deactivated() { return deactivated; },
    get frames() { return frames; },
    async until(predicate: () => boolean) {
      for (let i = 0; i < 100; i++) {
        await settle();
        host.flush();
        if (predicate()) return;
        const next = [...timers].sort((a, b) => a.at - b.at)[0];
        assert.ok(next, `No pending timer before documented replay completed: ${logs.join('; ')}`);
        timers.delete(next);
        now = Math.max(now, next.at);
        next.fn();
      }
      assert.fail('Documented replay exceeded 100 deterministic timer steps');
    },
    dispose() { host.dispose(); engine.free(); },
  };
}

test('compiled DXtrade lots survive FrameApplier and DefaultStore with the wrapper empty ticker mask', async () => {
  const r = rig();
  try {
    r.host.start();
    await r.host.command({ op: 'setTickerSymbols', symbols: [] });
    await r.until(() => r.store.loaded.balance && r.requests.filter((request) => new URL(request.url).pathname.endsWith('/marketdata')).length >= 2);
    assert.ok(r.store.loaded.markets && r.store.loaded.tickers && r.store.loaded.positions && r.store.loaded.orders);
    assert.deepEqual(r.store.positions.map((position) => position.id).sort(), ['76515503', '76515542']);
    assert.equal(r.store.positions.length, 2);
    assert.equal(r.store.positions.reduce((sum, position) => sum + position.contracts, 0), 0.02);
    const upnl = r.store.positions.reduce((sum, position) => sum + position.unrealizedPnl, 0);
    const margin = r.store.positions.reduce((sum, position) => sum + (position.marginSize ?? 0), 0);
    assert.ok(Math.abs(upnl - (3656.01 - 3660) * 0.01) < 1e-10);
    assert.ok(Math.abs(r.store.balance.upnl - upnl) < 1e-10);
    assert.ok(Math.abs(r.store.balance.used - margin) < 1e-10);
    assert.ok(Math.abs(r.store.balance.total - (4999.96 + upnl)) < 1e-10);
    assert.ok(Math.abs(r.store.balance.free - (4999.96 + upnl - margin)) < 1e-10);
    assert.deepEqual(r.logs, []);
  } finally { r.dispose(); }
});

test('disposing a compiled venue aborts pending auth and ignores its late successful response', async () => {
  let release!: (value: HttpResponse) => void;
  const pending = new Promise<HttpResponse>((resolve) => { release = resolve; });
  const r = rig(() => pending);
  try {
    r.host.start();
    const candle = r.host.command({ op: 'fetchCandles', symbol: 'ETHUSD', interval: '1h' });
    const rejected = assert.rejects(candle, /engine disposed/);
    await settle();
    assert.equal(r.requests.length, 1);
    r.host.dispose();
    await rejected;
    assert.equal(r.requests[0].signal?.aborted, true);
    const frames = r.frames;
    release(response(login));
    await settle();
    r.host.flush();
    assert.equal(r.requests.length, 1);
    assert.equal(r.frames, frames);
    assert.equal(r.timers.size, 0);
    assert.equal(r.store.markets.length, 0);
  } finally { release(response(login)); r.dispose(); }
});

test('compiled definitive credential rejection deactivates instead of leaving startup pending', async () => {
  const r = rig(() => response({ message: 'Invalid credentials' }, 401));
  try {
    r.host.start();
    await r.until(() => r.deactivated > 0);
    assert.equal(r.deactivated, 1);
    assert.equal(r.requests.length, 1);
    assert.equal(r.timers.size, 0);
    assert.equal(r.store.loaded.markets, false);
  } finally { r.dispose(); }
});

test('compiled polling controls pause and resume through the fire-and-forget host command', { timeout: 5000 }, async () => {
  const r = rig();
  try {
    r.host.start();
    await r.until(() => r.store.loaded.balance && r.requests.filter((request) => new URL(request.url).pathname.endsWith('/marketdata')).length >= 2);
    await assert.rejects(r.host.command({ op: 'setPollScale' }), /scale/);
    assert.ok(r.timers.size > 0);
    await r.host.command({ op: 'setPollScale', scale: null });
    assert.equal(r.timers.size, 0);
    const before = r.requests.length;
    await settle();
    assert.equal(r.requests.length, before);
    await r.host.command({ op: 'setPollScale', scale: 5 });
    assert.equal(r.timers.size, 1);
    await r.until(() => r.requests.length > before);
    assert.ok(new URL(r.requests[before].url).pathname.endsWith('/portfolio'));
    assert.equal(r.deactivated, 0);
  } finally { r.dispose(); }
});


for (const [tickInterval, expected] of [[0.00001, 1_000], [1e300, 120_000]] as const) {
  test(`compiled initial polling interval ${tickInterval} obeys the request-pressure bounds`, async () => {
    const r = rig(undefined, { tickInterval });
    try {
      r.host.start();
      await r.until(() => r.store.loaded.balance);
      assert.equal(r.timers.size, 1);
      const next = [...r.timers][0];
      assert.equal(next.at - r.now, expected);
      const before = r.requests.length;
      await r.until(() => r.requests.length > before);
      assert.ok(new URL(r.requests[before].url).pathname.endsWith('/portfolio'));
      assert.deepEqual(r.logs, []);
    } finally { r.dispose(); }
  });
}

test('compiled pollingEnabled false pauses even an initial flood interval after account bootstrap', async () => {
  const r = rig(undefined, { tickInterval: 0.00001, pollingEnabled: false });
  try {
    r.host.start();
    await r.until(() => r.store.loaded.balance);
    assert.equal(r.timers.size, 0);
    const before = r.requests.length;
    await settle();
    assert.equal(r.requests.length, before);
    assert.deepEqual(r.logs, []);
  } finally { r.dispose(); }
});
