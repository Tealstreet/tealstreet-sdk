import type { Transport } from '@tealstreet/safe-cex-wasm/host';

import { afterEach, describe, expect, it, vi } from 'vitest';

import * as sdk from '@tealstreet/safe-cex/src';

import { realVenue } from '@tealstreet/safe-cex/src/__tests__/cutover/realVenue';
import { loadNodeEngine } from '@tealstreet/safe-cex/src/exchanges/rust/loadNodeEngine';
import {
  RustEngineExchange,
  setRustEngineTransport,
  setWasmEngineLoader,
} from '@tealstreet/safe-cex/src/exchanges/rust/rust.exchange';

import { PublicExchangeFactory } from './PublicExchangeFactory';
import { type RedisLikeMarketCacheClient, writeMarketCacheSnapshot } from './marketCache';

class MemoryRedis implements RedisLikeMarketCacheClient {
  readonly values = new Map<string, string>();
  failWrites = false;
  async get(key: string) {
    return this.values.get(key) ?? null;
  }
  async set(key: string, value: string) {
    if (this.failWrites) throw new Error('cache unavailable');
    this.values.set(key, value);
  }
  async del(...keys: string[]) {
    for (const key of keys) this.values.delete(key);
  }
}

const catalog = '/fapi/v1/exchangeInfo';
const deferred = () => {
  let resolve = () => {};
  const promise = new Promise<void>((done) => {
    resolve = done;
  });
  return { promise, resolve };
};

describe('PublicExchangeFactory with the real native engine', () => {
  let factory: PublicExchangeFactory | undefined;
  let input: ReturnType<typeof realVenue> | undefined;
  let release: (() => void) | undefined;

  afterEach(async () => {
    await factory?.dispose();
    release?.();
    release = undefined;
    input?.dispose();
    setWasmEngineLoader(loadNodeEngine);
    vi.restoreAllMocks();
  });

  const setup = (wrap?: (transport: Transport) => Transport) => {
    input = realVenue('binance');
    const transport = wrap?.(input.transport) ?? input.transport;
    setRustEngineTransport(() => transport);
    setWasmEngineLoader(loadNodeEngine);
    factory = new PublicExchangeFactory({ exchangeOptions: { engine: 'rust' } });
    return factory;
  };

  it('starts Rust explicitly and shares one cold startup across callers', async () => {
    const source = setup();
    const handles = await Promise.all(Array.from({ length: 4 }, () => source.get('binance')));
    expect(handles.every((handle) => handle === handles[0])).toBe(true);
    expect(handles[0].exchange).toBeInstanceOf(RustEngineExchange);
    expect(handles[0].markets.length).toBeGreaterThan(0);
    expect(handles[0].exchange.store.loaded.markets).toBe(true);
    expect(handles[0].markets).toEqual(handles[0].exchange.store.markets);
    expect(input!.requests.filter((url) => url.includes(catalog))).toHaveLength(1);
    expect(input!.errors).toEqual([]);
  });

  it('force-refreshes native metadata, coalesces callers and replaces the engine after success', async () => {
    let stepSize = '0.001';
    const source = setup((transport) => ({
      ...transport,
      async request(request) {
        const response = await transport.request(request);
        if (!request.url.includes(catalog)) return response;
        if (!(response.body instanceof Uint8Array)) throw new Error('Expected buffered fixture response');
        const body = JSON.parse(new TextDecoder().decode(response.body));
        const btc = body.symbols.find((market: { symbol: string }) => market.symbol === 'BTCUSDT');
        btc.filters.find((filter: { filterType: string }) => filter.filterType === 'LOT_SIZE').stepSize = stepSize;
        return { ...response, body: new TextEncoder().encode(JSON.stringify(body)) };
      },
    }));
    const handle = await source.get('binance');
    const previous = handle.exchange;
    expect(handle.markets.find((market) => market.symbol === 'BTCUSDT')!.precision.amount).toBe(0.001);
    stepSize = '0.1';
    const results = await Promise.all([
      handle.forceRefreshMarkets('instrument-change'),
      source.refreshMarkets('binance', 'same-change'),
      handle.forceRefreshMarkets('another-consumer'),
    ]);
    expect(results[0]).toBe(results[1]);
    expect(results[1]).toBe(results[2]);
    expect(handle.exchange).not.toBe(previous);
    expect(previous.isDisposed).toBe(true);
    expect(handle.exchange.isDisposed).toBe(false);
    expect(handle.markets.find((market) => market.symbol === 'BTCUSDT')!.precision.amount).toBe(0.1);
    expect(handle.exchange.store.markets).toEqual(handle.markets);
    expect(input!.requests.filter((url) => url.includes(catalog))).toHaveLength(2);
    expect(await source.get('binance')).toBe(handle);
    expect(input!.errors).toEqual([]);
  });

  it('preserves the working engine and metadata when replacement startup fails and allows retry', async () => {
    const source = setup();
    const handle = await source.get('binance');
    const previous = handle.exchange;
    const markets = handle.markets;
    const refreshedAt = handle.refreshedAt;
    setWasmEngineLoader(async () => {
      throw new Error('native startup unavailable');
    });
    await expect(handle.forceRefreshMarkets('failed-startup')).rejects.toThrow('native startup unavailable');
    expect(handle.exchange).toBe(previous);
    expect(previous.isDisposed).toBe(false);
    expect(handle.markets).toBe(markets);
    expect(handle.refreshedAt).toBe(refreshedAt);
    expect(await source.get('binance')).toBe(handle);
    setWasmEngineLoader(loadNodeEngine);
    await handle.forceRefreshMarkets('retry');
    expect(previous.isDisposed).toBe(true);
    expect(input!.requests.filter((url) => url.includes(catalog))).toHaveLength(2);
  });

  it('hydrates a warm Rust cache without network and refetches after an externally started handle', async () => {
    const redis = new MemoryRedis();
    setup();
    factory = new PublicExchangeFactory({ redis, exchangeOptions: { engine: 'rust' } });
    const first = await factory.get('binance');
    const originalMarkets = first.markets;
    await factory.dispose();
    const count = input!.requests.length;
    factory = new PublicExchangeFactory({ redis, exchangeOptions: { engine: 'rust' } });
    const cached = await factory.get('binance');
    expect(cached.markets).toEqual(originalMarkets);
    expect(input!.requests).toHaveLength(count);
    expect((cached.exchange as RustEngineExchange).hasStarted).toBe(false);
    await cached.exchange.start();
    const externallyStarted = cached.exchange;
    await cached.forceRefreshMarkets('metadata-after-live-start');
    expect(cached.exchange).not.toBe(externallyStarted);
    expect(externallyStarted.isDisposed).toBe(true);
    expect(input!.requests.filter((url) => url.includes(catalog))).toHaveLength(3);
  });

  it('preserves the last successful engine and metadata if cache persistence fails', async () => {
    const redis = new MemoryRedis();
    setup();
    factory = new PublicExchangeFactory({ redis, exchangeOptions: { engine: 'rust' } });
    const handle = await factory.get('binance');
    const previous = handle.exchange;
    const markets = handle.markets;
    redis.failWrites = true;
    await expect(handle.forceRefreshMarkets('cache-failed')).rejects.toThrow('cache unavailable');
    expect(handle.exchange).toBe(previous);
    expect(handle.markets).toBe(markets);
    expect(previous.isDisposed).toBe(false);
    redis.failWrites = false;
    await handle.forceRefreshMarkets('cache-recovered');
    expect(previous.isDisposed).toBe(true);
  });

  it('keeps a lazy cached engine and store intact when forced-refresh persistence fails', async () => {
    const redis = new MemoryRedis();
    setup();
    factory = new PublicExchangeFactory({ redis, exchangeOptions: { engine: 'rust' } });
    await factory.get('binance');
    await factory.dispose();
    factory = new PublicExchangeFactory({ redis, exchangeOptions: { engine: 'rust' } });
    const handle = await factory.get('binance');
    const previous = handle.exchange as RustEngineExchange;
    const markets = handle.markets;
    const loaded = { ...previous.store.loaded };
    const refreshedAt = handle.refreshedAt;
    expect(previous.hasStarted).toBe(false);
    redis.failWrites = true;
    await expect(handle.forceRefreshMarkets('lazy-cache-write-failed')).rejects.toThrow('cache unavailable');
    expect(handle.exchange).toBe(previous);
    expect(previous.isDisposed).toBe(false);
    expect(previous.hasStarted).toBe(false);
    expect(previous.store.markets).toBe(markets);
    expect(previous.store.loaded).toEqual(loaded);
    expect(handle.markets).toBe(markets);
    expect(handle.refreshedAt).toBe(refreshedAt);
    expect(input!.requests.filter((url) => url.includes(catalog))).toHaveLength(2);
    redis.failWrites = false;
    await handle.forceRefreshMarkets('lazy-cache-recovered');
    expect(previous.isDisposed).toBe(true);
    expect(handle.exchange).not.toBe(previous);
    expect(input!.requests.filter((url) => url.includes(catalog))).toHaveLength(3);
  });

  it('bounds a genuinely held native metadata request and preserves the working engine', async () => {
    let held = false;
    let aborted = 0;
    setup((transport) => ({
      ...transport,
      async request(request) {
        if (held && request.url.includes(catalog)) {
          return new Promise<never>((_, reject) =>
            request.signal!.addEventListener(
              'abort',
              () => {
                aborted++;
                reject(new Error('held metadata request aborted'));
              },
              { once: true },
            ),
          );
        }
        return transport.request(request);
      },
    }));
    factory = new PublicExchangeFactory({ nativeStartupTimeoutMs: 10_000, exchangeOptions: { engine: 'rust' } });
    const handle = await factory.get('binance');
    const previous = handle.exchange;
    const markets = handle.markets;
    held = true;
    await expect(handle.forceRefreshMarkets('held-native-read')).rejects.toThrow('startup timed out after 10000ms');
    expect(aborted).toBe(1);
    expect(handle.exchange).toBe(previous);
    expect(previous.isDisposed).toBe(false);
    expect(handle.markets).toBe(markets);
    expect(await factory.get('binance')).toBe(handle);
  }, 30_000);

  it('recovers a lazy Rust engine after cold startup times out into stale Redis fallback', async () => {
    const created: Array<Awaited<ReturnType<typeof sdk.createPublicExchange>>> = [];
    const createPublicExchange = sdk.createPublicExchange;
    vi.spyOn(sdk, 'createPublicExchange').mockImplementation(async (...args) => {
      const exchange = await createPublicExchange(...args);
      created.push(exchange);
      return exchange;
    });
    const redis = new MemoryRedis();
    let held = false;
    let aborted = 0;
    setup((transport) => ({
      ...transport,
      async request(request) {
        if (held && request.url.includes(catalog)) {
          return new Promise<never>((_, reject) =>
            request.signal!.addEventListener(
              'abort',
              () => {
                aborted++;
                reject(new Error('held metadata request aborted'));
              },
              { once: true },
            ),
          );
        }
        return transport.request(request);
      },
    }));
    const markets = (await factory!.get('binance')).markets;
    await factory!.dispose();
    await writeMarketCacheSnapshot({
      client: redis,
      exchange: 'binance',
      bucket: '2026102011',
      markets,
      refreshedAt: 1,
    });
    held = true;
    factory = new PublicExchangeFactory({
      redis,
      nativeStartupTimeoutMs: 100,
      now: () => Date.UTC(2026, 9, 20, 12),
      exchangeOptions: { engine: 'rust' },
    });
    const fallback = await factory.get('binance');
    expect(aborted).toBe(1);
    expect(fallback.markets).toEqual(markets);
    expect(fallback.exchange.store.markets).toEqual(markets);
    expect(fallback.marketsBucket).toBe('2026102011');
    expect(created).toHaveLength(3);
    expect(created[1].isDisposed).toBe(true);
    expect(fallback.exchange).toBe(created[2]);
    expect(fallback.exchange.isDisposed).toBe(false);
    expect((fallback.exchange as RustEngineExchange).hasStarted).toBe(false);
    const requests = input!.requests.length;
    expect(requests).toBeGreaterThan(0);
    held = false;
    await fallback.exchange.start();
    expect(fallback.exchange.store.loaded.markets).toBe(true);
    expect(fallback.exchange.store.loaded.tickers).toBe(true);
    expect(fallback.exchange.store.markets).toEqual(markets);
    expect(input!.requests.length).toBeGreaterThan(requests);
    expect(input!.errors).toEqual([]);
  });

  it.each(['outer cache lookup', 'locked refresh lookup'])(
    'revives a failed cold handle lazily when a current Redis snapshot arrives at %s',
    async (lookup) => {
      const created: Array<Awaited<ReturnType<typeof sdk.createPublicExchange>>> = [];
      const createPublicExchange = sdk.createPublicExchange;
      vi.spyOn(sdk, 'createPublicExchange').mockImplementation(async (...args) => {
        const exchange = await createPublicExchange(...args);
        created.push(exchange);
        return exchange;
      });
      const redis = new MemoryRedis();
      let held = false;
      setup((transport) => ({
        ...transport,
        async request(request) {
          if (held && request.url.includes(catalog)) {
            return new Promise<never>((_, reject) =>
              request.signal!.addEventListener('abort', () => reject(new Error('held metadata request aborted')), {
                once: true,
              }),
            );
          }
          return transport.request(request);
        },
      }));
      const markets = (await factory!.get('binance')).markets;
      await factory!.dispose();
      held = true;
      factory = new PublicExchangeFactory({
        redis,
        nativeStartupTimeoutMs: 100,
        now: () => Date.UTC(2026, 9, 20, 12),
        exchangeOptions: { engine: 'rust' },
      });
      await expect(factory.get('binance')).rejects.toThrow('startup timed out');
      expect(created[1].isDisposed).toBe(true);
      await writeMarketCacheSnapshot({
        client: redis,
        exchange: 'binance',
        bucket: '2026102012',
        markets,
        refreshedAt: 1,
      });
      if (lookup === 'locked refresh lookup') {
        const originalGet = redis.get.bind(redis);
        let missed = false;
        redis.get = async (key) => {
          if (!missed && key.endsWith(':2026102012')) {
            missed = true;
            return null;
          }
          return originalGet(key);
        };
      }
      const cached = await factory.get('binance');
      expect(created).toHaveLength(3);
      expect(cached.exchange).toBe(created[2]);
      expect(cached.exchange.isDisposed).toBe(false);
      expect((cached.exchange as RustEngineExchange).hasStarted).toBe(false);
      expect(cached.exchange.store.markets).toEqual(markets);
      held = false;
      await cached.exchange.start();
      expect(cached.exchange.store.loaded.markets).toBe(true);
      expect(cached.exchange.store.loaded.tickers).toBe(true);
    },
  );

  it('disposal cancels a replacement startup and cannot resurrect the old handle', async () => {
    let hold = false;
    const blocked = deferred();
    const requested = deferred();
    release = blocked.resolve;
    const source = setup((transport) => ({
      ...transport,
      async request(request) {
        if (hold && request.url.includes(catalog)) {
          requested.resolve();
          await blocked.promise;
        }
        return transport.request(request);
      },
    }));
    const handle = await source.get('binance');
    hold = true;
    const refreshing = handle.forceRefreshMarkets('dispose-during-refresh');
    const outcome = expect(refreshing).rejects.toThrow(/disposed/);
    await requested.promise;
    await handle.dispose();
    await outcome;
    expect(handle.exchange.isDisposed).toBe(true);
    expect(() => handle.forceRefreshMarkets('stale-handle')).toThrow(/disposed/);
    hold = false;
    blocked.resolve();
    const replacement = await source.get('binance');
    expect(replacement).not.toBe(handle);
    expect(replacement.exchange.isDisposed).toBe(false);
    await handle.dispose();
    expect(replacement.exchange.isDisposed).toBe(false);
  });
});
