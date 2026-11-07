import type { HyperliquidInfoBudget } from './hyperliquidInfoBudget';

import { setImmediate } from 'node:timers';

import { IDBDatabase, IDBFactory, IDBObjectStore } from 'fake-indexeddb';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { nativeTransport } from '@tealstreet/safe-cex-wasm/host';
import { hyperliquidInfoWeight } from './hyperliquidInfoBudget';

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
  it.each([false, true])('allows local dispatch only if IDB never opened in this realm: %s', async (opened) => {
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
      await vi.advanceTimersByTimeAsync(20_000);
      expect(await result).toMatchObject([{ status: 'rejected', reason: { code: 'HTTP_PREPARATION_TIMED_OUT' } }]);
      expect(send).not.toHaveBeenCalled();
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
    expect(await firstOutcome).toMatchObject([{ status: 'rejected', reason: { code: 'HTTP_PREPARATION_TIMED_OUT' } }]);
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
    // No extra local bucket is available after joining the shared coordinator.
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

  it.each([
    'throw',
    'error',
    'blocked',
    'stalled',
    'transactionThrow',
    'transactionError',
    'quota',
    'transactionStalled',
  ])('bounds storage failure %s without leaking details or bypassing an established coordinator', async (failure) => {
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
    expect((await first)[0].status).toBe(established ? 'rejected' : 'fulfilled');
    const next = Promise.allSettled([admit(new Budget(), ORIGIN, ORDERS, Date.now() + 20_000, signal())]);
    await vi.advanceTimersByTimeAsync(20_000);
    expect((await next)[0].status).toBe(established ? 'rejected' : 'fulfilled');
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
