import type { HttpPacket, HttpResponse, Transport } from '@tealstreet/safe-cex-wasm/host';

import { setImmediate } from 'node:timers';

import { IDBDatabase, IDBFactory } from 'fake-indexeddb';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';

import { binanceCataloguePolicy } from './binanceCatalogue';
import { SharedCatalogue } from './sharedCatalogue';

const ROUTE = { network: 'mainnet' as const, route: 'direct' };
const URL = 'https://fapi.binance.com/fapi/v1/exchangeInfo';
const request = () => ({ method: 'GET', url: URL, headers: {}, intent: 'public' as const });
const bytes = new TextEncoder().encode(
  JSON.stringify({
    symbols: [
      {
        symbol: 'BTCUSDT',
        baseAsset: 'BTC',
        quoteAsset: 'USDT',
        marginAsset: 'USDT',
        status: 'TRADING',
        contractType: 'PERPETUAL',
        filters: [
          { filterType: 'PRICE_FILTER', tickSize: '0.1' },
          { filterType: 'LOT_SIZE', stepSize: '0.001' },
        ],
      },
    ],
  }),
);
const response = (status = 200, headers = ''): HttpResponse => ({ status, headers, body: bytes });
const flush = async () => {
  for (let i = 0; i < 16; i++) await new Promise<void>((resolve) => setImmediate(resolve));
};
let factory: IDBFactory;
const client = () => new SharedCatalogue(binanceCataloguePolicy, factory);
const advance = async (ms: number) => {
  for (let step = 0; step < ms; step += 100) {
    await flush();
    await vi.advanceTimersByTimeAsync(Math.min(100, ms - step));
  }
  await flush();
};
const prepare = (cache: SharedCatalogue, signal = new AbortController().signal, deadline = Date.now() + 20_000) =>
  cache.prepare(request(), signal, deadline, ROUTE);
beforeEach(() => {
  factory = new IDBFactory();
  vi.useFakeTimers({ toFake: ['Date', 'setTimeout', 'clearTimeout'] });
  vi.setSystemTime(1_000_000);
});
afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

it('separates exact query, network, endpoint and resolved route', async () => {
  const a = client();
  await (await prepare(a))!.complete(response());
  await flush();
  for (const [packet, route] of [
    [{ ...request(), url: URL + '?symbol=BTCUSDT' }, ROUTE],
    [{ ...request(), url: URL + '?symbols=%5B%22BTCUSDT%22%5D' }, ROUTE],
    [{ ...request(), url: 'https://demo-fapi.binance.com/fapi/v1/exchangeInfo' }, ROUTE],
    [{ ...request(), url: 'https://dapi.binance.com/dapi/v1/exchangeInfo' }, ROUTE],
    [request(), { ...ROUTE, network: 'testnet' as const }],
    [request(), { ...ROUTE, route: 'how:jp' }],
    [request(), { ...ROUTE, route: 'proxy:https://relay.invalid/api/exchange_proxy/' }],
  ] as const) {
    const read = await a.prepare(packet, new AbortController().signal, Date.now() + 20_000, route);
    expect(read).toBeDefined();
    expect(read!.cached).toBeUndefined();
    read!.cancel();
    await flush();
  }
  expect((await prepare(client()))?.cached?.body).toEqual(bytes);
});

it('excludes private/account requests, writes, credentials, tokens, candles and unapproved endpoints', async () => {
  const a = client();
  const excluded: Array<HttpPacket & { intent?: string }> = [
    { ...request(), intent: 'primary' },
    { ...request(), intent: 'secondary' },
    { ...request(), intent: undefined },
    { ...request(), method: 'POST' },
    { ...request(), body: '{}' },
    { ...request(), headers: { 'X-MBX-APIKEY': 'credential' } },
    { ...request(), headers: { Authorization: 'credential' } },
    { ...request(), url: URL + '?signature=secret' },
    { ...request(), url: URL + '?timestamp=1234' },
    { ...request(), url: URL + '?recvWindow=20000' },
    { ...request(), url: 'https://fapi.binance.com/fapi/v1/openOrders' },
    { ...request(), url: 'https://fapi.binance.com/fapi/v1/listenKey' },
    { ...request(), url: 'https://fapi.binance.com/fapi/v1/klines' },
    { ...request(), url: 'https://user:key@fapi.binance.com/fapi/v1/exchangeInfo' },
    { ...request(), url: 'https://unapproved.invalid/fapi/v1/exchangeInfo' },
  ];
  for (const packet of excluded)
    expect(await a.prepare(packet, new AbortController().signal, Date.now() + 20_000, ROUTE)).toBeUndefined();
  expect(await a.prepare(request(), new AbortController().signal, Date.now() + 20_000)).toBeUndefined();
});

it('never bypasses an active leader after the HL three-second fallback bound', async () => {
  const leader = await prepare(client());
  const wait = prepare(client(), undefined, Date.now() + 4500);
  const observed = Promise.allSettled([wait]);
  await flush();
  for (let i = 0; i < 45; i++) {
    await advance(100);
  }
  expect((await observed)[0]).toMatchObject({ status: 'rejected', reason: { code: 'HTTP_PREPARATION_TIMED_OUT' } });
  await leader!.complete(response());
  expect((await prepare(client()))?.cached).toBeDefined();
});

it.each(['network', 'http', 'invalid'])('a %s failure installs cooldown before followers can elect', async (kind) => {
  const leader = await prepare(client());
  const follower = prepare(client());
  let settled = false;
  void follower.then(() => {
    settled = true;
  });
  await flush();
  if (kind === 'network') await leader!.failed!();
  else
    await leader!.complete(
      kind === 'http' ? response(503) : { ...response(), body: new TextEncoder().encode('{"symbols":[{}]}') },
    );
  await advance(900);
  expect(settled).toBe(false);
  await advance(100);
  const next = await follower;
  expect(next).toBeDefined();
  expect(next!.cached).toBeUndefined();
  await next!.complete(response());
  expect((await prepare(client()))?.cached?.body).toEqual(bytes);
});

it('a late failed owner cannot replace a newer success or remove its lease', async () => {
  const old = await prepare(client(), undefined, Date.now() + 100_000);
  await vi.advanceTimersByTimeAsync(26_001);
  const next = await prepare(client());
  await next!.complete(response());
  await flush();
  await old!.failed!('retry-after: 300');
  old!.cancel();
  expect((await prepare(client()))?.cached?.body).toEqual(bytes);
});

it('caps Retry-After at five minutes and preserves original waiter deadlines', async () => {
  const leader = await prepare(client());
  await leader!.complete(response(429, 'Retry-After: 999999'));
  const waiting = Promise.allSettled([prepare(client(), undefined, Date.now() + 1000)]);
  for (let i = 0; i < 10; i++) {
    await advance(100);
  }
  expect((await waiting)[0]).toMatchObject({ status: 'rejected', reason: { code: 'HTTP_PREPARATION_TIMED_OUT' } });
  await vi.advanceTimersByTimeAsync(299_000);
  expect((await prepare(client()))?.cached).toBeUndefined();
});

it('a follower abort does not erase its peer cooldown', async () => {
  await (await prepare(client()))!.failed!();
  const stop = new AbortController();
  const cancelled = Promise.allSettled([prepare(client(), stop.signal)]);
  await flush();
  stop.abort();
  expect((await cancelled)[0].status).toBe('rejected');
  const waiting = Promise.allSettled([prepare(client(), undefined, Date.now() + 500)]);
  for (let i = 0; i < 5; i++) {
    await advance(100);
  }
  expect((await waiting)[0]).toMatchObject({ status: 'rejected', reason: { code: 'HTTP_PREPARATION_TIMED_OUT' } });
});

it('unavailable storage from the start preserves independent discovery', async () => {
  vi.spyOn(factory, 'open').mockImplementation(() => {
    throw new DOMException('unavailable', 'SecurityError');
  });
  expect(await prepare(client())).toBeUndefined();
});

it('copies cache bytes, keeps exact invalid response bytes and rejects rollback/expired successes', async () => {
  const a = client();
  const leader = await prepare(a);
  const returned = await leader!.complete(response());
  (returned.body as Uint8Array).fill(0);
  await flush();
  expect((await prepare(client()))?.cached?.body).toEqual(bytes);
  await vi.advanceTimersByTimeAsync(5001);
  const fresh = await prepare(client());
  const bad = new TextEncoder().encode('{"symbols":[{}]}');
  const answer = await fresh!.complete({ ...response(), body: bad });
  expect(answer.body).toEqual(bad);
  await advance(1001);
  await (await prepare(client()))!.complete(response());
  await flush();
  vi.setSystemTime(900_000);
  expect((await prepare(client()))?.cached).toBeUndefined();
});

it('storage lost after observing a peer lease refuses rather than bypassing it', async () => {
  const leader = await prepare(client());
  const follower = client();
  const waiting = Promise.allSettled([prepare(follower)]);
  await flush();
  vi.spyOn(IDBDatabase.prototype, 'transaction').mockImplementation(() => {
    throw new DOMException('storage lost', 'InvalidStateError');
  });
  await advance(100);
  expect((await waiting)[0]).toMatchObject({ status: 'rejected', reason: { code: 'HTTP_PREPARATION_NETWORK' } });
  vi.restoreAllMocks();
  await leader!.complete(response());
});

it('caches spot and inverse catalogues with the native contractStatus alias', () => {
  const row = JSON.parse(new TextDecoder().decode(bytes)).symbols[0];
  const encode = (row: unknown) => new TextEncoder().encode(JSON.stringify({ symbols: [row] }));
  const inverse = { ...row, symbol: 'BTCUSD_PERP', quoteAsset: 'USD', marginAsset: 'BTC', contractStatus: 'TRADING' };
  delete inverse.status;
  const inverseKey = binanceCataloguePolicy.keyFor(
    { ...request(), url: 'https://dapi.binance.com/dapi/v1/exchangeInfo' },
    ROUTE,
  )!;
  expect(binanceCataloguePolicy.valid(inverseKey, encode(inverse))).toBe(true);
  expect(binanceCataloguePolicy.valid(inverseKey, encode({ ...inverse, status: 'TRADING' }))).toBe(false);
  const spotKey = binanceCataloguePolicy.keyFor(
    { ...request(), url: 'https://api.binance.com/api/v3/exchangeInfo' },
    ROUTE,
  )!;
  const spot = { ...row };
  delete spot.contractType;
  delete spot.marginAsset;
  expect(binanceCataloguePolicy.valid(spotKey, encode(spot))).toBe(true);
});

it('validates the full bounded reply and accepts eight MiB exactly', () => {
  const key = binanceCataloguePolicy.keyFor(request(), ROUTE)!;
  const limit = new Uint8Array(8 * 1024 * 1024).fill(32);
  limit.set(bytes);
  expect(binanceCataloguePolicy.valid(key, limit)).toBe(true);
  expect(binanceCataloguePolicy.valid(key, new Uint8Array(limit.length + 1))).toBe(false);
  const valid = JSON.parse(new TextDecoder().decode(bytes));
  for (const row of [
    {},
    { ...valid.symbols[0], status: {} },
    { ...valid.symbols[0], baseAssetPrecision: '8' },
    { ...valid.symbols[0], quoteAssetPrecision: 1.5 },
    { ...valid.symbols[0], marginAsset: '' },
    { ...valid.symbols[0], filters: [{ filterType: 'PRICE_FILTER', tickSize: 'NaN' }] },
  ])
    expect(binanceCataloguePolicy.valid(key, new TextEncoder().encode(JSON.stringify({ symbols: [row] })))).toBe(false);
});

it.each([
  ['expired', 'ready'],
  ['expired', 'dispatch'],
  ['rollback', 'ready'],
  ['rollback', 'dispatch'],
])('refuses a %s lease at the final %s boundary without HTTP', async (kind, boundary) => {
  vi.stubGlobal('indexedDB', factory);
  vi.resetModules();
  const { createEngineTransport } = await import('./engine');
  const { prepareRecordedHttp } = await import('../__tests__/support/managedHttp');
  const send = vi.fn(async () => response());
  const raw: Transport = {
    now: Date.now,
    setTimeout,
    clearTimeout: (timer) => clearTimeout(timer as ReturnType<typeof setTimeout>),
    prepareHttp: prepareRecordedHttp,
    request: send,
    connect: () => ({ send() {}, close() {} }),
  };
  const transport = createEngineTransport('binance', { key: '', secret: '' }, raw);
  const packet = { ...request(), request: 1, route: 'direct' as const, timeoutMs: 60_000 };
  try {
    const read = await transport.prepareHttp!(packet);
    const bound = read.bind(packet);
    expect(bound.ready()).toBe(true);
    vi.setSystemTime(kind === 'expired' ? Date.now() + 25_001 : Date.now() - 1);
    expect(() => (boundary === 'ready' ? bound.ready() : bound.dispatch())).toThrow(
      expect.objectContaining({ code: 'HTTP_PREPARATION_STALE' }),
    );
    expect(send).not.toHaveBeenCalled();
  } finally {
    transport.dispose?.();
    await flush();
  }
});

it.each(['lease', 'cooldown'])(
  'review strict %s survives clock rollback without a second physical request',
  async (phase) => {
    vi.stubGlobal('indexedDB', factory);
    vi.resetModules();
    const { createEngineTransport } = await import('./engine');
    const { prepareRecordedHttp } = await import('../__tests__/support/managedHttp');
    const send = vi.fn(async () => (phase === 'cooldown' ? response(503) : new Promise<HttpResponse>(() => {})));
    const raw: Transport = {
      now: Date.now,
      setTimeout,
      clearTimeout: (t) => clearTimeout(t as ReturnType<typeof setTimeout>),
      prepareHttp: prepareRecordedHttp,
      request: send,
      connect: () => ({ send() {}, close() {} }),
    };
    const a = createEngineTransport('binance', { key: '', secret: '' }, raw);
    const b = createEngineTransport('binance', { key: '', secret: '' }, raw);
    const packet = (id: number) => ({ ...request(), request: id, route: 'direct' as const, timeoutMs: 20000 });
    const pending: Promise<unknown>[] = [];
    try {
      const p = packet(1);
      const leader = await a.prepareHttp!(p);
      const physical = leader.bind(p).dispatch();
      const first = Promise.allSettled([physical]);
      pending.push(first);
      await flush();
      expect(send).toHaveBeenCalledTimes(1);
      if (phase === 'cooldown') {
        await first;
        await flush();
      }
      vi.setSystemTime(Date.now() - 1);
      let follower: Awaited<ReturnType<NonNullable<Transport['prepareHttp']>>> | undefined;
      const q = packet(2);
      pending.push(
        Promise.allSettled([
          Promise.resolve(b.prepareHttp!(q)).then((p) => {
            follower = p;
          }),
        ]),
      );
      await flush();
      if (follower) pending.push(Promise.allSettled([follower.bind(q).dispatch()]));
      await flush();
      expect(send, 'rollback must not bypass the outstanding lease/cooldown').toHaveBeenCalledTimes(1);
    } finally {
      a.dispose?.();
      b.dispose?.();
      await flush();
      await Promise.all(pending);
    }
  },
);

it('review physically dispatched timeout installs cooldown before a follower sends', async () => {
  vi.stubGlobal('indexedDB', factory);
  vi.resetModules();
  const { createEngineTransport } = await import('./engine');
  const { prepareRecordedHttp } = await import('../__tests__/support/managedHttp');
  const send = vi.fn(async () => new Promise<HttpResponse>(() => {}));
  const raw: Transport = {
    now: Date.now,
    setTimeout,
    clearTimeout: (t) => clearTimeout(t as ReturnType<typeof setTimeout>),
    prepareHttp: prepareRecordedHttp,
    request: send,
    connect: () => ({ send() {}, close() {} }),
  };
  const a = createEngineTransport('binance', { key: '', secret: '' }, raw),
    b = createEngineTransport('binance', { key: '', secret: '' }, raw);
  const pending: Promise<unknown>[] = [];
  try {
    const p = { ...request(), request: 1, route: 'direct' as const, timeoutMs: 500 };
    pending.push(Promise.allSettled([(await a.prepareHttp!(p)).bind(p).dispatch()]));
    await flush();
    expect(send).toHaveBeenCalledTimes(1);
    const q = { ...request(), request: 2, route: 'direct' as const, timeoutMs: 5000 };
    pending.push(Promise.allSettled([Promise.resolve(b.prepareHttp!(q)).then((p) => p.bind(q).dispatch())]));
    for (let n = 0; n < 7; n++) {
      await advance(100);
    }
    expect(send, 'timed-out physical leader must impose the first 1s shared failure cooldown').toHaveBeenCalledTimes(1);
  } finally {
    a.dispose?.();
    b.dispose?.();
    await flush();
    await Promise.all(pending);
  }
});

it.each(['never-dispatched', 'user-cancelled'])(
  'does not publish a failure cooldown for a %s preparation',
  async (mode) => {
    vi.stubGlobal('indexedDB', factory);
    vi.resetModules();
    const { createEngineTransport } = await import('./engine');
    const { prepareRecordedHttp } = await import('../__tests__/support/managedHttp');
    const send = vi.fn(async () =>
      mode === 'user-cancelled' && send.mock.calls.length === 1 ? new Promise<HttpResponse>(() => {}) : response(),
    );
    const raw: Transport = {
      now: Date.now,
      setTimeout,
      clearTimeout: (timer) => clearTimeout(timer as ReturnType<typeof setTimeout>),
      prepareHttp: prepareRecordedHttp,
      request: send,
      connect: () => ({ send() {}, close() {} }),
    };
    const a = createEngineTransport('binance', { key: '', secret: '' }, raw);
    const b = createEngineTransport('binance', { key: '', secret: '' }, raw);
    const pending: Promise<unknown>[] = [];
    try {
      const p = { ...request(), request: 1, route: 'direct' as const, timeoutMs: 500 };
      const read = await a.prepareHttp!(p);
      const bound = read.bind(p);
      if (mode === 'user-cancelled') pending.push(Promise.allSettled([bound.dispatch()]));
      const q = { ...p, request: 2, timeoutMs: 5000 };
      pending.push(Promise.allSettled([Promise.resolve(b.prepareHttp!(q)).then((next) => next.bind(q).dispatch())]));
      await advance(100);
      if (mode === 'user-cancelled') bound.cancel?.();
      await advance(600);
      expect(send).toHaveBeenCalledTimes(mode === 'never-dispatched' ? 1 : 2);
      expect(await pending.at(-1)!).toMatchObject([{ status: 'fulfilled', value: { status: 200 } }]);
    } finally {
      a.dispose?.();
      b.dispose?.();
      await flush();
      await Promise.all(pending);
    }
  },
);

it('holds an expired peer lease while a delayed owner deadline has not published its failure yet', async () => {
  vi.stubGlobal('indexedDB', factory);
  vi.resetModules();
  const { createEngineTransport } = await import('./engine');
  const { prepareRecordedHttp } = await import('../__tests__/support/managedHttp');
  const send = vi.fn(async () => new Promise<HttpResponse>(() => {}));
  const raw: Transport = {
    now: Date.now,
    setTimeout,
    clearTimeout: (timer) => clearTimeout(timer as ReturnType<typeof setTimeout>),
    prepareHttp: prepareRecordedHttp,
    request: send,
    connect: () => ({ send() {}, close() {} }),
  };
  const a = createEngineTransport('binance', { key: '', secret: '' }, raw);
  const b = createEngineTransport('binance', { key: '', secret: '' }, raw);
  const pending: Promise<unknown>[] = [];
  try {
    const p = { ...request(), request: 1, route: 'direct' as const, timeoutMs: 500 };
    pending.push(Promise.allSettled([(await a.prepareHttp!(p)).bind(p).dispatch()]));
    await flush();
    // Advance the wall clock without running the original owner's timer.
    vi.setSystemTime(Date.now() + 500);
    const q = { ...p, request: 2, timeoutMs: 5000 };
    pending.push(Promise.allSettled([Promise.resolve(b.prepareHttp!(q)).then((next) => next.bind(q).dispatch())]));
    await flush();
    expect(send).toHaveBeenCalledTimes(1);
    await advance(900);
    expect(send).toHaveBeenCalledTimes(1);
    await advance(800);
    expect(send).toHaveBeenCalledTimes(2);
  } finally {
    a.dispose?.();
    b.dispose?.();
    await flush();
    await Promise.all(pending);
  }
});

it('settles the dispatched caller at its deadline without awaiting stalled cooldown storage', async () => {
  vi.stubGlobal('indexedDB', factory);
  vi.resetModules();
  const { createEngineTransport } = await import('./engine');
  const { prepareRecordedHttp } = await import('../__tests__/support/managedHttp');
  const raw: Transport = {
    now: Date.now,
    setTimeout,
    clearTimeout: (timer) => clearTimeout(timer as ReturnType<typeof setTimeout>),
    prepareHttp: prepareRecordedHttp,
    request: () => new Promise<HttpResponse>(() => {}),
    connect: () => ({ send() {}, close() {} }),
  };
  const a = createEngineTransport('binance', { key: '', secret: '' }, raw);
  let observed: unknown;
  try {
    const p = { ...request(), request: 1, route: 'direct' as const, timeoutMs: 500 };
    const caller = Promise.allSettled([(await a.prepareHttp!(p)).bind(p).dispatch()]).then((value) => {
      observed = value;
    });
    await flush();
    vi.spyOn(IDBDatabase.prototype, 'transaction').mockImplementationOnce(
      () =>
        ({
          objectStore: () => ({ get: () => ({}) }),
          abort() {},
        }) as unknown as IDBTransaction,
    );
    await advance(500);
    expect(observed).toMatchObject([{ status: 'rejected', reason: { code: 'HTTP_PREPARATION_TIMED_OUT' } }]);
    await caller;
    await advance(300);
  } finally {
    a.dispose?.();
    await flush();
  }
});

it.each(['network', 'http'])(
  'keeps the original deadline while a %s failure publication is already stalled',
  async (kind) => {
    vi.stubGlobal('indexedDB', factory);
    vi.resetModules();
    const { createEngineTransport } = await import('./engine');
    const { prepareRecordedHttp } = await import('../__tests__/support/managedHttp');
    let resolve!: (response: HttpResponse) => void;
    let reject!: (error: Error) => void;
    const responsePending = new Promise<HttpResponse>((yes, no) => {
      resolve = yes;
      reject = no;
    });
    const raw: Transport = {
      now: Date.now,
      setTimeout,
      clearTimeout: (timer) => clearTimeout(timer as ReturnType<typeof setTimeout>),
      prepareHttp: prepareRecordedHttp,
      request: () => responsePending,
      connect: () => ({ send() {}, close() {} }),
    };
    const a = createEngineTransport('binance', { key: '', secret: '' }, raw);
    let observed: unknown;
    try {
      const p = { ...request(), request: 1, route: 'direct' as const, timeoutMs: 500 };
      const caller = Promise.allSettled([(await a.prepareHttp!(p)).bind(p).dispatch()]).then((value) => {
        observed = value;
      });
      await advance(400);
      vi.spyOn(IDBDatabase.prototype, 'transaction').mockImplementationOnce(
        () =>
          ({
            objectStore: () => ({ get: () => ({}) }),
            abort() {},
          }) as unknown as IDBTransaction,
      );
      if (kind === 'network') reject(new Error('network failed'));
      else resolve(response(503));
      await flush();
      await advance(100);
      expect(observed).toMatchObject([{ status: 'rejected' }]);
      await advance(300);
      await caller;
    } finally {
      a.dispose?.();
      await flush();
    }
  },
);
