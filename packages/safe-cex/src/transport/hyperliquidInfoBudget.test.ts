import type { InternalAxiosRequestConfig } from 'axios';
import type { HyperliquidInfoBudget } from './hyperliquidInfoBudget';

import { setImmediate } from 'node:timers';

import { AxiosHeaders } from 'axios';
import { IDBDatabase, IDBFactory, IDBObjectStore } from 'fake-indexeddb';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { nativeTransport } from '@tealstreet/safe-cex-wasm/host';
import { hyperliquidInfoBudgetKey, hyperliquidInfoWeight } from './hyperliquidInfoBudget';

const ORIGIN = 'https://api.hyperliquid.xyz';
const ORDERS = JSON.stringify({ type: 'frontendOpenOrders', user: 'local-only' });
const signal = () => new AbortController().signal;
const flushDatabase = async () => {
  for (let i = 0; i < 32; i++) await new Promise<void>((resolve) => setImmediate(resolve));
};
let factory: IDBFactory;
let budgetModule: typeof import('./hyperliquidInfoBudget');
const client = () => new budgetModule.HyperliquidInfoBudget(new budgetModule.IndexedInfoBudgetStorage(factory));
const admit = async (
  budget: HyperliquidInfoBudget,
  origin: string,
  body: string | undefined,
  deadline: number,
  signal: AbortSignal,
) => {
  // These existing controls exercise the full aggregate bucket; normal/bulk reserves have dedicated controls.
  const grant = await budget.wait(origin, body, deadline, signal, 'critical');
  return budget.admitDispatch(origin, grant, deadline, signal, () => 0);
};
const spend = async (budget: HyperliquidInfoBudget, weight = 1000) => {
  while (weight > 0) {
    const cost = weight >= 20 ? 20 : 2;
    await admit(
      budget,
      ORIGIN,
      cost === 20 ? ORDERS : JSON.stringify({ type: 'clearinghouseState' }),
      Date.now() + 20_000,
      signal(),
    );
    weight -= cost;
  }
};

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
  // Supersedes d732927cb30's established-store refusal: the maintainer now prioritizes
  // chart availability with half-rate local admission after shared storage fails.
  it.each([false, true])('allows conservative local dispatch after storage failure: opened=%s', async (opened) => {
    vi.spyOn(console, 'warn').mockImplementation(() => undefined);
    if (opened) await client().response(ORIGIN, 200, '');
    const unavailable = {
      open: () => {
        throw new DOMException('storage disabled', 'SecurityError');
      },
    } as unknown as IDBFactory;
    const a = new budgetModule.HyperliquidInfoBudget(new budgetModule.IndexedInfoBudgetStorage(unavailable));
    if (opened) {
      const grant = await a.wait(ORIGIN, ORDERS, Date.now() + 20_000, signal(), 'critical');
      const send = vi.fn();
      const result = Promise.allSettled([a.admitDispatch(ORIGIN, grant, Date.now() + 20_000, signal(), send)]);
      await vi.advanceTimersByTimeAsync(0);
      expect(send).toHaveBeenCalledOnce();
      expect(await result).toMatchObject([{ status: 'fulfilled' }]);
    } else {
      await spend(a);
      let sent = false;
      const next = admit(a, ORIGIN, ORDERS, Date.now() + 20_000, signal()).then(() => {
        sent = true;
      });
      await vi.advanceTimersByTimeAsync(1199);
      expect(sent).toBe(false);
      await vi.advanceTimersByTimeAsync(1);
      await next;
    }
  });

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
        admit(a, ORIGIN, JSON.stringify({ type: 'metaAndAssetCtxs' }), Date.now() + 20_000, signal()),
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
    await admit(a, ORIGIN, ORDERS, Date.now() + 20_000, signal());
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
    const waiting = admit(a, ORIGIN, ORDERS, Date.now() + 20_000, signal());
    await flushDatabase();
    await vi.advanceTimersByTimeAsync(1500);
    await flushDatabase();
    await waiting;
    expect(warn).not.toHaveBeenCalled();
  });

  it('max-merges a local 429 pause when established storage recovers', async () => {
    vi.spyOn(console, 'warn').mockImplementation(() => undefined);
    const a = client();
    const b = client();
    await spend(b, 20);
    vi.spyOn(IDBDatabase.prototype, 'transaction').mockImplementationOnce(() => {
      throw new DOMException('disabled', 'InvalidStateError');
    });
    await a.response(ORIGIN, 429, 'Retry-After: 60');
    await vi.advanceTimersByTimeAsync(30_000);
    await a.response(ORIGIN, 200, '');
    await expect(admit(b, ORIGIN, ORDERS, Date.now() + 20_000, signal())).rejects.toMatchObject({
      code: 'HTTP_PREPARATION_TIMED_OUT',
    });
  });

  it('max-merges memory spend from never-opened storage when it becomes available', async () => {
    vi.spyOn(console, 'warn').mockImplementation(() => undefined);
    let available = false;
    const unavailable = {
      open: (name: string, version: number) => {
        if (!available) throw new DOMException('disabled', 'SecurityError');
        return factory.open(name, version);
      },
    } as IDBFactory;
    const a = new budgetModule.HyperliquidInfoBudget(new budgetModule.IndexedInfoBudgetStorage(unavailable));
    await spend(a);
    available = true;
    await vi.advanceTimersByTimeAsync(30_000);
    await a.response(ORIGIN, 200, '');
    const b = client();
    await spend(b, 500);
    let sent = false;
    const next = admit(b, ORIGIN, ORDERS, Date.now() + 20_000, signal()).then(() => {
      sent = true;
    });
    await flushDatabase();
    expect(sent).toBe(false);
    await vi.advanceTimersByTimeAsync(1200);
    await next;
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
    const first = admit(a, ORIGIN, ORDERS, Date.now() + 20_000, signal());
    const firstOutcome = Promise.allSettled([first]);
    await flushDatabase();
    await vi.advanceTimersByTimeAsync(1500);
    await flushDatabase();
    await vi.advanceTimersByTimeAsync(1500);
    await flushDatabase();
    for (let step = 0; step < 160; step++) {
      await flushDatabase();
      await vi.advanceTimersByTimeAsync(200);
    }
    // c0705fca9 supersedes d732927cb30's blackout; recovery still restores the shared cap.
    expect(await firstOutcome).toMatchObject([{ status: 'fulfilled' }]);
    await admit(a, ORIGIN, ORDERS, Date.now() + 20_000, signal());
    const end = Date.now() + 60_000;
    const sends: number[] = [];
    const burst = async (budget: InstanceType<typeof Budget>) => {
      while (Date.now() < end) {
        try {
          await admit(budget, ORIGIN, ORDERS, end, signal());
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
    // After the cooldown, merged local debt shares the original aggregate bucket.
    expect(sends.length * 20).toBeLessThanOrEqual(2000);
    expect(sends.length * 20).toBeGreaterThanOrEqual(1500);
  });

  it('keeps a known 429 pause when an established database transaction fails', async () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => undefined);
    const a = client();
    await a.response(ORIGIN, 429, 'Retry-After: 60');
    vi.spyOn(IDBDatabase.prototype, 'transaction').mockImplementationOnce(() => {
      throw new DOMException('disabled', 'InvalidStateError');
    });
    await expect(admit(a, ORIGIN, ORDERS, Date.now() + 20_000, signal())).rejects.toMatchObject({
      code: 'HTTP_PREPARATION_TIMED_OUT',
    });
    await vi.advanceTimersByTimeAsync(30_000);
    await a.response(ORIGIN, 200, '');
    await expect(admit(client(), ORIGIN, ORDERS, Date.now() + 20_000, signal())).rejects.toMatchObject({
      code: 'HTTP_PREPARATION_TIMED_OUT',
    });
    expect(warn).toHaveBeenCalledOnce();
  });

  // d732927cb30's storage-fault matrix now requires the same degraded admission above.
  it.each([
    'throw',
    'error',
    'blocked',
    'stalled',
    'transactionThrow',
    'transactionError',
    'quota',
    'transactionStalled',
  ])('bounds storage failure %s and degrades admission without leaking details', async (failure) => {
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
    const first = Promise.allSettled([admit(new Budget(), ORIGIN, ORDERS, Date.now() + 20_000, signal())]);
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
    const established = failure.startsWith('transaction') || failure === 'quota';
    if (established) await vi.advanceTimersByTimeAsync(17_000);
    expect((await first)[0].status).toBe('fulfilled');
    const next = Promise.allSettled([admit(new Budget(), ORIGIN, ORDERS, Date.now() + 20_000, signal())]);
    await vi.advanceTimersByTimeAsync(20_000);
    expect((await next)[0].status).toBe('fulfilled');
    expect(open).toHaveBeenCalled();
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
    await expect(admit(client(), ORIGIN, ORDERS, Date.now() + 300_000, signal())).rejects.toMatchObject({
      code: 'HTTP_PREPARATION_TIMED_OUT',
    });
    await vi.advanceTimersByTimeAsync(300_000);
    const waiting = admit(client(), ORIGIN, ORDERS, Date.now() + 20_000, signal());
    await flushDatabase();
    await vi.advanceTimersByTimeAsync(1200);
    await waiting;
  });
});

describe('cross-worker Hyperliquid info admission', () => {
  it('refills to one full bucket after idle without accumulating extra capacity', async () => {
    const a = client();
    await spend(a);
    await vi.advanceTimersByTimeAsync(120_000);
    await spend(a);
    let admitted = false;
    const next = admit(client(), ORIGIN, ORDERS, Date.now() + 20_000, signal()).then(() => {
      admitted = true;
    });
    await flushDatabase();
    await vi.advanceTimersByTimeAsync(1199);
    expect(admitted).toBe(false);
    await vi.advanceTimersByTimeAsync(1);
    await next;
    expect(admitted).toBe(true);
  });

  it('shares admission within a realm when IndexedDB is unavailable', async () => {
    vi.stubGlobal('indexedDB', undefined);
    const { HyperliquidInfoBudget: Budget } = await import('./hyperliquidInfoBudget');
    await spend(new Budget());
    let admitted = false;
    const next = admit(new Budget(), ORIGIN, ORDERS, Date.now() + 20_000, signal()).then(() => {
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
    await spend(a);
    let admitted = false;
    const next = admit(b, ORIGIN, ORDERS, Date.now() + 20_000, signal()).then(() => {
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
    await spend(a);
    await admit(a, 'https://api.hyperliquid-testnet.xyz', ORDERS, Date.now() + 20_000, signal());
  });

  it('refuses a reservation beyond its deadline without sending or extending the queue', async () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => undefined);
    const a = client();
    const b = client();
    await spend(a);
    await expect(admit(b, ORIGIN, ORDERS, Date.now() + 1000, signal())).rejects.toMatchObject({
      code: 'HTTP_PREPARATION_TIMED_OUT',
    });
    const next = admit(b, ORIGIN, ORDERS, Date.now() + 2000, signal());
    await flushDatabase();
    await vi.advanceTimersByTimeAsync(1200);
    await next;
    expect(warn).not.toHaveBeenCalled();
  });

  it('cancels an aborted queued read and never admits it later', async () => {
    const a = client();
    await spend(a);
    const controller = new AbortController();
    const waiting = admit(client(), ORIGIN, ORDERS, Date.now() + 20_000, controller.signal);
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
    // Renewing leases performs real IDB I/O; settle it between simulated polling turns.
    const advance = async (ms: number) => {
      for (let left = ms; left > 0; left -= 100) {
        await vi.advanceTimersByTimeAsync(Math.min(left, 100));
        await flushDatabase();
      }
    };
    await spend(a);
    const old = admit(b, ORIGIN, ORDERS, Date.now() + 20_000, signal()).then(() => sends.push(Date.now()));
    await flushDatabase();
    await a.response(ORIGIN, 429, 'Retry-After: 10\r\n');
    const fresh = admit(client(), ORIGIN, ORDERS, Date.now() + 20_000, signal()).then(() => sends.push(Date.now()));
    await flushDatabase();
    await advance(1200);
    await flushDatabase();
    expect(sends).toEqual([]);
    await advance(8800);
    await flushDatabase();
    expect(sends).toEqual([]);
    await advance(1200);
    await flushDatabase();
    expect(sends).toEqual([1_011_200]);
    await advance(1200);
    await Promise.all([old, fresh]);
    expect(sends).toEqual([1_011_200, 1_012_400]);
  });

  it('honors a ban longer than the read deadline by refusing rather than dispatching late', async () => {
    const a = client();
    await a.response(ORIGIN, 429, 'retry-after: 30');
    await expect(admit(client(), ORIGIN, ORDERS, Date.now() + 20_000, signal())).rejects.toMatchObject({
      code: 'HTTP_PREPARATION_TIMED_OUT',
    });
  });

  it('accepts HTTP-date Retry-After and ignores a successful response header', async () => {
    const a = client();
    await a.response(ORIGIN, 200, 'Retry-After: 60');
    await admit(a, ORIGIN, ORDERS, Date.now() + 20_000, signal());
    await a.response(ORIGIN, 429, 'Retry-After: ' + new Date(Date.now() + 30_000).toUTCString());
    await expect(admit(client(), ORIGIN, ORDERS, Date.now() + 20_000, signal())).rejects.toMatchObject({
      code: 'HTTP_PREPARATION_TIMED_OUT',
    });
  });

  it('backs off repeated headerless 429s and caps overflowing header values', async () => {
    const a = client();
    await a.response(ORIGIN, 429, '');
    await expect(admit(client(), ORIGIN, ORDERS, Date.now() + 4999, signal())).rejects.toMatchObject({
      code: 'HTTP_PREPARATION_TIMED_OUT',
    });
    await vi.advanceTimersByTimeAsync(5000);
    await client().response(ORIGIN, 429, '');
    await expect(admit(client(), ORIGIN, ORDERS, Date.now() + 9999, signal())).rejects.toMatchObject({
      code: 'HTTP_PREPARATION_TIMED_OUT',
    });
    await a.response(ORIGIN, 429, 'Retry-After: 1e308');
    await expect(admit(client(), ORIGIN, ORDERS, Date.now() + 299_999, signal())).rejects.toMatchObject({
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
  it.each(['legacy', 'managed'] as const)(
    'dispatches a cold discovery burst without spacing through %s HTTP',
    async (mode) => {
      vi.stubGlobal('indexedDB', factory);
      const { createEngineTransport } = await import('./engine');
      const sends: number[] = [];
      const send = async () => {
        sends.push(Date.now());
        return { status: 200, headers: '', body: new Uint8Array() };
      };
      const transport = createEngineTransport(
        'hyperliquid',
        { key: '', secret: '', publicDataOnly: true },
        {
          ...nativeTransport(),
          request: send,
          prepareHttp: () => ({ bind: () => ({ ready: () => true, dispatch: send }) }),
        },
      );
      try {
        for (let request = 1; request <= 12; request++) {
          const packet = {
            request,
            method: 'POST' as const,
            url: ORIGIN + '/info',
            headers: {},
            body: JSON.stringify({ type: request === 2 ? 'perpDexs' : 'metaAndAssetCtxs' }),
          };
          if (mode === 'legacy') await transport.request(packet);
          else await (await transport.prepareHttp!({ ...packet, intent: 'public' })).bind(packet).dispatch();
        }
        expect(sends).toEqual(Array(12).fill(1_000_000));
      } finally {
        transport.dispose?.();
      }
    },
  );

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
      for (let i = 0; i < 4 && response.mock.calls.length === 0; i++) await flushDatabase();
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
    ['legacy', 'frontendOpenOrders', 20],
    ['managed', 'frontendOpenOrders', 20],
    ['managed', 'metaAndAssetCtxs', 20],
    ['managed', 'candleSnapshot', 104],
  ] as const)(
    'applies the shared budget to two actual %s transports for %s before dispatch',
    async (mode, type, weight) => {
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
        const capacity = 800;
        const initial = Math.floor(capacity / weight);
        const delay = ((initial + 1) * weight - capacity) * 60;
        for (let request = 1; request <= initial; request++) await execute(a, { ...packet, request });
        const next = execute(b, { ...packet, request: initial + 1 });
        await flushDatabase();
        expect(sends).toEqual(Array(initial).fill(1_000_000));
        await vi.advanceTimersByTimeAsync(delay);
        await next;
        expect(sends).toEqual([...Array(initial).fill(1_000_000), 1_000_000 + delay]);
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

describe('independent review boundaries', () => {
  it('review: independent module realms share the healthy bucket atomically', async () => {
    const a = client();
    await spend(a);
    vi.resetModules();
    const realm = await import('./hyperliquidInfoBudget');
    const b = new realm.HyperliquidInfoBudget(new realm.IndexedInfoBudgetStorage(factory));
    let admitted = false;
    const waiting = admit(b, ORIGIN, ORDERS, Date.now() + 20_000, signal()).then(() => (admitted = true));
    await flushDatabase();
    expect(admitted).toBe(false);
    await vi.advanceTimersByTimeAsync(1199);
    expect(admitted).toBe(false);
    await vi.advanceTimersByTimeAsync(1);
    await waiting;
    expect(admitted).toBe(true);
  });

  it('review: documents cold unavailable storage multiplying the burst across realms', async () => {
    vi.spyOn(console, 'warn').mockImplementation(() => undefined);
    const unavailable = {
      open: () => {
        throw new DOMException('denied', 'SecurityError');
      },
    } as unknown as IDBFactory;
    const a = new budgetModule.HyperliquidInfoBudget(new budgetModule.IndexedInfoBudgetStorage(unavailable));
    await spend(a);
    vi.resetModules();
    const realm = await import('./hyperliquidInfoBudget');
    const b = new realm.HyperliquidInfoBudget(new realm.IndexedInfoBudgetStorage(unavailable));
    await spend(b);
    expect(Date.now()).toBe(1_000_000);
  });

  it.each([
    [false, false],
    [true, false],
    [false, true],
    [true, true],
  ])(
    'prepared unsent read respects a later 429: separate realms=%s, already bound=%s',
    async (separateRealm, alreadyBound) => {
      vi.stubGlobal('indexedDB', factory);
      const { createEngineTransport } = await import('./engine');
      const sends: string[] = [];
      const packet = { request: 1, method: 'POST' as const, url: ORIGIN + '/info', headers: {}, body: ORDERS };
      const a = createEngineTransport(
        'hyperliquid',
        { key: '', secret: '', publicDataOnly: true },
        {
          ...nativeTransport(),
          prepareHttp: () => ({
            bind: () => ({
              ready: () => true,
              dispatch: async () => {
                sends.push('prepared after 429');
                return { status: 200, headers: '', body: new Uint8Array() };
              },
            }),
          }),
        },
      );
      if (separateRealm) vi.resetModules();
      const createOther = separateRealm ? (await import('./engine')).createEngineTransport : createEngineTransport;
      const b = createOther(
        'hyperliquidspot',
        { key: '', secret: '', publicDataOnly: true },
        {
          ...nativeTransport(),
          request: async () => {
            sends.push('429');
            return { status: 429, headers: 'Retry-After: 30', body: new Uint8Array() };
          },
        },
      );
      try {
        const prepared = await a.prepareHttp!({ ...packet, intent: 'public' });
        const bound = alreadyBound ? prepared.bind(packet) : undefined;
        expect(sends).toEqual([]);
        expect((await b.request({ ...packet, request: 2 })).status).toBe(429);
        await expect(b.request({ ...packet, request: 3 })).rejects.toMatchObject({
          code: 'HTTP_PREPARATION_TIMED_OUT',
        });
        await expect(Promise.resolve().then(() => (bound ?? prepared.bind(packet)).dispatch())).rejects.toMatchObject({
          code: 'HTTP_PREPARATION_TIMED_OUT',
        });
        expect(sends).toEqual(['429']);
      } finally {
        a.dispose?.();
        b.dispose?.();
      }
    },
  );
});

it('does not repeat a physical send when its final admission transaction fails after handoff', async () => {
  const warn = vi.spyOn(console, 'warn').mockImplementation(() => undefined);
  const budget = client();
  const controller = new AbortController();
  const generation = await budget.wait(ORIGIN, ORDERS, Date.now() + 20_000, controller.signal, 'critical');
  const put = IDBObjectStore.prototype.put;
  vi.spyOn(IDBObjectStore.prototype, 'put')
    .mockImplementationOnce(function (this: IDBObjectStore, ...args) {
      return put.apply(this, args);
    })
    .mockImplementationOnce(() => {
      throw new DOMException('full', 'QuotaExceededError');
    });
  const send = vi.fn(() => 42);
  expect(await budget.admitDispatch(ORIGIN, generation, Date.now() + 20_000, controller.signal, send)).toBe(42);
  await flushDatabase();
  expect(send).toHaveBeenCalledOnce();
  expect(warn).toHaveBeenCalledOnce();
  vi.resetModules();
  const realm = await import('./hyperliquidInfoBudget');
  const peerBudget = new realm.HyperliquidInfoBudget(new realm.IndexedInfoBudgetStorage(factory));
  await spend(peerBudget, 980);
  let peerSent = false;
  const peer = admit(peerBudget, ORIGIN, ORDERS, Date.now() + 20_000, signal()).then(() => {
    peerSent = true;
  });
  await flushDatabase();
  expect(peerSent).toBe(false);
  await vi.advanceTimersByTimeAsync(1200);
  await peer;
});

it.each(['memory', 'indexed'] as const)(
  'does not charge abandoned preparation or queued grants with %s storage',
  async (mode) => {
    if (mode === 'memory') vi.stubGlobal('indexedDB', undefined);
    const a = mode === 'memory' ? new budgetModule.HyperliquidInfoBudget() : client();
    const b = mode === 'memory' ? new budgetModule.HyperliquidInfoBudget() : client();
    await spend(a, 960);
    const controllers = Array.from({ length: 4 }, () => new AbortController());
    const pending = Promise.allSettled(
      controllers.map((controller) => a.wait(ORIGIN, '{"type":"userRole"}', Date.now() + 20_000, controller.signal)),
    );
    await flushDatabase();
    controllers.forEach((controller) => controller.abort());
    await pending;
    let sent = false;
    const peer = admit(b, ORIGIN, ORDERS, Date.now() + 20_000, signal()).then(() => {
      sent = true;
    });
    await flushDatabase();
    expect(sent).toBe(true);
    await peer;
    await b.wait(ORIGIN, ORDERS, Date.now() + 20_000, signal(), 'critical');
    await admit(a, ORIGIN, ORDERS, Date.now() + 20_000, signal());
  },
);

it('never refunds a request handed to fetch when the caller aborts', async () => {
  const a = client();
  const controller = new AbortController();
  const grant = await a.wait(ORIGIN, ORDERS, Date.now() + 20_000, controller.signal, 'critical');
  const send = vi.fn();
  await a.admitDispatch(ORIGIN, grant, Date.now() + 20_000, controller.signal, send);
  controller.abort();
  await spend(a, 980);
  let sent = false;
  const peer = admit(client(), ORIGIN, ORDERS, Date.now() + 20_000, signal()).then(() => {
    sent = true;
  });
  await flushDatabase();
  expect(sent).toBe(false);
  await vi.advanceTimersByTimeAsync(1200);
  await peer;
  expect(send).toHaveBeenCalledOnce();
});

it('does not charge a managed claim that waits without dispatching', async () => {
  vi.stubGlobal('indexedDB', factory);
  const { createEngineTransport } = await import('./engine');
  const send = vi.fn(async () => ({ status: 200, headers: '', body: new Uint8Array() }));
  const transport = createEngineTransport(
    'hyperliquid',
    { key: '', secret: '', publicDataOnly: true },
    {
      ...nativeTransport(),
      request: send,
      prepareHttp: () => ({ bind: () => ({ ready: () => true, dispatch: send }) }),
    },
  );
  const packet = { request: 1, method: 'POST' as const, url: ORIGIN + '/info', headers: {}, body: ORDERS };
  try {
    const bound = (await transport.prepareHttp!({ ...packet, intent: 'public' })).bind(packet);
    await bound.admitDispatch!(() => undefined);
    expect(send).not.toHaveBeenCalled();
    await spend(client());
    let done = false;
    const next = admit(client(), ORIGIN, ORDERS, Date.now() + 20_000, signal()).then(() => {
      done = true;
    });
    await flushDatabase();
    expect(done).toBe(false);
    await vi.advanceTimersByTimeAsync(1200);
    await next;
  } finally {
    transport.dispose?.();
  }
});

it('atomically charges concurrent dispatches across workers while granting preparations freely', async () => {
  const a = client();
  const b = client();
  const deadline = Date.now() + 20_000;
  const grants = await Promise.all(
    Array.from({ length: 60 }, (_, index) => (index % 2 ? a : b).wait(ORIGIN, ORDERS, deadline, signal(), 'critical')),
  );
  const sends: number[] = [];
  const requests = Promise.all(
    grants.map((grant, index) =>
      (index % 2 ? a : b).admitDispatch(ORIGIN, grant, deadline, signal(), () => {
        sends.push(Date.now());
      }),
    ),
  );
  for (let i = 0; i < 30; i++) await flushDatabase();
  expect(sends).toEqual(Array(50).fill(1_000_000));
  for (let step = 0; step < 120; step++) {
    await flushDatabase();
    await vi.advanceTimersByTimeAsync(100);
  }
  await flushDatabase();
  await requests;
  expect(sends).toHaveLength(60);
  expect(sends.filter((at) => at < 1_001_200)).toHaveLength(50);
});

it.each([false, true])(
  'commits the charge before fetch so a write failure cannot hide spend from a peer (failure=%s)',
  async (failure) => {
    vi.spyOn(console, 'warn').mockImplementation(() => undefined);
    const a = client();
    await spend(a, 980);
    await flushDatabase();
    const grant = await a.wait(ORIGIN, ORDERS, Date.now() + 20_000, signal(), 'critical');
    if (failure)
      vi.spyOn(IDBObjectStore.prototype, 'put').mockImplementationOnce(() => {
        throw new DOMException('full', 'QuotaExceededError');
      });
    const physical = vi.fn();
    const pending = Promise.allSettled([a.admitDispatch(ORIGIN, grant, Date.now() + 20_000, signal(), physical)]);
    await flushDatabase();
    vi.resetModules();
    const realm = await import('./hyperliquidInfoBudget');
    const b = new realm.HyperliquidInfoBudget(new realm.IndexedInfoBudgetStorage(factory));
    let peerSent = false;
    const peer = admit(b, ORIGIN, ORDERS, Date.now() + 20_000, signal()).then(() => {
      peerSent = true;
    });
    await flushDatabase();
    expect(physical).toHaveBeenCalledTimes(failure ? 0 : 1);
    expect(peerSent).toBe(failure);
    await vi.advanceTimersByTimeAsync(1200);
    await peer;
    await vi.advanceTimersByTimeAsync(20_000);
    await pending;
  },
);

it.each(['cancel', 'pause'] as const)(
  'refunds a durable reservation only before dispatch and within its generation: %s',
  async (cause) => {
    const storage = new budgetModule.IndexedInfoBudgetStorage(factory);
    const a = new budgetModule.HyperliquidInfoBudget(storage);
    const b = client();
    const controller = new AbortController();
    const grant = await a.wait(ORIGIN, ORDERS, Date.now() + 20_000, controller.signal, 'critical');
    const update = storage.update.bind(storage);
    vi.spyOn(storage, 'update').mockImplementationOnce(async (...args) => {
      const reserved = await update(...args);
      if (cause === 'cancel') controller.abort(new Error('disposed'));
      else await b.response(ORIGIN, 429, 'Retry-After: 10');
      return reserved;
    });
    const send = vi.fn();
    await expect(a.admitDispatch(ORIGIN, grant, Date.now() + 20_000, controller.signal, send)).rejects.toThrow();
    expect(send).not.toHaveBeenCalled();
    if (cause === 'cancel') await spend(b);
    else await vi.advanceTimersByTimeAsync(10_000);
    let peerSent = false;
    const peer = admit(b, ORIGIN, ORDERS, Date.now() + 20_000, signal()).then(() => {
      peerSent = true;
    });
    await flushDatabase();
    expect(peerSent).toBe(false);
    await vi.advanceTimersByTimeAsync(1199);
    expect(peerSent).toBe(false);
    await vi.advanceTimersByTimeAsync(1);
    await peer;
  },
);

it('does not let newer cheap reads overtake an older cross-realm dispatch waiter', async () => {
  const a = client();
  await spend(a);
  const deadline = Date.now() + 20_000;
  const sent: string[] = [];
  const older = a.admitDispatch(ORIGIN, { generation: 0, weight: 20, readClass: 'critical' }, deadline, signal(), () =>
    sent.push('orders'),
  );
  await flushDatabase();
  vi.resetModules();
  const realm = await import('./hyperliquidInfoBudget');
  const b = new realm.HyperliquidInfoBudget(new realm.IndexedInfoBudgetStorage(factory));
  await vi.advanceTimersByTimeAsync(120);
  const newer = b.admitDispatch(ORIGIN, { generation: 0, weight: 2, readClass: 'critical' }, deadline, signal(), () =>
    sent.push('state'),
  );
  await flushDatabase();
  expect(sent).toEqual([]);
  for (let step = 0; step < 15; step++) {
    await vi.advanceTimersByTimeAsync(100);
    await flushDatabase();
  }
  await Promise.all([older, newer]);
  expect(sent).toEqual(['orders', 'state']);
});

it.each(['expired', 'cancelled'] as const)('a %s dispatch waiter cannot block a healthy peer', async (cause) => {
  const backend = new budgetModule.IndexedInfoBudgetStorage(factory);
  const a = new budgetModule.HyperliquidInfoBudget(backend);
  await spend(a);
  const sent: string[] = [];
  const controller = new AbortController();
  const deadline = Date.now() + 20_000;
  let abandoned: Promise<unknown> | undefined;
  if (cause === 'expired')
    await backend.update(ORIGIN, (state) => {
      state.dispatchQueue = [{ id: 'dead-worker', until: Date.now() + 2000, readClass: 'critical' }];
    });
  else {
    abandoned = a
      .admitDispatch(ORIGIN, { generation: 0, weight: 20, readClass: 'critical' }, deadline, controller.signal, () =>
        sent.push('cancelled'),
      )
      .catch(() => undefined);
    await flushDatabase();
    controller.abort();
    await abandoned;
  }
  const peer = client().admitDispatch(
    ORIGIN,
    { generation: 0, weight: 20, readClass: 'critical' },
    deadline,
    signal(),
    () => sent.push('peer'),
  );
  await flushDatabase();
  for (let step = 0; step < 21; step++) {
    await vi.advanceTimersByTimeAsync(100);
    await flushDatabase();
  }
  await peer;
  expect(sent).toEqual(['peer']);
});

it('does not require crypto.randomUUID to schedule ordinary reads', async () => {
  vi.stubGlobal('crypto', {});
  const sent = vi.fn();
  await client().admitDispatch(
    ORIGIN,
    { generation: 0, weight: 20, readClass: 'critical' },
    Date.now() + 20_000,
    signal(),
    sent,
  );
  expect(sent).toHaveBeenCalledOnce();
});

describe('critical order reads', () => {
  it('holds 200 weight for critical reads across workers', async () => {
    const ordinary = client();
    const critical = client();
    const grant = await ordinary.wait(ORIGIN, ORDERS, Date.now() + 20_000, signal(), 'normal');
    await spend(ordinary, 800);
    const send = vi.fn();
    const waiting = ordinary.admitDispatch(ORIGIN, grant, Date.now() + 20_000, signal(), send);
    await flushDatabase();
    expect(send).not.toHaveBeenCalled();
    const urgent = await critical.wait(ORIGIN, ORDERS, Date.now() + 20_000, signal(), 'critical');
    const sent = vi.fn();
    await critical.admitDispatch(ORIGIN, urgent, Date.now() + 20_000, signal(), sent);
    expect(sent).toHaveBeenCalledOnce();
    await vi.advanceTimersByTimeAsync(2400);
    await waiting;
  });

  it('a critical read passes an older bulk waiter without minting total budget', async () => {
    const a = client();
    const b = client();
    const abort = new AbortController();
    const bulk = await a.wait(
      ORIGIN,
      JSON.stringify({ type: 'candleSnapshot' }),
      Date.now() + 20_000,
      abort.signal,
      'bulk',
    );
    const urgent = await b.wait(ORIGIN, ORDERS, Date.now() + 20_000, signal(), 'critical');
    await spend(a, 900);
    const bulkSend = vi.fn();
    const waiting = Promise.allSettled([a.admitDispatch(ORIGIN, bulk, Date.now() + 20_000, abort.signal, bulkSend)]);
    await flushDatabase();
    const sent = vi.fn();
    const finish = b.admitDispatch(ORIGIN, urgent, Date.now() + 20_000, signal(), sent);
    await flushDatabase();
    expect(sent).toHaveBeenCalledOnce();
    expect(bulkSend).not.toHaveBeenCalled();
    await finish;
    abort.abort();
    await waiting;
    await spend(b, 80);
    await expect(b.wait(ORIGIN, ORDERS, Date.now() + 1, signal(), 'critical')).rejects.toMatchObject({
      code: 'HTTP_PREPARATION_TIMED_OUT',
    });
  });

  it('critical grants cannot bypass a shared 429 pause', async () => {
    const a = client();
    const b = client();
    const grant = await b.wait(ORIGIN, ORDERS, Date.now() + 20_000, signal(), 'critical');
    await a.response(ORIGIN, 429, 'Retry-After: 5');
    const send = vi.fn();
    await expect(b.admitDispatch(ORIGIN, grant, Date.now() + 20_000, signal(), send)).rejects.toMatchObject({
      code: 'HTTP_PREPARATION_TIMED_OUT',
    });
    expect(send).not.toHaveBeenCalled();
  });
});

it('preserves critical FIFO when the older high-weight read starts waiting before Prepare completes', async () => {
  const a = client();
  const b = client();
  const abort = new AbortController();
  await spend(a);
  const sent: string[] = [];
  const older = (async () => {
    const grant = await a.wait(ORIGIN, '{"type":"userRole"}', Date.now() + 20_000, abort.signal, 'critical');
    await a.admitDispatch(ORIGIN, grant, Date.now() + 20_000, abort.signal, () => sent.push('role'));
  })();
  const settled = Promise.allSettled([older]);
  try {
    await flushDatabase();
    await vi.advanceTimersByTimeAsync(120);
    const grant = await b.wait(ORIGIN, '{"type":"clearinghouseState"}', Date.now() + 20_000, abort.signal, 'critical');
    const newer = b.admitDispatch(ORIGIN, grant, Date.now() + 20_000, abort.signal, () => sent.push('state'));
    const joined = Promise.allSettled([newer]);
    await flushDatabase();
    expect(sent).toEqual([]);
    for (let i = 0; i < 40; i++) {
      await vi.advanceTimersByTimeAsync(100);
      await flushDatabase();
    }
    await joined;
    await settled;
    expect(sent).toEqual(['role', 'state']);
  } finally {
    abort.abort();
    await settled;
  }
});

it('the real managed transport admits a critical sizing read while its bulk lookup peer waits', async () => {
  vi.stubGlobal('indexedDB', factory);
  const { createEngineTransport } = await import('./engine');
  await spend(client(), 800);
  const sends: string[] = [];
  const raw = {
    ...nativeTransport(),
    prepareHttp: () => ({
      bind: (packet: { readClass?: string }) => ({
        ready: () => true,
        dispatch: async () => {
          sends.push(packet.readClass ?? 'normal');
          return { status: 200, headers: '', body: new Uint8Array() };
        },
      }),
    }),
  };
  const transport = createEngineTransport('hyperliquid', { key: '', secret: '', publicDataOnly: true }, raw);
  const packet = {
    request: 123,
    method: 'POST' as const,
    intent: 'public' as const,
    url: ORIGIN + '/info',
    headers: {},
    timeoutMs: 20_000,
    body: '{"type":"orderStatus","oid":9,"user":"fixture"}',
    readClass: 'critical' as const,
  };
  try {
    let prepared: Awaited<ReturnType<NonNullable<typeof transport.prepareHttp>>> | undefined;
    const pending = Promise.resolve(transport.prepareHttp!(packet)).then((value) => {
      prepared = value;
    });
    void pending.catch(() => undefined);
    await flushDatabase();
    expect(prepared).toBeDefined();
    await pending;
    await prepared!.bind(packet).dispatch();
    expect(sends).toEqual(['critical']);
    const bulkPacket = { ...packet, request: 124, readClass: 'bulk' as const };
    const bulk = (async () => {
      const ready = await transport.prepareHttp!(bulkPacket);
      await ready.bind(bulkPacket).dispatch();
    })();
    await flushDatabase();
    expect(sends).toEqual(['critical']);
    for (let i = 0; i < 3; i++) {
      await vi.advanceTimersByTimeAsync(100);
      await flushDatabase();
    }
    await bulk;
    expect(sends).toEqual(['critical', 'bulk']);
  } finally {
    transport.dispose?.();
  }
});

it('cancelling a quota-waiting critical preparation cannot retire its peer or spend weight', async () => {
  const a = client();
  const b = client();
  const abort = new AbortController();
  await spend(a);
  const sends: string[] = [];
  const first = (async () => {
    const grant = await a.wait(ORIGIN, '{"type":"userRole"}', Date.now() + 20_000, abort.signal, 'critical');
    await a.admitDispatch(ORIGIN, grant, Date.now() + 20_000, abort.signal, () => sends.push('disposed'));
  })();
  const stopped = Promise.allSettled([first]);
  await flushDatabase();
  const peer = (async () => {
    const grant = await b.wait(ORIGIN, '{"type":"userRole"}', Date.now() + 20_000, signal(), 'critical');
    await b.admitDispatch(ORIGIN, grant, Date.now() + 20_000, signal(), () => sends.push('peer'));
  })();
  await flushDatabase();
  abort.abort();
  await stopped;
  for (let i = 0; i < 40; i++) {
    await vi.advanceTimersByTimeAsync(100);
    await flushDatabase();
  }
  await peer;
  expect(sends).toEqual(['peer']);
});

it('financial reconciliation shares the critical reserve and never exceeds the total quota', async () => {
  const a = client();
  await spend(a, 800);
  const controller = new AbortController();
  const grant = await a.wait(
    ORIGIN,
    '{"type":"clearinghouseState"}',
    Date.now() + 20_000,
    controller.signal,
    'reconciliation',
  );
  const send = vi.fn();
  await a.admitDispatch(ORIGIN, grant, Date.now() + 20_000, controller.signal, send);
  expect(send).toHaveBeenCalledOnce();
  await spend(a, 198);
  await expect(
    a.wait(ORIGIN, '{"type":"clearinghouseState"}', Date.now() + 1, controller.signal, 'reconciliation'),
  ).rejects.toMatchObject({ code: 'HTTP_PREPARATION_TIMED_OUT' });
});

it('financial reconciliation cannot bypass the shared 429 pause', async () => {
  const a = client();
  const controller = new AbortController();
  const grant = await a.wait(ORIGIN, ORDERS, Date.now() + 20_000, controller.signal, 'reconciliation');
  await a.response(ORIGIN, 429, 'Retry-After: 5');
  const send = vi.fn();
  await expect(a.admitDispatch(ORIGIN, grant, Date.now() + 20_000, controller.signal, send)).rejects.toMatchObject({
    code: 'HTTP_PREPARATION_TIMED_OUT',
  });
  expect(send).not.toHaveBeenCalled();
});

describe('closed info admission timeout diagnosis', () => {
  it('labels a refused quota reservation without changing its deadline or shared pause', async () => {
    const a = client();
    await a.response(ORIGIN, 429, 'retry-after: 30');
    await expect(admit(client(), ORIGIN, ORDERS, Date.now() + 20_000, signal())).rejects.toMatchObject({
      code: 'HTTP_PREPARATION_TIMED_OUT',
      cause: 'info_admission',
      message: 'HTTP admission deadline expired',
    });
    await vi.advanceTimersByTimeAsync(30_000);
    // The pause still owns the same empty-bucket refill; no earlier send is granted.
    await expect(admit(client(), ORIGIN, ORDERS, Date.now() + 1, signal())).rejects.toMatchObject({
      code: 'HTTP_PREPARATION_TIMED_OUT',
      cause: 'info_admission',
    });
  });

  it('labels a final admission refusal and never calls physical handoff', async () => {
    const a = client();
    const grant = await a.wait(ORIGIN, ORDERS, Date.now() + 20_000, signal(), 'critical');
    await client().response(ORIGIN, 429, 'retry-after: 30');
    const send = vi.fn();
    await expect(a.admitDispatch(ORIGIN, grant, Date.now() + 20_000, signal(), send)).rejects.toMatchObject({
      code: 'HTTP_PREPARATION_TIMED_OUT',
      cause: 'info_admission',
      message: 'HTTP admission deadline expired',
    });
    expect(send).not.toHaveBeenCalled();
  });

  it('labels the original deadline timer only while the budget wait is pending', async () => {
    const { HttpPreparationError } = await import('@tealstreet/safe-cex-wasm/host');
    const a = client();
    await spend(a);
    const abort = new AbortController();
    const waiting = a.wait(ORIGIN, ORDERS, Date.now() + 20_000, abort.signal, 'critical');
    const checked = expect(waiting).rejects.toMatchObject({
      code: 'HTTP_PREPARATION_TIMED_OUT',
      cause: 'info_admission',
      message: 'HTTP admission deadline expired',
    });
    await flushDatabase();
    abort.abort(new HttpPreparationError('timedOut'));
    await checked;
  });

  it('preserves cancellation and errors after handoff instead of labelling them admission', async () => {
    const { HttpPreparationError } = await import('@tealstreet/safe-cex-wasm/host');
    const a = client();
    const grant = await a.wait(ORIGIN, ORDERS, Date.now() + 20_000, signal(), 'critical');
    const error = new HttpPreparationError('timedOut');
    const send = vi.fn(() => {
      throw error;
    });
    await expect(a.admitDispatch(ORIGIN, grant, Date.now() + 20_000, signal(), send)).rejects.toBe(error);
    expect(send).toHaveBeenCalledOnce();
    expect(error.cause).toBeUndefined();
    const abort = new AbortController();
    const cancelled = new Error('disposed');
    abort.abort(cancelled);
    await expect(a.wait(ORIGIN, ORDERS, Date.now() + 20_000, abort.signal)).rejects.toBe(cancelled);
  });
});

it.each(['wait', 'dispatch'] as const)(
  'does not relabel a timeout already aborted before %s admission starts',
  async (stage) => {
    const { HttpPreparationError } = await import('@tealstreet/safe-cex-wasm/host');
    const a = client();
    const grant = await a.wait(ORIGIN, ORDERS, Date.now() + 20_000, signal(), 'critical');
    const abort = new AbortController();
    const error = new HttpPreparationError('timedOut');
    abort.abort(error);
    const send = vi.fn();
    const work =
      stage === 'wait'
        ? a.wait(ORIGIN, ORDERS, Date.now() + 20_000, abort.signal)
        : a.admitDispatch(ORIGIN, grant, Date.now() + 20_000, abort.signal, send);
    await expect(work).rejects.toBe(error);
    expect(error.cause).toBeUndefined();
    expect(send).not.toHaveBeenCalled();
  },
);

describe('mid-session storage degradation', () => {
  it('serves two candle reads after an established transaction never answers', async () => {
    vi.spyOn(console, 'warn').mockImplementation(() => undefined);
    const budget = client();
    await budget.response(ORIGIN, 200, '');
    const stalled = { objectStore: () => ({ get: () => ({}) }), abort: vi.fn() };
    vi.spyOn(IDBDatabase.prototype, 'transaction').mockReturnValue(stalled as unknown as IDBTransaction);
    const send = vi.fn(() => 0);
    const pending = Promise.allSettled(
      [0, 1].map(async () => {
        const controller = new AbortController();
        const body = JSON.stringify({ type: 'candleSnapshot' });
        const deadline = Date.now() + 20_000;
        const grant = await budget.wait(ORIGIN, body, deadline, controller.signal, 'bulk');
        return budget.admitDispatch(ORIGIN, grant, deadline, controller.signal, send);
      }),
    );
    await flushDatabase();
    await vi.advanceTimersByTimeAsync(1500);
    await flushDatabase();
    await vi.advanceTimersByTimeAsync(1500);
    await flushDatabase();
    expect(send).toHaveBeenCalledTimes(2);
    expect(await pending).toMatchObject([{ status: 'fulfilled' }, { status: 'fulfilled' }]);
  });

  it.each(['critical', 'reconciliation', 'normal', 'bulk'] as const)(
    'halves both local burst and refill after prior shared storage: %s',
    async (readClass) => {
      vi.spyOn(console, 'warn').mockImplementation(() => undefined);
      await client().response(ORIGIN, 200, '');
      const unavailable = {
        open: () => {
          throw new Error('unavailable');
        },
      } as unknown as IDBFactory;
      const budget = new budgetModule.HyperliquidInfoBudget(new budgetModule.IndexedInfoBudgetStorage(unavailable));
      const run = async () => {
        const controller = new AbortController();
        const deadline = Date.now() + 20_000;
        const grant = await budget.wait(ORIGIN, ORDERS, deadline, controller.signal, readClass);
        return budget.admitDispatch(ORIGIN, grant, deadline, controller.signal, () => 0);
      };
      const burst = readClass === 'critical' || readClass === 'reconciliation' ? 25 : 20;
      let burstCompleted = false;
      const sends = Promise.allSettled([
        (async () => {
          for (let i = 0; i < burst; i++) await run();
          burstCompleted = true;
        })(),
      ]);
      await vi.advanceTimersByTimeAsync(0);
      expect(burstCompleted).toBe(true);
      expect(await sends).toMatchObject([{ status: 'fulfilled' }]);
      let completed = false;
      const waiting = run().then(() => {
        completed = true;
      });
      await vi.advanceTimersByTimeAsync(2399);
      expect(completed).toBe(false);
      await vi.advanceTimersByTimeAsync(1);
      await waiting;
      expect(completed).toBe(true);
    },
  );

  it('rejoins shared storage after cooldown and retains a known 429 pause', async () => {
    vi.spyOn(console, 'warn').mockImplementation(() => undefined);
    const storage = new budgetModule.IndexedInfoBudgetStorage(factory);
    const budget = new budgetModule.HyperliquidInfoBudget(storage);
    await budget.response(ORIGIN, 200, '');
    const original = IDBDatabase.prototype.transaction;
    const unavailable = vi.spyOn(IDBDatabase.prototype, 'transaction').mockImplementation(() => {
      throw new DOMException('closed', 'InvalidStateError');
    });
    let initialSent = false;
    const first = Promise.allSettled([
      admit(budget, ORIGIN, ORDERS, Date.now() + 20_000, signal()).then(() => {
        initialSent = true;
      }),
    ]);
    await flushDatabase();
    await vi.advanceTimersByTimeAsync(0);
    expect(initialSent).toBe(true);
    await first;
    await budget.response(ORIGIN, 429, 'Retry-After: 45');
    await expect(admit(budget, ORIGIN, ORDERS, Date.now() + 20_000, signal())).rejects.toMatchObject({
      code: 'HTTP_PREPARATION_TIMED_OUT',
    });
    unavailable.mockImplementation(original);
    await vi.advanceTimersByTimeAsync(30_000);
    await expect(admit(budget, ORIGIN, ORDERS, Date.now() + 1000, signal())).rejects.toMatchObject({
      code: 'HTTP_PREPARATION_TIMED_OUT',
    });
    await vi.advanceTimersByTimeAsync(16_200);
    await admit(budget, ORIGIN, ORDERS, Date.now() + 20_000, signal());
    expect(unavailable).toHaveBeenCalled();
  });
});

it('tops up a shared reservation if storage degrades immediately before handoff', async () => {
  await client().response(ORIGIN, 200, '');
  const now = Date.now();
  const state = { nextAt: now + 58_800, pausedUntil: 0, generation: 0, failures: 0 };
  let updates = 0;
  const storage = {
    update: async <T>(_key: string, change: (draft: typeof state, shared: boolean) => T) =>
      change(state, ++updates === 1),
  };
  const budget = new budgetModule.HyperliquidInfoBudget(storage);
  const send = vi.fn(() => 0);
  const result = budget.admitDispatch(
    ORIGIN,
    { generation: 0, weight: 20, readClass: 'critical' },
    now + 20_000,
    signal(),
    send,
  );
  await vi.advanceTimersByTimeAsync(1199);
  expect(send).not.toHaveBeenCalled();
  await vi.advanceTimersByTimeAsync(1);
  await result;
  expect(send).toHaveBeenCalledOnce();
});

it('keeps public-only candle preparation off private proxy authorization and tunnel readiness', async () => {
  vi.stubGlobal('indexedDB', factory);
  const { createEngineTransport } = await import('./engine');
  const { directTransport } = await import('./direct');
  const privateRoute = vi.fn(() => {
    throw new Error('Private route must not prepare public candles');
  });
  const send = vi.fn(async () => ({ status: 200, headers: '', body: new Uint8Array() }));
  const prepare = vi.fn((_packet: { url: string }) => ({ bind: () => ({ ready: () => true, dispatch: send }) }));
  const transport = createEngineTransport(
    'hyperliquid',
    {
      key: '',
      secret: '',
      publicDataOnly: true,
      tealstreetProxy: true,
      tealstreetTunnel: true,
      transport: { ...directTransport, name: 'route-boundary', rest: privateRoute, restAdapter: privateRoute },
    },
    { ...nativeTransport(), request: send, prepareHttp: prepare },
  );
  const packet = {
    request: 1,
    method: 'POST' as const,
    url: ORIGIN + '/info',
    headers: {},
    body: JSON.stringify({ type: 'candleSnapshot' }),
    timeoutMs: 20_000,
  };
  try {
    await (await transport.prepareHttp!({ ...packet, intent: 'public' })).bind(packet).dispatch();
    expect(send).toHaveBeenCalledOnce();
    expect(prepare.mock.calls[0][0].url).toBe(ORIGIN + '/info');
    expect(privateRoute).not.toHaveBeenCalled();
  } finally {
    transport.dispose?.();
  }
});

it('keeps two actual candle transports available when an established IndexedDB stops responding', async () => {
  vi.spyOn(console, 'warn').mockImplementation(() => undefined);
  vi.stubGlobal('indexedDB', factory);
  const { createEngineTransport } = await import('./engine');
  const send = vi.fn(async () => ({ status: 200, headers: '', body: new Uint8Array() }));
  const transport = createEngineTransport(
    'hyperliquid',
    { key: '', secret: '', publicDataOnly: true },
    {
      ...nativeTransport(),
      request: send,
      prepareHttp: () => ({ bind: () => ({ ready: () => true, dispatch: send }) }),
    },
  );
  const packet = {
    request: 1,
    method: 'POST' as const,
    url: ORIGIN + '/info',
    headers: {},
    body: JSON.stringify({ type: 'candleSnapshot' }),
    timeoutMs: 20_000,
  };
  const run = async (request: number) =>
    (await transport.prepareHttp!({ ...packet, request, intent: 'public' })).bind({ ...packet, request }).dispatch();
  try {
    await run(1);
    const stalled = { objectStore: () => ({ get: () => ({}) }), abort: vi.fn() };
    vi.spyOn(IDBDatabase.prototype, 'transaction').mockReturnValue(stalled as unknown as IDBTransaction);
    const pending = Promise.allSettled([run(2), run(3)]);
    await flushDatabase();
    await vi.advanceTimersByTimeAsync(1500);
    await flushDatabase();
    await vi.advanceTimersByTimeAsync(1500);
    await flushDatabase();
    expect(send).toHaveBeenCalledTimes(3);
    expect(await pending).toMatchObject([{ status: 'fulfilled' }, { status: 'fulfilled' }]);
  } finally {
    transport.dispose?.();
  }
});

describe('resolved Hyperliquid egress admission', () => {
  const setup = async () => {
    vi.stubGlobal('indexedDB', factory);
    const { createEngineTransport } = await import('./engine');
    const { directTransport } = await import('./direct');
    const tunnel = await import('@tealstreet/http-over-websocket/src/tunnelEndpoints');
    tunnel.resetTunnelLane(true);
    const directSend = vi.fn(async () => ({ status: 200, headers: '', body: new Uint8Array() }));
    const fleetSend = vi.fn(async (config: InternalAxiosRequestConfig = { headers: new AxiosHeaders() }) => ({
      status: 200,
      statusText: 'OK',
      headers: {},
      data: '[]',
      config,
    }));
    const adapter = Object.assign(fleetSend, {
      prepare: async () => ({ bind: () => ({ ready: () => true, dispatch: fleetSend }) }),
    });
    const provider = {
      ...directTransport,
      name: 'egress-budget-fixture',
      publicCorsProxy: () => 'https://fsjnpy143pbz-tyo.tealstreet.io/api/public_proxy',
      rest: ({ options }: { options: { overrideProxyUrl?: string; tealstreetProxy?: boolean } }) => {
        const prefix =
          options.overrideProxyUrl ||
          (options.tealstreetProxy ? 'https://fsjnpy143pbz-tyo.tealstreet.io/api/exchange_proxy' : '');
        return { baseURL: prefix ? `${prefix}/${ORIGIN}` : ORIGIN, proxied: Boolean(prefix) };
      },
      restAdapter: ({ options }: { options: { tealstreetTunnel?: boolean } }) =>
        options.tealstreetTunnel === false ? undefined : adapter,
      wsRouting: (_exchange: unknown, options: { proxyRegion?: string }) => ({
        restProxyHost: '',
        howEgress: options.proxyRegion === 'SG' ? ('sg' as const) : ('jp' as const),
        wsPublic: { tunnelled: false },
        wsPrivate: { tunnelled: false },
        wsBusiness: { tunnelled: false },
      }),
    };
    const raw = {
      ...nativeTransport(),
      request: directSend,
      prepareHttp: () => ({ bind: () => ({ ready: () => true, dispatch: directSend }) }),
    };
    const create = (
      publicDataOnly: boolean,
      options: { proxyRegion?: string; tealstreetTunnel?: boolean; overrideProxyUrl?: string } = {},
    ) =>
      createEngineTransport(
        'hyperliquid',
        {
          key: '',
          secret: '',
          publicDataOnly,
          transport: provider,
          ...options,
        },
        raw,
      );
    const run = async (
      transport: ReturnType<typeof create>,
      publicRead: boolean,
      timeoutMs = 20_000,
      signal?: AbortSignal,
      route?: 'publicCors',
    ) => {
      const packet = {
        request: 1,
        method: 'POST' as const,
        url: ORIGIN + '/info',
        headers: {},
        body: JSON.stringify({ type: publicRead ? 'candleSnapshot' : 'frontendOpenOrders' }),
        timeoutMs,
        readClass: publicRead ? ('bulk' as const) : ('reconciliation' as const),
        signal,
        route,
      };
      const prepared = await transport.prepareHttp!({ ...packet, intent: publicRead ? 'public' : 'primary' });
      return prepared.bind(packet).dispatch();
    };
    return { create, run, directSend, fleetSend, tunnel };
  };

  it('admits direct public candles immediately after the account fleet exhausts its bucket', async () => {
    const { create, run, directSend, fleetSend, tunnel } = await setup();
    const account = create(false);
    const chart = create(true);
    try {
      for (let i = 0; i < 50; i++) await run(account, false);
      expect(fleetSend).toHaveBeenCalledTimes(50);
      let completed = false;
      const candles = Promise.allSettled([run(chart, true, 1000), run(chart, true, 1000)]).then((rows) => {
        completed = rows.every((row) => row.status === 'fulfilled');
      });
      await flushDatabase();
      await vi.advanceTimersByTimeAsync(0);
      await flushDatabase();
      expect(completed).toBe(true);
      expect(directSend).toHaveBeenCalledTimes(2);
      await candles;
    } finally {
      account.dispose?.();
      chart.dispose?.();
      tunnel.resetTunnelLane();
    }
  });

  it('keeps separate account instances on the same resolved fleet coordinated', async () => {
    const { create, run, fleetSend, tunnel } = await setup();
    const a = create(false);
    const b = create(false, { proxyRegion: 'SG' });
    try {
      for (let i = 0; i < 50; i++) await run(a, false);
      await expect(run(b, false, 1000)).rejects.toMatchObject({ code: 'HTTP_PREPARATION_TIMED_OUT' });
      expect(fleetSend).toHaveBeenCalledTimes(50);
    } finally {
      a.dispose?.();
      b.dispose?.();
      tunnel.resetTunnelLane();
    }
  });

  it('recovers shared legacy pauses after provisional inheritance during an outage', async () => {
    vi.spyOn(console, 'warn').mockImplementation(() => undefined);
    const shared = client();
    await shared.response(ORIGIN, 429, 'Retry-After: 300');
    let available = false;
    const unstable = {
      open: (name: string, version?: number) => {
        if (!available) throw new DOMException('storage unavailable', 'SecurityError');
        return factory.open(name, version);
      },
    } as IDBFactory;
    const recovering = new budgetModule.HyperliquidInfoBudget(new budgetModule.IndexedInfoBudgetStorage(unstable));
    const fleet = hyperliquidInfoBudgetKey(ORIGIN, 'wss://fsjnpy143pbz-tyo.tealstreet.io/tunnel', true);
    const send = vi.fn(() => 0);
    const dispatch = async (budget: HyperliquidInfoBudget, key: string) => {
      const deadline = Date.now() + 1000;
      const abort = signal();
      const grant = await budget.wait(key, ORDERS, deadline, abort, 'critical');
      return budget.admitDispatch(key, grant, deadline, abort, send);
    };
    await dispatch(recovering, fleet);
    expect(send).toHaveBeenCalledOnce();
    available = true;
    await vi.advanceTimersByTimeAsync(30_001);
    const freshFleet = hyperliquidInfoBudgetKey(ORIGIN, 'wss://other.example.test/tunnel', true);
    await expect(dispatch(shared, freshFleet)).rejects.toMatchObject({ code: 'HTTP_PREPARATION_TIMED_OUT' });
    await expect(dispatch(recovering, fleet)).rejects.toMatchObject({ code: 'HTTP_PREPARATION_TIMED_OUT' });
    expect(send).toHaveBeenCalledOnce();
  });

  it('retries legacy inheritance when the target migration could not commit', async () => {
    vi.spyOn(console, 'warn').mockImplementation(() => undefined);
    const shared = client();
    await shared.response(ORIGIN, 200, '');
    const fleet = hyperliquidInfoBudgetKey(ORIGIN, 'wss://fsjnpy143pbz-tyo.tealstreet.io/tunnel', true);
    const put = IDBObjectStore.prototype.put;
    let failTarget = true;
    vi.spyOn(IDBObjectStore.prototype, 'put').mockImplementation(function (this: IDBObjectStore, value, key) {
      if (key === fleet && failTarget) {
        failTarget = false;
        throw new DOMException('target migration failed', 'UnknownError');
      }
      return put.call(this, value, key);
    });
    const recovering = client();
    await admit(recovering, fleet, ORDERS, Date.now() + 1000, signal());
    await shared.response(ORIGIN, 429, 'Retry-After: 300');
    await vi.advanceTimersByTimeAsync(30_001);
    await expect(admit(recovering, fleet, ORDERS, Date.now() + 1000, signal())).rejects.toMatchObject({
      code: 'HTTP_PREPARATION_TIMED_OUT',
    });
  });

  it('finishes durable inheritance when storage recovers between migration and admission', async () => {
    vi.spyOn(console, 'warn').mockImplementation(() => undefined);
    await client().response(ORIGIN, 429, 'Retry-After: 300');
    const fleet = hyperliquidInfoBudgetKey(ORIGIN, 'wss://fsjnpy143pbz-tyo.tealstreet.io/tunnel', true);
    let available = false;
    const unstable = {
      open: (name: string, version?: number) => {
        if (!available) throw new DOMException('storage unavailable', 'SecurityError');
        return factory.open(name, version);
      },
    } as IDBFactory;
    const backend = new budgetModule.IndexedInfoBudgetStorage(unstable);
    const update = backend.update.bind(backend);
    let recoverAfterMigration = true;
    vi.spyOn(backend, 'update').mockImplementation(async (key, change) => {
      const result = await update(key, change);
      if (key === fleet && recoverAfterMigration) {
        recoverAfterMigration = false;
        available = true;
        vi.setSystemTime(Date.now() + 30_001);
      }
      return result;
    });
    const recovering = new budgetModule.HyperliquidInfoBudget(backend);
    const send = vi.fn(() => 0);
    const deadline = Date.now() + 60_000;
    const dispatched = recovering.wait(fleet, ORDERS, deadline, signal(), 'critical').then((grant) =>
      recovering.admitDispatch(fleet, grant, deadline, signal(), send),
    );
    await expect(dispatched).rejects.toMatchObject({ code: 'HTTP_PREPARATION_TIMED_OUT' });
    expect(send).not.toHaveBeenCalled();
  });

  it('holds legacy unscoped 429 evidence when introducing a fleet bucket', async () => {
    const { create, run, fleetSend, tunnel } = await setup();
    await client().response(ORIGIN, 429, 'Retry-After: 45');
    const account = create(false);
    try {
      await expect(run(account, false, 1000)).rejects.toMatchObject({ code: 'HTTP_PREPARATION_TIMED_OUT' });
      expect(fleetSend).not.toHaveBeenCalled();
    } finally {
      account.dispose?.();
      tunnel.resetTunnelLane();
    }
  });

  it('does not spread a fleet 429 pause to direct public candles', async () => {
    const { create, run, directSend, fleetSend, tunnel } = await setup();
    fleetSend.mockResolvedValue({
      status: 429,
      statusText: 'limited',
      headers: { 'retry-after': '45' },
      data: '[]',
      config: { headers: new AxiosHeaders() },
    });
    const account = create(false);
    const chart = create(true);
    try {
      await run(account, false);
      await expect(run(account, false, 1000)).rejects.toMatchObject({ code: 'HTTP_PREPARATION_TIMED_OUT' });
      await run(chart, true, 1000);
      expect(directSend).toHaveBeenCalledOnce();
      expect(fleetSend).toHaveBeenCalledOnce();
    } finally {
      account.dispose?.();
      chart.dispose?.();
      tunnel.resetTunnelLane();
    }
  });

  it('keeps private HTTP fallback in the exhausted tunnel fleet without a resend', async () => {
    const { create, run, directSend, fleetSend, tunnel } = await setup();
    const account = create(false);
    try {
      for (let i = 0; i < 50; i++) await run(account, false);
      const endpoint = tunnel.tunnelEgressUrl('jp');
      for (let i = 0; i < 4; i++) tunnel.noteTunnelDialFailed(tunnel.tunnelDialUrl(endpoint));
      expect(tunnel.tunnelOutage()).toBeDefined();
      await expect(run(account, false, 1000)).rejects.toMatchObject({ code: 'HTTP_PREPARATION_TIMED_OUT' });
      expect(fleetSend).toHaveBeenCalledTimes(50);
      expect(directSend).not.toHaveBeenCalled();
    } finally {
      account.dispose?.();
      tunnel.resetTunnelLane();
    }
  });

  it('keeps distinct resolved tunnel endpoints in independent fleet buckets', async () => {
    const { create, run, fleetSend, tunnel } = await setup();
    const endpoints = { jp: tunnel.tunnelEgressUrl('jp'), sg: tunnel.tunnelEgressUrl('sg') };
    tunnel.setTunnelEgressUrls({ jp: 'wss://tokyo.example.test/tunnel', sg: 'wss://singapore.example.test/tunnel' });
    const a = create(false);
    const b = create(false, { proxyRegion: 'SG' });
    try {
      for (let i = 0; i < 50; i++) await run(a, false);
      await run(b, false, 1000);
      expect(fleetSend).toHaveBeenCalledTimes(51);
    } finally {
      a.dispose?.();
      b.dispose?.();
      tunnel.setTunnelEgressUrls(endpoints);
      tunnel.resetTunnelLane();
    }
  });

  it('inherits old unscoped debt without copying it repeatedly or weakening later pauses', async () => {
    const { create, run, fleetSend, tunnel } = await setup();
    await spend(client(), 1000);
    const account = create(false);
    try {
      await expect(run(account, false, 1000)).rejects.toMatchObject({ code: 'HTTP_PREPARATION_TIMED_OUT' });
      expect(fleetSend).not.toHaveBeenCalled();
      await vi.advanceTimersByTimeAsync(1200);
      await run(account, false, 1000);
      expect(fleetSend).toHaveBeenCalledOnce();
    } finally {
      account.dispose?.();
      tunnel.resetTunnelLane();
    }
  });

  it('keeps a public CORS candle on its own egress after the private fleet is exhausted', async () => {
    const { create, run, directSend, tunnel } = await setup();
    const account = create(false);
    const chart = create(true);
    try {
      for (let i = 0; i < 50; i++) await run(account, false);
      await run(chart, true, 1000, undefined, 'publicCors');
      expect(directSend).toHaveBeenCalledOnce();
    } finally {
      account.dispose?.();
      chart.dispose?.();
      tunnel.resetTunnelLane();
    }
  });

  it('cancels an unsent fleet waiter without a later resend or renewed deadline', async () => {
    const { create, run, fleetSend, tunnel } = await setup();
    const account = create(false);
    try {
      for (let i = 0; i < 50; i++) await run(account, false);
      const controller = new AbortController();
      const pending = Promise.allSettled([run(account, false, 20_000, controller.signal)]);
      await flushDatabase();
      controller.abort(new Error('cancelled'));
      await pending;
      await vi.advanceTimersByTimeAsync(60_000);
      await flushDatabase();
      expect(fleetSend).toHaveBeenCalledTimes(50);
      await expect(run(account, false, 1, controller.signal)).rejects.toThrow('cancelled');
      expect(fleetSend).toHaveBeenCalledTimes(50);
    } finally {
      account.dispose?.();
      tunnel.resetTunnelLane();
    }
  });

  it('does not re-import later direct debt when a new account joins an existing fleet bucket', async () => {
    const { create, run, fleetSend, tunnel } = await setup();
    const first = create(false);
    const peer = create(false);
    try {
      await run(first, false);
      await client().response(ORIGIN, 429, 'Retry-After: 45');
      await run(peer, false, 1000);
      expect(fleetSend).toHaveBeenCalledTimes(2);
    } finally {
      first.dispose?.();
      peer.dispose?.();
      tunnel.resetTunnelLane();
    }
  });
});

it('keys resolved routes without account, credential, query or lane-port identity', () => {
  const direct = hyperliquidInfoBudgetKey(ORIGIN);
  expect(direct).toBe(ORIGIN);
  const privateFleet = hyperliquidInfoBudgetKey(ORIGIN, 'wss://fsjnpy143pbz-tyo.tealstreet.io/tunnel', true);
  expect(hyperliquidInfoBudgetKey(ORIGIN, 'wss://fsjnpy143pbz-tyo.tealstreet.io:8443/tunnel-staging', true)).toBe(
    privateFleet,
  );
  expect(hyperliquidInfoBudgetKey(ORIGIN, 'https://fsjnpy143pbz-tyo.tealstreet.io/api/exchange_proxy/')).toBe(
    privateFleet,
  );
  expect(hyperliquidInfoBudgetKey(ORIGIN, 'https://fsjnpy143pbz-tyo.tealstreet.io/api/public_proxy')).not.toBe(
    privateFleet,
  );
  const custom = hyperliquidInfoBudgetKey(
    ORIGIN,
    'https://PRIVATE_USER:PRIVATE_PASS@proxy.example.test/path?credential=PRIVATE_QUERY',
  );
  expect(custom).toBe(hyperliquidInfoBudgetKey(ORIGIN, 'https://proxy.example.test/another-path'));
  expect(custom).not.toContain('PRIVATE');
  expect(custom).not.toBe(direct);
});
