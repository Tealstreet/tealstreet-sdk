import { setImmediate } from 'node:timers';

import { IDBDatabase, IDBFactory } from 'fake-indexeddb';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { nativeTransport } from '@tealstreet/safe-cex-wasm/host';
import { hyperliquidInfoWeight } from './hyperliquidInfoBudget';

const ORIGIN = 'https://api.hyperliquid.xyz';
const ORDERS = JSON.stringify({ type: 'frontendOpenOrders', user: 'local-only' });
const signal = () => new AbortController().signal;
const flushDatabase = async () => {
  for (let i = 0; i < 12; i++) await new Promise<void>((resolve) => setImmediate(resolve));
};
let factory: IDBFactory;
let budgetModule: typeof import('./hyperliquidInfoBudget');
const client = () => new budgetModule.HyperliquidInfoBudget(new budgetModule.IndexedInfoBudgetStorage(factory));

beforeEach(async () => {
  vi.resetModules();
  factory = new IDBFactory();
  vi.useFakeTimers({ toFake: ['Date', 'setTimeout', 'clearTimeout'] });
  vi.setSystemTime(1_000_000);
  budgetModule = await import('./hyperliquidInfoBudget');
});
afterEach(() => {
  vi.restoreAllMocks();
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

describe('unavailable shared storage', () => {
  it.each([false, true])(
    'uses normal pacing for never-opened storage unless another client opened it: %s',
    async (opened) => {
      vi.spyOn(console, 'warn').mockImplementation(() => undefined);
      if (opened) await client().response(ORIGIN, 200, '');
      const unavailable = {
        open: () => {
          throw new DOMException('storage disabled', 'SecurityError');
        },
      } as unknown as IDBFactory;
      const a = new budgetModule.HyperliquidInfoBudget(new budgetModule.IndexedInfoBudgetStorage(unavailable));
      await a.wait(ORIGIN, ORDERS, Date.now() + 20_000, signal());
      let admitted = false;
      const next = a.wait(ORIGIN, ORDERS, Date.now() + 20_000, signal()).then(() => {
        admitted = true;
      });
      const delay = opened ? 4800 : 1200;
      await vi.advanceTimersByTimeAsync(delay - 1);
      expect(admitted).toBe(false);
      await vi.advanceTimersByTimeAsync(1);
      await next;
      expect(admitted).toBe(true);
    },
  );

  it('admits a startup metadata burst within the read window when storage never opened', async () => {
    vi.spyOn(console, 'warn').mockImplementation(() => undefined);
    const unavailable = {
      open: () => {
        throw new DOMException('storage disabled', 'SecurityError');
      },
    } as unknown as IDBFactory;
    const a = new budgetModule.HyperliquidInfoBudget(new budgetModule.IndexedInfoBudgetStorage(unavailable));
    const reads = Promise.allSettled(
      Array.from({ length: 16 }, () =>
        a.wait(ORIGIN, JSON.stringify({ type: 'metaAndAssetCtxs' }), Date.now() + 20_000, signal()),
      ),
    );
    await vi.advanceTimersByTimeAsync(20_000);
    expect((await reads).every((result) => result.status === 'fulfilled')).toBe(true);
  });

  it('recovers a denied IndexedDB global accessor after the cooldown', async () => {
    vi.spyOn(console, 'warn').mockImplementation(() => undefined);
    const opened = vi.spyOn(factory, 'open');
    let allowed = false;
    vi.stubGlobal('indexedDB', factory);
    Object.defineProperty(globalThis, 'indexedDB', {
      configurable: true,
      get: () => {
        if (!allowed) throw new DOMException('storage disabled', 'SecurityError');
        return factory;
      },
    });
    const a = new budgetModule.HyperliquidInfoBudget();
    await a.wait(ORIGIN, ORDERS, Date.now() + 20_000, signal());
    expect(opened).not.toHaveBeenCalled();
    allowed = true;
    await vi.advanceTimersByTimeAsync(30_000);
    await a.response(ORIGIN, 200, '');
    expect(opened).toHaveBeenCalledOnce();
  });

  const stalledTransaction = () =>
    ({ objectStore: () => ({ get: () => ({}) }), abort: vi.fn() }) as unknown as IDBTransaction;

  it('retries one transaction timeout without splitting into the local budget', async () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => undefined);
    const a = client();
    await a.response(ORIGIN, 200, '');
    vi.spyOn(IDBDatabase.prototype, 'transaction').mockImplementationOnce(stalledTransaction);
    const waiting = a.wait(ORIGIN, ORDERS, Date.now() + 20_000, signal());
    await flushDatabase();
    await vi.advanceTimersByTimeAsync(1500);
    await flushDatabase();
    await waiting;
    expect(warn).not.toHaveBeenCalled();
  });

  it.each(['pause', 'reservation'])('rejoins after cooldown and max-merges a local %s', async (mode) => {
    vi.spyOn(console, 'warn').mockImplementation(() => undefined);
    const a = client();
    const b = client();
    await b.wait(ORIGIN, ORDERS, Date.now() + 20_000, signal());
    vi.spyOn(IDBDatabase.prototype, 'transaction').mockImplementationOnce(() => {
      throw new DOMException('storage disabled', 'InvalidStateError');
    });
    await a.wait(ORIGIN, ORDERS, Date.now() + 20_000, signal());
    if (mode === 'pause') {
      await a.response(ORIGIN, 429, 'Retry-After: 60');
      await vi.advanceTimersByTimeAsync(30_000);
    } else {
      const candles = JSON.stringify({ type: 'candleSnapshot' });
      const first = a.wait(ORIGIN, candles, Date.now() + 60_000, signal());
      const second = a.wait(ORIGIN, candles, Date.now() + 60_000, signal());
      await vi.advanceTimersByTimeAsync(4800);
      await first;
      await vi.advanceTimersByTimeAsync(24_960);
      await second;
      await vi.advanceTimersByTimeAsync(240);
    }
    await a.response(ORIGIN, 200, '');
    await expect(b.wait(ORIGIN, ORDERS, Date.now() + 20_000, signal())).rejects.toMatchObject({
      code: 'HTTP_PREPARATION_TIMED_OUT',
    });
  });

  it('bounds aggregate admission while one worker times out twice and recovers during a burst', async () => {
    vi.spyOn(console, 'warn').mockImplementation(() => undefined);
    const { HyperliquidInfoBudget: Budget, IndexedInfoBudgetStorage: Storage } =
      await import('./hyperliquidInfoBudget');
    const degradedConnections = new WeakSet<IDBDatabase>();
    const unstableFactory = {
      open: (name: string, version: number) => {
        const request = factory.open(name, version);
        request.addEventListener('success', () => degradedConnections.add(request.result));
        return request;
      },
    } as IDBFactory;
    const original = IDBDatabase.prototype.transaction;
    let stalls = 2;
    vi.spyOn(IDBDatabase.prototype, 'transaction').mockImplementation(function (this: IDBDatabase, ...args) {
      if (degradedConnections.has(this) && stalls > 0) {
        stalls -= 1;
        return stalledTransaction();
      }
      return original.apply(this, args);
    });
    const a = new Budget(new Storage(unstableFactory));
    const b = new Budget(new Storage(factory));
    const first = a.wait(ORIGIN, ORDERS, Date.now() + 20_000, signal());
    await flushDatabase();
    await vi.advanceTimersByTimeAsync(1500);
    await flushDatabase();
    await vi.advanceTimersByTimeAsync(1500);
    await flushDatabase();
    await first;
    const end = Date.now() + 60_000;
    const sends: number[] = [];
    const burst = async (budget: InstanceType<typeof Budget>) => {
      while (Date.now() < end) {
        try {
          await budget.wait(ORIGIN, ORDERS, end, signal());
          if (Date.now() < end) sends.push(Date.now());
        } catch (error) {
          expect(error).toMatchObject({ code: 'HTTP_PREPARATION_TIMED_OUT' });
          return;
        }
      }
    };
    const bursting = Promise.all([burst(a), burst(b)]);
    for (let step = 0; step < 300; step++) {
      await flushDatabase();
      await vi.advanceTimersByTimeAsync(200);
    }
    await flushDatabase();
    await bursting;
    expect(stalls).toBe(0);
    expect(sends.length * 20).toBeLessThanOrEqual(1300);
    expect(sends.length * 20).toBeGreaterThanOrEqual(900);
  });

  it('keeps a known 429 pause when an established database transaction fails', async () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => undefined);
    const a = client();
    await a.response(ORIGIN, 429, 'Retry-After: 10');
    vi.spyOn(IDBDatabase.prototype, 'transaction').mockImplementationOnce(() => {
      throw new DOMException('storage disabled', 'InvalidStateError');
    });
    await expect(a.wait(ORIGIN, ORDERS, Date.now() + 9999, signal())).rejects.toMatchObject({
      code: 'HTTP_PREPARATION_TIMED_OUT',
    });
    const next = a.wait(ORIGIN, ORDERS, Date.now() + 20_000, signal());
    await vi.advanceTimersByTimeAsync(10_000);
    await next;
    expect(warn).toHaveBeenCalledOnce();
  });

  it.each([
    'throw',
    'error',
    'blocked',
    'stalled',
    'transactionThrow',
    'transactionError',
    'quota',
    'transactionStalled',
  ])('falls back once to a shared local budget on %s without poisoning later reads', async (failure) => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => undefined);
    const openRequest: any = {};
    const readRequest: any = {};
    const transaction: any = {
      objectStore: () => ({
        get: () => readRequest,
        put: () => {
          throw new DOMException('private details', 'QuotaExceededError');
        },
      }),
      abort: vi.fn(),
    };
    const database = {
      transaction: () => {
        if (failure === 'transactionThrow') throw new DOMException('private details', 'InvalidStateError');
        return transaction;
      },
      close: vi.fn(),
    };
    const open = vi.fn(() => {
      if (failure === 'throw') throw new DOMException('private details', 'SecurityError');
      return openRequest;
    });
    vi.stubGlobal('indexedDB', { open });
    const { HyperliquidInfoBudget: Budget } = await import('./hyperliquidInfoBudget');
    const first = new Budget().wait(ORIGIN, ORDERS, Date.now() + 20_000, signal());
    await flushDatabase();
    if (failure === 'error') openRequest.onerror();
    if (failure === 'blocked') openRequest.onblocked();
    if (failure.startsWith('transaction') || failure === 'quota') {
      openRequest.result = database;
      openRequest.onsuccess();
      await flushDatabase();
      if (failure === 'transactionError') transaction.onerror();
      if (failure === 'quota') readRequest.onsuccess();
    }
    await vi.advanceTimersByTimeAsync(3000);
    await flushDatabase();
    await first;
    const next = new Budget().wait(ORIGIN, ORDERS, Date.now() + 20_000, signal());
    await vi.advanceTimersByTimeAsync(4800);
    await next;
    expect(open).toHaveBeenCalledTimes(failure === 'stalled' || failure === 'transactionStalled' ? 2 : 1);
    expect(warn).toHaveBeenCalledTimes(1);
    expect(warn.mock.calls.flat().join(' ')).not.toContain('private details');
    if (failure === 'stalled' || failure === 'blocked') {
      openRequest.result = database;
      openRequest.onsuccess();
      expect(database.close).toHaveBeenCalledOnce();
    }
  });

  it('bounds a persisted pause and reservation after a clock step backwards', async () => {
    const a = client();
    await a.response(ORIGIN, 429, 'Retry-After: 3600');
    vi.setSystemTime(1000);
    await expect(client().wait(ORIGIN, ORDERS, Date.now() + 300_000, signal())).rejects.toMatchObject({
      code: 'HTTP_PREPARATION_TIMED_OUT',
    });
    await vi.advanceTimersByTimeAsync(300_000);
    await client().wait(ORIGIN, ORDERS, Date.now() + 20_000, signal());
  });
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
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => undefined);
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
    expect(warn).not.toHaveBeenCalled();
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
    await expect(client().wait(ORIGIN, ORDERS, Date.now() + 299_999, signal())).rejects.toMatchObject({
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
  it('does not spend the storage warning on an aborted 429 response', async () => {
    vi.stubGlobal('indexedDB', factory);
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => undefined);
    const response = vi
      .spyOn(budgetModule.HyperliquidInfoBudget.prototype, 'response')
      .mockImplementationOnce(() => new Promise(() => undefined))
      .mockRejectedValue(new Error('storage failure'));
    const { createEngineTransport } = await import('./engine');
    const transport = createEngineTransport(
      'hyperliquid',
      { key: '', secret: '', publicDataOnly: true },
      { ...nativeTransport(), request: async () => ({ status: 429, headers: '', body: new Uint8Array() }) },
    );
    const controller = new AbortController();
    const packet = { request: 1, method: 'POST' as const, url: ORIGIN + '/info', headers: {}, body: ORDERS };
    try {
      const first = transport.request({ ...packet, signal: controller.signal });
      await flushDatabase();
      expect(response).toHaveBeenCalledOnce();
      controller.abort();
      expect((await first).status).toBe(429);
      expect(warn).not.toHaveBeenCalled();
      const next = transport.request({ ...packet, request: 2 });
      await flushDatabase();
      await vi.advanceTimersByTimeAsync(1200);
      expect((await next).status).toBe(429);
      expect(warn).toHaveBeenCalledOnce();
    } finally {
      transport.dispose?.();
    }
  });

  it.each(['rejected', 'thrown', 'stalled'])(
    'keeps a successful HTTP answer when response bookkeeping is %s',
    async (failure) => {
      vi.stubGlobal('indexedDB', factory);
      const warn = vi.spyOn(console, 'warn').mockImplementation(() => undefined);
      const budget = await import('./hyperliquidInfoBudget');
      vi.spyOn(budget.HyperliquidInfoBudget.prototype, 'response').mockImplementation(() => {
        if (failure === 'thrown') throw new Error('private details');
        return failure === 'rejected' ? Promise.reject(new Error('private details')) : new Promise(() => undefined);
      });
      const { createEngineTransport } = await import('./engine');
      const transport = createEngineTransport(
        'hyperliquid',
        { key: '', secret: '', publicDataOnly: true },
        { ...nativeTransport(), request: async () => ({ status: 200, headers: '', body: new Uint8Array([42]) }) },
      );
      try {
        for (const request of [1, 2]) {
          const response = transport.request({
            request,
            method: 'POST',
            url: ORIGIN + '/info',
            headers: {},
            body: ORDERS,
          });
          await flushDatabase();
          await vi.advanceTimersByTimeAsync(1200);
          expect(await response).toMatchObject({ status: 200, body: new Uint8Array([42]) });
        }
        expect(warn).toHaveBeenCalledTimes(failure === 'stalled' ? 0 : 1);
        expect(warn.mock.calls.flat().join(' ')).not.toContain('private details');
      } finally {
        transport.dispose?.();
      }
    },
  );

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
