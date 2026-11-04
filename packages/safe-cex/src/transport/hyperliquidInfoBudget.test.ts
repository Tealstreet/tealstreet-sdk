import { setImmediate } from 'node:timers';

import { IDBFactory } from 'fake-indexeddb';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { nativeTransport } from '@tealstreet/safe-cex-wasm/host';
import { HyperliquidInfoBudget, hyperliquidInfoWeight, IndexedInfoBudgetStorage } from './hyperliquidInfoBudget';

const ORIGIN = 'https://api.hyperliquid.xyz';
const ORDERS = JSON.stringify({ type: 'frontendOpenOrders', user: 'local-only' });
const signal = () => new AbortController().signal;
const flushDatabase = async () => {
  for (let i = 0; i < 12; i++) await new Promise<void>((resolve) => setImmediate(resolve));
};
let factory: IDBFactory;
const client = () => new HyperliquidInfoBudget(new IndexedInfoBudgetStorage(factory));

beforeEach(() => {
  vi.resetModules();
  factory = new IDBFactory();
  vi.useFakeTimers({ toFake: ['Date', 'setTimeout', 'clearTimeout'] });
  vi.setSystemTime(1_000_000);
});
afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

describe('cross-worker Hyperliquid info admission', () => {
  it('shares admission within a realm when IndexedDB is unavailable', async () => {
    vi.stubGlobal('indexedDB', undefined);
    const { HyperliquidInfoBudget: Budget } = await import('./hyperliquidInfoBudget');
    await new Budget().wait(ORIGIN, ORDERS, Date.now() + 20_000, signal());
    let admitted = false;
    const next = new Budget().wait(ORIGIN, ORDERS, Date.now() + 20_000, signal()).then(() => {
      admitted = true;
    });
    await vi.advanceTimersByTimeAsync(1199);
    expect(admitted).toBe(false);
    await vi.advanceTimersByTimeAsync(1);
    await next;
    expect(admitted).toBe(true);
  });

  it('shares weighted admission across independent worker database connections', async () => {
    const a = client();
    const b = client();
    await a.wait(ORIGIN, ORDERS, Date.now() + 20_000, signal());
    let admitted = false;
    const next = b.wait(ORIGIN, ORDERS, Date.now() + 20_000, signal()).then(() => {
      admitted = true;
    });
    await flushDatabase();
    await vi.advanceTimersByTimeAsync(1199);
    expect(admitted).toBe(false);
    await vi.advanceTimersByTimeAsync(1);
    await next;
    expect(admitted).toBe(true);
  });

  it('does not fuse mainnet and testnet budgets', async () => {
    const a = client();
    await a.wait(ORIGIN, ORDERS, Date.now() + 20_000, signal());
    await a.wait('https://api.hyperliquid-testnet.xyz', ORDERS, Date.now() + 20_000, signal());
  });

  it('refuses a reservation beyond its deadline without sending or extending the queue', async () => {
    const a = client();
    const b = client();
    await a.wait(ORIGIN, ORDERS, Date.now() + 20_000, signal());
    await expect(b.wait(ORIGIN, ORDERS, Date.now() + 1000, signal())).rejects.toMatchObject({
      code: 'HTTP_PREPARATION_TIMED_OUT',
    });
    const next = b.wait(ORIGIN, ORDERS, Date.now() + 2000, signal());
    await flushDatabase();
    await vi.advanceTimersByTimeAsync(1200);
    await next;
  });

  it('cancels an aborted queued read and never admits it later', async () => {
    const a = client();
    await a.wait(ORIGIN, ORDERS, Date.now() + 20_000, signal());
    const controller = new AbortController();
    const waiting = client().wait(ORIGIN, ORDERS, Date.now() + 20_000, controller.signal);
    const rejection = expect(waiting).rejects.toThrow('disposed');
    await flushDatabase();
    controller.abort(new Error('disposed'));
    await rejection;
    await vi.advanceTimersByTimeAsync(20_000);
  });

  it('shares Retry-After and re-spaces existing reservations after the pause', async () => {
    const a = client();
    const b = client();
    const sends: number[] = [];
    await a.wait(ORIGIN, ORDERS, Date.now() + 20_000, signal());
    const old = b.wait(ORIGIN, ORDERS, Date.now() + 20_000, signal()).then(() => sends.push(Date.now()));
    await flushDatabase();
    await a.response(ORIGIN, 429, 'Retry-After: 10\r\n');
    const fresh = client()
      .wait(ORIGIN, ORDERS, Date.now() + 20_000, signal())
      .then(() => sends.push(Date.now()));
    await flushDatabase();
    await vi.advanceTimersByTimeAsync(1200);
    await flushDatabase();
    expect(sends).toEqual([]);
    await vi.advanceTimersByTimeAsync(8800);
    await flushDatabase();
    expect(sends).toEqual([1_010_000]);
    await vi.advanceTimersByTimeAsync(1200);
    await Promise.all([old, fresh]);
    expect(sends).toEqual([1_010_000, 1_011_200]);
  });

  it('honors a ban longer than the read deadline by refusing rather than dispatching late', async () => {
    const a = client();
    await a.response(ORIGIN, 429, 'retry-after: 30');
    await expect(client().wait(ORIGIN, ORDERS, Date.now() + 20_000, signal())).rejects.toMatchObject({
      code: 'HTTP_PREPARATION_TIMED_OUT',
    });
  });

  it('accepts HTTP-date Retry-After and ignores a successful response header', async () => {
    const a = client();
    await a.response(ORIGIN, 200, 'Retry-After: 60');
    await a.wait(ORIGIN, ORDERS, Date.now() + 20_000, signal());
    await a.response(ORIGIN, 429, 'Retry-After: ' + new Date(Date.now() + 30_000).toUTCString());
    await expect(client().wait(ORIGIN, ORDERS, Date.now() + 20_000, signal())).rejects.toMatchObject({
      code: 'HTTP_PREPARATION_TIMED_OUT',
    });
  });

  it('backs off repeated headerless 429s and caps overflowing header values', async () => {
    const a = client();
    await a.response(ORIGIN, 429, '');
    await expect(client().wait(ORIGIN, ORDERS, Date.now() + 4999, signal())).rejects.toMatchObject({
      code: 'HTTP_PREPARATION_TIMED_OUT',
    });
    await vi.advanceTimersByTimeAsync(5000);
    await client().response(ORIGIN, 429, '');
    await expect(client().wait(ORIGIN, ORDERS, Date.now() + 9999, signal())).rejects.toMatchObject({
      code: 'HTTP_PREPARATION_TIMED_OUT',
    });
    await a.response(ORIGIN, 429, 'Retry-After: 1e308');
    await expect(client().wait(ORIGIN, ORDERS, Date.now() + 3_599_999, signal())).rejects.toMatchObject({
      code: 'HTTP_PREPARATION_TIMED_OUT',
    });
  });

  it.each([
    ['clearinghouseState', 2],
    ['userRole', 60],
    ['metaAndAssetCtxs', 20],
    ['frontendOpenOrders', 20],
    ['userFillsByTime', 120],
    ['candleSnapshot', 104],
  ])('reserves the documented weight for %s including bounded reply surcharges', (type, weight) => {
    expect(hyperliquidInfoWeight(JSON.stringify({ type }))).toBe(weight);
  });
});

describe('managed transport admission', () => {
  it.each([
    ['legacy', 'frontendOpenOrders', 1200],
    ['managed', 'frontendOpenOrders', 1200],
    ['managed', 'metaAndAssetCtxs', 1200],
    ['managed', 'candleSnapshot', 6240],
  ] as const)(
    'applies the shared budget to two actual %s transports for %s before dispatch',
    async (mode, type, delay) => {
      vi.stubGlobal('indexedDB', factory);
      const { createEngineTransport } = await import('./engine');
      const sends: number[] = [];
      const base = {
        ...nativeTransport(),
        request: async () => {
          sends.push(Date.now());
          return { status: 200, headers: '', body: new Uint8Array() };
        },
        prepareHttp: () => ({
          bind: () => ({
            ready: () => true,
            dispatch: () => {
              sends.push(Date.now());
              return Promise.resolve({ status: 200, headers: '', body: new Uint8Array() });
            },
          }),
        }),
      };
      const a = createEngineTransport('hyperliquid', { key: '', secret: '', publicDataOnly: true }, base);
      const b = createEngineTransport('hyperliquid', { key: '', secret: '', publicDataOnly: true }, base);
      const packet = {
        request: 1,
        method: 'POST' as const,
        url: ORIGIN + '/info',
        headers: {},
        body: JSON.stringify({ type }),
        timeoutMs: 20_000,
      };
      const execute = async (transport: typeof a, request: typeof packet) => {
        if (mode === 'legacy') return transport.request(request);
        const prepared = await transport.prepareHttp!({ ...request, intent: 'public' });
        return prepared.bind(request).dispatch();
      };
      try {
        await execute(a, packet);
        const next = execute(b, { ...packet, request: 2 });
        await flushDatabase();
        expect(sends).toEqual([1_000_000]);
        await vi.advanceTimersByTimeAsync(delay);
        await next;
        expect(sends).toEqual([1_000_000, 1_000_000 + delay]);
      } finally {
        a.dispose?.();
        b.dispose?.();
      }
    },
  );

  it('propagates a transport 429 pause to another engine without dispatching past its deadline', async () => {
    vi.stubGlobal('indexedDB', factory);
    const { createEngineTransport } = await import('./engine');
    const request = vi.fn(async () => ({ status: 429, headers: 'Retry-After: 30', body: new Uint8Array() }));
    const base = { ...nativeTransport(), request };
    const options = { key: '', secret: '', publicDataOnly: true };
    const a = createEngineTransport('hyperliquid', options, base);
    const b = createEngineTransport('hyperliquidspot', options, base);
    const packet = {
      request: 1,
      method: 'POST' as const,
      url: ORIGIN + '/info',
      headers: {},
      body: ORDERS,
      timeoutMs: 20_000,
    };
    try {
      expect((await a.request(packet)).status).toBe(429);
      await expect(b.request({ ...packet, request: 2 })).rejects.toMatchObject({
        code: 'HTTP_PREPARATION_TIMED_OUT',
      });
      expect(request).toHaveBeenCalledTimes(1);
    } finally {
      a.dispose?.();
      b.dispose?.();
    }
  });
});
