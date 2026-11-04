import type { HttpResponse } from '@tealstreet/safe-cex-wasm/host';

import { setImmediate } from 'node:timers';

import { IDBDatabase, IDBFactory, IDBObjectStore } from 'fake-indexeddb';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';

import { HyperliquidCatalogue } from './hyperliquidCatalogue';

const ORIGIN = 'https://api.hyperliquid.xyz';
const request = (body = '{"type":"perpDexs"}', url = ORIGIN + '/info') => ({
  method: 'POST',
  url,
  headers: { 'Content-Type': 'application/json' },
  body,
  intent: 'public' as const,
});
const response = (body = '[null,{"name":"xyz"}]', status = 200): HttpResponse => ({
  status,
  headers: '',
  body: new TextEncoder().encode(body),
});
const controller = () => new AbortController();
const flush = async () => {
  for (let i = 0; i < 16; i++) await new Promise<void>((resolve) => setImmediate(resolve));
};
let factory: IDBFactory;
beforeEach(() => {
  factory = new IDBFactory();
  vi.useFakeTimers({ toFake: ['Date', 'setTimeout', 'clearTimeout'] });
  vi.setSystemTime(1_000_000);
});
afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});

it('shares byte-exact public catalogue replies across independent worker clients', async () => {
  const a = new HyperliquidCatalogue(factory);
  const b = new HyperliquidCatalogue(factory);
  const leader = await a.prepare(request(), controller().signal, Date.now() + 20_000);
  expect(leader?.cached).toBeUndefined();
  const expected = response();
  await leader!.complete(expected);
  const follower = await b.prepare(request(), controller().signal, Date.now() + 20_000);
  expect(follower?.cached?.body).toEqual(expected.body);
});

it('single-flights a pending catalogue and never puts account reads in the cache', async () => {
  const a = new HyperliquidCatalogue(factory);
  const b = new HyperliquidCatalogue(factory);
  const leader = await a.prepare(request(), controller().signal, Date.now() + 20_000);
  let resolved = false;
  const waiting = b.prepare(request(), controller().signal, Date.now() + 20_000).then((value) => {
    resolved = true;
    return value;
  });
  await flush();
  expect(resolved).toBe(false);
  await leader!.complete(response());
  await vi.advanceTimersByTimeAsync(100);
  expect((await waiting)?.cached).toBeDefined();
  for (const body of [
    '{"type":"userRole","user":"private"}',
    '{"type":"clearinghouseState","user":"private"}',
    '{"type":"frontendOpenOrders","user":"private"}',
    '{"type":"perpDexs","user":"private"}',
  ])
    expect(await a.prepare(request(body), controller().signal, Date.now() + 20_000)).toBeUndefined();
});

it('recovers a dead leader by allowing its own fetch within a bounded wait', async () => {
  const a = new HyperliquidCatalogue(factory);
  const b = new HyperliquidCatalogue(factory);
  const dead = await a.prepare(request(), controller().signal, Date.now() + 20_000);
  const recovering = b.prepare(request(), controller().signal, Date.now() + 20_000);
  for (let i = 0; i < 31; i++) {
    await flush();
    await vi.advanceTimersByTimeAsync(100);
  }
  const replacement = await recovering;
  expect(replacement?.cached).toBeUndefined();
  await replacement!.complete(response('[null,{"name":"new"}]'));
  await dead!.complete(response('[null,{"name":"old"}]'));
  const fresh = await a.prepare(request(), controller().signal, Date.now() + 20_000);
  expect(fresh?.cached?.body).toEqual(response('[null,{"name":"new"}]').body);
});

it('expires a short TTL and keeps mainnet, testnet and dex keys separate', async () => {
  const a = new HyperliquidCatalogue(factory);
  const leader = await a.prepare(request(), controller().signal, Date.now() + 20_000);
  await leader!.complete(response());
  expect(
    (
      await a.prepare(
        request(undefined, 'https://api.hyperliquid-testnet.xyz/info'),
        controller().signal,
        Date.now() + 20_000,
      )
    )?.cached,
  ).toBeUndefined();
  await vi.advanceTimersByTimeAsync(5001);
  expect((await a.prepare(request(), controller().signal, Date.now() + 20_000))?.cached).toBeUndefined();
});

it.each(['invalid', 'http', 'abort'])(
  'invalidates the catalogue on %s failure without stranding a follower',
  async (cause) => {
    const a = new HyperliquidCatalogue(factory);
    const b = new HyperliquidCatalogue(factory);
    const stop = controller();
    const leader = await a.prepare(request(), stop.signal, Date.now() + 20_000);
    if (cause === 'abort') {
      stop.abort();
      leader!.cancel();
    } else
      await leader!.complete(
        response(cause === 'invalid' ? '{"error":"bad catalogue"}' : 'unavailable', cause === 'http' ? 500 : 200),
      );
    const next = await b.prepare(request(), controller().signal, Date.now() + 20_000);
    expect(next?.cached).toBeUndefined();
  },
);

it('fences every old leader on an origin error without touching another endpoint', async () => {
  const a = new HyperliquidCatalogue(factory);
  const b = new HyperliquidCatalogue(factory);
  const old = await a.prepare(request(), controller().signal, Date.now() + 20_000);
  const testnet = request(undefined, 'https://api.hyperliquid-testnet.xyz/info');
  await (await b.prepare(testnet, controller().signal, Date.now() + 20_000))!.complete(response());
  await b.invalidateOrigin(ORIGIN);
  await old!.complete(response());
  expect((await b.prepare(request(), controller().signal, Date.now() + 20_000))?.cached).toBeUndefined();
  expect((await a.prepare(testnet, controller().signal, Date.now() + 20_000))?.cached).toBeDefined();
});

it.each(['throw', 'blocked', 'stalled', 'quota'])(
  'bypasses a %s cache within a bounded time without failing a managed read',
  async (failure) => {
    const openRequest: any = {};
    const open = vi.spyOn(factory, 'open');
    if (failure !== 'quota')
      open.mockImplementation(() => {
        if (failure === 'throw') throw new DOMException('unavailable', 'SecurityError');
        return openRequest;
      });
    else
      vi.spyOn(IDBObjectStore.prototype, 'put').mockImplementationOnce(() => {
        throw new DOMException('full', 'QuotaExceededError');
      });
    const a = new HyperliquidCatalogue(factory);
    const work = a.prepare(request(), controller().signal, Date.now() + 20_000);
    await flush();
    if (failure === 'blocked') openRequest.onblocked();
    await vi.advanceTimersByTimeAsync(251);
    expect(await work).toBeUndefined();
    expect(await a.prepare(request(), controller().signal, Date.now() + 20_000)).toBeUndefined();
    expect(open).toHaveBeenCalledTimes(1);
  },
);

it('keeps a good HTTP answer when cache publication fails', async () => {
  const a = new HyperliquidCatalogue(factory);
  const read = await a.prepare(request(), controller().signal, Date.now() + 20_000);
  vi.spyOn(IDBObjectStore.prototype, 'put').mockImplementationOnce(() => {
    throw new DOMException('full', 'QuotaExceededError');
  });
  const answer = response();
  expect(await read!.complete(answer)).toEqual(answer);
  await vi.advanceTimersByTimeAsync(3001);
  expect(
    (await new HyperliquidCatalogue(factory).prepare(request(), controller().signal, Date.now() + 20_000))?.cached,
  ).toBeUndefined();
});

it('does not cancel a leader when a follower is aborted', async () => {
  const a = new HyperliquidCatalogue(factory);
  const b = new HyperliquidCatalogue(factory);
  const leader = await a.prepare(request(), controller().signal, Date.now() + 20_000);
  const stop = controller();
  const result = Promise.allSettled([b.prepare(request(), stop.signal, Date.now() + 20_000)]);
  await flush();
  stop.abort();
  expect((await result)[0].status).toBe('rejected');
  await leader!.complete(response());
  expect((await b.prepare(request(), controller().signal, Date.now() + 20_000))?.cached).toBeDefined();
});

it('keeps dex scopes separate and rejects credential-bearing or non-public requests', async () => {
  const a = new HyperliquidCatalogue(factory);
  const body = (dex: string) => JSON.stringify({ type: 'metaAndAssetCtxs', dex });
  await (await a.prepare(request(body('xyz')), controller().signal, Date.now() + 20_000))!.complete(
    response('[{"universe":[]},[]]'),
  );
  expect((await a.prepare(request(body('flx')), controller().signal, Date.now() + 20_000))?.cached).toBeUndefined();
  for (const packet of [
    { ...request(), intent: 'primary' },
    { ...request(), headers: { Authorization: 'secret' } },
    request(undefined, 'https://user:secret@api.hyperliquid.xyz/info'),
    request('{"type":"metaAndAssetCtxs","user":"private"}'),
    request('{"type":"order"}'),
    { ...request(), method: 'GET' },
  ])
    expect(await a.prepare(packet, controller().signal, Date.now() + 20_000)).toBeUndefined();
});

it('rejects clock-rollback freshness and a wait beyond its original deadline', async () => {
  const a = new HyperliquidCatalogue(factory);
  await (await a.prepare(request(), controller().signal, Date.now() + 20_000))!.complete(response());
  await flush();
  vi.setSystemTime(Date.now() - 1000);
  const leader = await a.prepare(request(), controller().signal, Date.now() + 20_000);
  expect(leader?.cached).toBeUndefined();
  const rejected = Promise.allSettled([
    new HyperliquidCatalogue(factory).prepare(request(), controller().signal, Date.now() + 50),
  ]);
  await flush();
  await vi.advanceTimersByTimeAsync(100);
  expect((await rejected)[0].status).toBe('rejected');
});

it('copies reused streaming buffers and preserves an oversized response byte for byte', async () => {
  const a = new HyperliquidCatalogue(factory);
  const read = await a.prepare(request(), controller().signal, Date.now() + 20_000);
  const bytes = new TextEncoder().encode('[null]');
  const body = (async function* () {
    const reused = new Uint8Array(1);
    for (const byte of bytes) {
      reused[0] = byte;
      yield reused;
    }
  })();
  const complete = await read!.complete({ status: 200, body });
  expect(complete.body).toEqual(bytes);
  await a.invalidateOrigin(ORIGIN);
  const large = new Uint8Array(1_048_577).fill(120);
  const next = await a.prepare(request(), controller().signal, Date.now() + 20_000);
  const oversized = await next!.complete({
    status: 200,
    body: (async function* () {
      yield large.subarray(0, 500_000);
      yield large.subarray(500_000);
    })(),
  });
  const chunks: Uint8Array[] = [];
  for await (const chunk of oversized.body as AsyncIterable<Uint8Array>) chunks.push(chunk);
  expect(Buffer.concat(chunks)).toEqual(Buffer.from(large));
  expect((await a.prepare(request(), controller().signal, Date.now() + 20_000))?.cached).toBeUndefined();
});

it('releases a failed or cancelled stream without caching a partial reply', async () => {
  const a = new HyperliquidCatalogue(factory);
  const first = await a.prepare(request(), controller().signal, Date.now() + 20_000);
  await expect(
    first!.complete({
      status: 200,
      body: (async function* () {
        yield new TextEncoder().encode('[null');
        throw new Error('stream failed');
      })(),
    }),
  ).rejects.toThrow('stream failed');
  const stop = controller();
  const second = await a.prepare(request(), stop.signal, Date.now() + 20_000);
  const iterator = {
    next: () => new Promise<IteratorResult<Uint8Array>>(() => {}),
    return: async () => ({ done: true as const, value: undefined }),
  };
  const result = Promise.allSettled([
    second!.complete({ status: 200, body: { [Symbol.asyncIterator]: () => iterator } }),
  ]);
  stop.abort();
  expect((await result)[0].status).toBe('rejected');
  const next = await a.prepare(request(), controller().signal, Date.now() + 20_000);
  expect(next?.cached).toBeUndefined();
  next!.cancel();
});

it('bounds cache keys and sweeps expired entries before accepting a new catalogue', async () => {
  const a = new HyperliquidCatalogue(factory);
  const packet = (i: number) => request(JSON.stringify({ type: 'metaAndAssetCtxs', dex: 'dex' + i }));
  for (let i = 0; i < 64; i++)
    await (await a.prepare(packet(i), controller().signal, Date.now() + 20_000))!.complete(
      response('[{"universe":[]},[]]'),
    );
  expect(await a.prepare(packet(64), controller().signal, Date.now() + 20_000)).toBeUndefined();
  await vi.advanceTimersByTimeAsync(30_001);
  const next = await a.prepare(packet(64), controller().signal, Date.now() + 20_000);
  expect(next).toBeDefined();
  next!.cancel();
});

it('shares spot catalogues with the complete token metadata intact', async () => {
  const a = new HyperliquidCatalogue(factory);
  const b = new HyperliquidCatalogue(factory);
  const packet = request('{"type":"spotMetaAndAssetCtxs"}');
  const expected = response('[{"universe":[],"tokens":[{"name":"USDC","index":0,"szDecimals":8}]},[]]');
  await (await a.prepare(packet, controller().signal, Date.now() + 20_000))!.complete(expected);
  expect((await b.prepare(packet, controller().signal, Date.now() + 20_000))?.cached?.body).toEqual(expected.body);
});

it.each([
  '[{"universe":[{}]},[{}]]',
  '[{"universe":[{"name":"BTC","szDecimals":5}]},[{"markPx":"not-a-price"}]]',
  '[{"universe":[],"tokens":[{"name":"USDC"}]},[]]',
])('passes through malformed rows unchanged without caching them: %s', async (body) => {
  const a = new HyperliquidCatalogue(factory);
  const packet = request(
    JSON.stringify({ type: body.includes('tokens') ? 'spotMetaAndAssetCtxs' : 'metaAndAssetCtxs' }),
  );
  const leader = await a.prepare(packet, controller().signal, Date.now() + 20_000);
  const original = response(body);
  expect(await leader!.complete(original)).toEqual(original);
  const next = await new HyperliquidCatalogue(factory).prepare(packet, controller().signal, Date.now() + 20_000);
  expect(next?.cached).toBeUndefined();
  next!.cancel();
});

it('does not hold a good HTTP answer behind stalled cache publication', async () => {
  const a = new HyperliquidCatalogue(factory);
  const read = await a.prepare(request(), controller().signal, Date.now() + 20_000);
  const abort = vi.fn();
  vi.spyOn(IDBDatabase.prototype, 'transaction').mockImplementationOnce(
    () =>
      ({
        objectStore: () => ({ get: () => ({}) }),
        abort,
      }) as unknown as IDBTransaction,
  );
  const expected = response();
  expect(await read!.complete(expected)).toEqual(expected);
  await flush();
  await vi.advanceTimersByTimeAsync(251);
  expect(abort).toHaveBeenCalledTimes(1);
});

it('rechecks freshness after an asynchronous storage observation crosses the TTL', async () => {
  const a = new HyperliquidCatalogue(factory);
  await (await a.prepare(request(), controller().signal, Date.now() + 20_000))!.complete(response());
  await flush();
  const get = IDBObjectStore.prototype.get;
  vi.spyOn(IDBObjectStore.prototype, 'get').mockImplementationOnce(function (this: IDBObjectStore, ...args) {
    const read = get.apply(this, args);
    read.addEventListener('success', () => queueMicrotask(() => vi.setSystemTime(Date.now() + 5001)));
    return read;
  });
  const next = await a.prepare(request(), controller().signal, Date.now() + 20_000);
  expect(next?.cached).toBeUndefined();
  next!.cancel();
});
