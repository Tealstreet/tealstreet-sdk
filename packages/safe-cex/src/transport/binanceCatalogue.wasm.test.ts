// @vitest-environment node
import type { HttpResponse, Transport } from '@tealstreet/safe-cex-wasm/host';

import { setImmediate } from 'node:timers';

import { forceCloseDatabase, IDBDatabase, IDBFactory } from 'fake-indexeddb';
import { afterEach, expect, it, vi } from 'vitest';

import { EngineHost } from '@tealstreet/safe-cex-wasm/host';
import { Frame } from '@tealstreet/safe-cex-wasm/wire';
import { prepareRecordedHttp } from '../__tests__/support/managedHttp';
import { loadNodeEngine } from '../exchanges/rust/loadNodeEngine';
import { FrameApplier } from '../store/frameApplier';
import { DefaultStore } from '../store/store.base';

// fake-indexeddb declares a constructor parameter, but its runtime takes a database instance.
const closeDatabase = forceCloseDatabase as unknown as (database: IDBDatabase) => void;
// Host identity entropy also drives native retry jitter; each fixture account owns its stream.
const seededEntropy = (seed: number) => (length: number) => Uint8Array.from({ length }, () => seed++ & 255);

const hosts: EngineHost[] = [];
const transports: Transport[] = [];
let databaseTasks = 0;
const settle = async () => {
  let quiet = 0;
  for (let turn = 0; turn < 10_000; turn++) {
    await new Promise<void>((resolve) => setImmediate(resolve));
    quiet = databaseTasks === 0 ? quiet + 1 : 0;
    if (quiet === 2) return;
  }
  throw new Error('IndexedDB tasks did not settle');
};
afterEach(async () => {
  hosts.splice(0).forEach((host) => host.dispose());
  transports.splice(0).forEach((transport) => transport.dispose?.());
  await settle();
  vi.useRealTimers();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

const CATALOGUE = JSON.stringify({
  symbols: [
    {
      symbol: 'BTCUSDT',
      status: 'TRADING',
      baseAsset: 'BTC',
      quoteAsset: 'USDT',
      marginAsset: 'USDT',
      contractType: 'PERPETUAL',
      filters: [
        { filterType: 'PRICE_FILTER', tickSize: '0.10' },
        { filterType: 'LOT_SIZE', stepSize: '0.001', minQty: '0.001', maxQty: '1000' },
        { filterType: 'MARKET_LOT_SIZE', stepSize: '0.001', minQty: '0.001', maxQty: '120' },
      ],
    },
  ],
});
const response = (status = 200, headers = '', body = CATALOGUE): HttpResponse => ({
  status,
  headers,
  body: new TextEncoder().encode(body),
});

async function sixEngines(
  answer: (attempt: number) => Promise<HttpResponse>,
  unavailable = false,
  entropySeed?: number,
) {
  vi.useFakeTimers({ toFake: ['Date', 'setTimeout', 'clearTimeout'] });
  vi.setSystemTime(1_000_000);
  const factory = new IDBFactory();
  const databases = new Set<IDBDatabase>();
  const open = factory.open.bind(factory);
  vi.spyOn(factory, 'open').mockImplementation((...args) => {
    const request = open(...args);
    request.addEventListener('success', () => databases.add(request.result));
    return request;
  });
  if (unavailable)
    vi.spyOn(factory, 'open').mockImplementation(() => {
      throw new DOMException('blocked', 'SecurityError');
    });
  vi.stubGlobal('indexedDB', factory);
  databaseTasks = 0;
  vi.stubGlobal('setImmediate', (callback: (...args: unknown[]) => void, ...args: unknown[]) => {
    databaseTasks++;
    return setImmediate(() => {
      try {
        callback(...args);
      } finally {
        databaseTasks--;
      }
    });
  });
  const { WasmEngine } = await loadNodeEngine();
  const started = Date.now();
  const stores: DefaultStore[] = [];
  const sends: Array<{ account: number; at: number }> = [];
  const preparations: Array<{ account: number; at: number }> = [];
  for (let account = 0; account < 6; account++) {
    // Separate module instances model account-worker realms sharing only IDB.
    vi.resetModules();
    const { createEngineTransport } = await import('./engine');
    const store = new DefaultStore();
    stores.push(store);
    const applier = new FrameApplier(store);
    const raw: Transport = {
      now: Date.now,
      setTimeout,
      clearTimeout: (timer) => clearTimeout(timer as ReturnType<typeof setTimeout>),
      prepareHttp: prepareRecordedHttp,
      async request(packet) {
        packet.signal?.throwIfAborted();
        if (new URL(packet.url).pathname === '/fapi/v1/exchangeInfo') {
          expect(packet.method).toBe('GET');
          expect(packet.body).toBeUndefined();
          sends.push({ account, at: Date.now() - started });
          return answer(sends.length);
        }
        return response(200, '', '[]');
      },
      connect(_url, handlers) {
        let closed = false;
        queueMicrotask(() => {
          if (!closed) handlers.onOpen();
        });
        return {
          close() {
            closed = true;
          },
          send() {},
        };
      },
    };
    const transport = createEngineTransport('binance', { key: '', secret: '' }, raw);
    const prepare = transport.prepareHttp!.bind(transport);
    transport.prepareHttp = (packet) => {
      if (new URL(packet.url).pathname === '/fapi/v1/exchangeInfo')
        preparations.push({ account, at: Date.now() - started });
      return prepare(packet);
    };
    transports.push(transport);
    const host = new EngineHost(
      new WasmEngine('binance', JSON.stringify({ publicDataOnly: true })),
      transport,
      { onFrame: (buffer) => applier.apply(new Frame(buffer)) },
      {
        ownsEngine: true,
        ...(entropySeed === undefined ? {} : { entropy: seededEntropy(entropySeed + account * 16) }),
      },
    );
    hosts.push(host);
    host.start();
  }
  const advance = async (ms: number) => {
    for (let elapsed = 0; elapsed < ms; elapsed += 100) {
      await settle();
      await vi.advanceTimersByTimeAsync(Math.min(100, ms - elapsed));
      hosts.forEach((host) => host.flush());
    }
    await settle();
  };
  const loaded = (except = -1) =>
    stores.forEach((store, i) => {
      if (i === except) return;
      expect(store.loaded.markets).toBe(true);
      expect(store.markets.map((market) => market.symbol)).toContain('BTCUSDT');
    });
  return { advance, sends, preparations, loaded, stores, databases };
}

it('publishes one physical anonymous catalogue independently into six native Stores', async () => {
  const r = await sixEngines(async () => response());
  await r.advance(1500);
  r.loaded();
  expect(r.sends).toHaveLength(1);
});

it('shares failed waves and recovers all six engines without a cooldown bypass', async () => {
  const r = await sixEngines(async (attempt) => {
    if (attempt <= 2) throw new Error('Scripted network failure');
    return response();
  });
  await r.advance(500);
  expect(r.sends).toHaveLength(1);
  await r.advance(1000);
  expect(r.sends).toHaveLength(2);
  await r.advance(1000);
  expect(r.sends).toHaveLength(2);
  await r.advance(3500);
  r.loaded();
  expect(r.sends).toHaveLength(3);
  expect(r.sends[1].at - r.sends[0].at).toBeGreaterThanOrEqual(1000);
  expect(r.sends[2].at - r.sends[1].at).toBeGreaterThanOrEqual(2000);
});

it('honors shared Retry-After before electing one replacement request', async () => {
  const r = await sixEngines(async (attempt) =>
    attempt === 1 ? response(429, 'retry-after: 4', '{"code":-1003}') : response(),
  );
  await r.advance(3500);
  expect(r.sends).toHaveLength(1);
  await r.advance(2000);
  r.loaded();
  expect(r.sends).toHaveLength(2);
  expect(r.sends[1].at).toBeGreaterThanOrEqual(4000);
});

it('a cancelled follower cannot retire the physical leader or its remaining peers', async () => {
  let release!: (value: HttpResponse) => void;
  const pending = new Promise<HttpResponse>((resolve) => {
    release = resolve;
  });
  const r = await sixEngines(() => pending);
  await r.advance(500);
  expect(r.sends).toHaveLength(1);
  const follower = (r.sends[0].account + 1) % 6;
  hosts[follower].dispose();
  release(response());
  await r.advance(1500);
  r.loaded(follower);
  expect(r.sends).toHaveLength(1);
});

it('six engines still load independently when IndexedDB is unavailable from the start', async () => {
  const r = await sixEngines(async () => response(), true);
  await r.advance(1500);
  r.loaded();
  expect(r.sends).toHaveLength(6);
});

it('storage failure during a shared lease cannot stampede after the original deadlines', async () => {
  const r = await sixEngines(() => new Promise(() => {}));
  await r.advance(500);
  expect(r.sends).toHaveLength(1);
  vi.spyOn(IDBDatabase.prototype, 'transaction').mockImplementation(() => {
    throw new DOMException('storage lost', 'InvalidStateError');
  });
  await r.advance(21_000);
  expect(r.sends).toHaveLength(1);
  expect(r.stores.every((store) => !store.loaded.markets)).toBe(true);
});

it('the first native retry after a closed-database pause physically dispatches before the startup watchdog', async () => {
  let release!: (value: HttpResponse) => void;
  const pending = new Promise<HttpResponse>((resolve) => {
    release = resolve;
  });
  const r = await sixEngines(async (attempt) => (attempt === 1 ? pending : response()), false, 17);
  await r.advance(500);
  expect(r.sends).toHaveLength(1);
  expect(r.databases.size).toBe(6);
  r.databases.forEach((db) => closeDatabase(db));
  release(response(503));
  await r.advance(29_000);
  expect(r.sends).toHaveLength(1);
  expect(r.stores.every((store) => !store.loaded.markets)).toBe(true);
  for (let elapsed = 29_500; elapsed < 58_500 && r.sends.length === 1; elapsed += 100) await r.advance(100);
  expect(r.sends).toHaveLength(2);
  expect(r.sends[1].at).toBeGreaterThanOrEqual(30_500);
  expect(r.sends[1].at).toBeLessThan(60_000);
  const firstRetry = r.preparations.find((attempt) => attempt.at >= 30_500)!;
  expect(firstRetry).toBeDefined();
  expect(r.sends[1].account).toBe(firstRetry.account);
  expect(r.sends[1].at - firstRetry.at).toBeLessThanOrEqual(100);
  await r.advance(58_500 - r.sends[1].at);
  for (let i = 2; i < r.sends.length; i++) expect(r.sends[i].at - r.sends[i - 1].at).toBeGreaterThanOrEqual(5000);
  r.loaded();
});
