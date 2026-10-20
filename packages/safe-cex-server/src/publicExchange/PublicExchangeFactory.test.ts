import type { Market } from '@tealstreet/safe-cex/src/types';
import { RustEngineExchange } from '@tealstreet/safe-cex/src/exchanges/rust/rust.exchange';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { PublicExchangeFactory } from './PublicExchangeFactory';
import { getMarketCacheKeys } from './buckets';
import { type RedisLikeMarketCacheClient, writeMarketCacheSnapshot } from './marketCache';

const mockExchange = vi.hoisted(() => ({
  start: vi.fn(async () => {}),
  fetchMarkets: vi.fn(),
  dispose: vi.fn(),
  store: {
    loaded: {
      balance: false,
      orders: false,
      markets: false,
      tickers: false,
      positions: false,
    },
    update: vi.fn(),
  },
}));

const createPublicExchange = vi.hoisted(() =>
  vi.fn(async (_name?: string, _options?: { engine?: string }) => mockExchange),
);

vi.mock('@tealstreet/safe-cex/src', () => ({
  createPublicExchange,
}));

class MemoryRedis implements RedisLikeMarketCacheClient {
  readonly values = new Map<string, string>();

  async get(key: string): Promise<string | null> {
    return this.values.get(key) ?? null;
  }

  async set(key: string, value: string): Promise<void> {
    this.values.set(key, value);
  }

  async del(...keys: string[]): Promise<void> {
    for (const key of keys) {
      this.values.delete(key);
    }
  }
}

const market = (id: string): Market =>
  ({
    id,
    symbol: id,
  }) as Market;

describe('PublicExchangeFactory', () => {
  beforeEach(() => {
    createPublicExchange.mockReset();
    // Resolve the class as the SDK registry does, including its default-native
    // Orderly broker. Native startup itself is covered by the real WASM suite.
    createPublicExchange.mockImplementation(async (name, options) => {
      Object.setPrototypeOf(
        mockExchange,
        options?.engine === 'rust' || name === 'tealstreet-dex' ? RustEngineExchange.prototype : Object.prototype,
      );
      return mockExchange;
    });
    Object.setPrototypeOf(mockExchange, Object.prototype);
    mockExchange.start.mockClear();
    mockExchange.fetchMarkets.mockReset();
    mockExchange.dispose.mockClear();
    mockExchange.store.loaded = {
      balance: false,
      orders: false,
      markets: false,
      tickers: false,
      positions: false,
    };
    mockExchange.store.update.mockClear();
    mockExchange.store.update.mockImplementation((changes) => {
      if (changes.loaded) {
        mockExchange.store.loaded = changes.loaded;
      }
    });
  });

  it.each([0, -1, Number.NaN, Number.POSITIVE_INFINITY])(
    'rejects invalid native startup deadline %s',
    (nativeStartupTimeoutMs) => {
      expect(() => new PublicExchangeFactory({ nativeStartupTimeoutMs })).toThrow(
        'nativeStartupTimeoutMs must be finite and positive',
      );
      expect(createPublicExchange).not.toHaveBeenCalled();
    },
  );

  it('rejects a shared public port before constructing an exchange', () => {
    expect(
      () =>
        new PublicExchangeFactory({
          exchangeOptions: {
            engine: 'rust',
            publicFeed: { mode: 'shared', port: {} as MessagePort },
          },
        }),
    ).toThrow('attach a RustPublicFeedProvider outside the factory');
    expect(createPublicExchange).not.toHaveBeenCalled();
    expect(mockExchange.fetchMarkets).not.toHaveBeenCalled();
  });

  it('refreshes the current hour instead of using latest stale markets as a normal hit', async () => {
    const redis = new MemoryRedis();
    await writeMarketCacheSnapshot({
      client: redis,
      exchange: 'bybit',
      bucket: '2026050222',
      markets: [market('OLDUSDT')],
      refreshedAt: 1,
    });
    mockExchange.fetchMarkets.mockResolvedValueOnce([market('BTCUSDT')]);

    const factory = new PublicExchangeFactory({
      redis,
      now: () => Date.UTC(2026, 4, 2, 23, 15),
    });

    const handle = await factory.get('bybit');

    expect(mockExchange.fetchMarkets).toHaveBeenCalledTimes(1);
    expect(handle.marketsBucket).toBe('2026050223');
    expect(handle.markets.map((item) => item.id)).toEqual(['BTCUSDT']);
    expect(mockExchange.store.update).toHaveBeenCalledWith(
      expect.objectContaining({
        markets: [expect.objectContaining({ id: 'BTCUSDT' })],
        loaded: expect.objectContaining({ markets: true }),
      }),
    );
  });

  it('uses latest stale markets only as fallback when current refresh fails', async () => {
    const redis = new MemoryRedis();
    await writeMarketCacheSnapshot({
      client: redis,
      exchange: 'bybit',
      bucket: '2026050222',
      markets: [market('OLDUSDT')],
      refreshedAt: 1,
    });
    mockExchange.fetchMarkets.mockRejectedValueOnce(new Error('upstream unavailable'));

    const factory = new PublicExchangeFactory({
      redis,
      now: () => Date.UTC(2026, 4, 2, 23, 15),
    });

    const handle = await factory.get('bybit');

    expect(mockExchange.fetchMarkets).toHaveBeenCalledTimes(1);
    expect(handle.marketsBucket).toBe('2026050222');
    expect(handle.markets.map((item) => item.id)).toEqual(['OLDUSDT']);
  });

  it('shares one cold exchange and metadata fetch across concurrent callers', async () => {
    mockExchange.fetchMarkets.mockResolvedValue([market('BTCUSDT')]);
    const factory = new PublicExchangeFactory();
    const handles = await Promise.all(Array.from({ length: 4 }, () => factory.get('bybit')));
    expect(handles.every((handle) => handle === handles[0])).toBe(true);
    expect(createPublicExchange).toHaveBeenCalledTimes(1);
    expect(mockExchange.fetchMarkets).toHaveBeenCalledTimes(1);
    await factory.dispose();
  });

  it('partitions Rust product catalogs while preserving each profile warm cache and semantic metadata', async () => {
    const redis = new MemoryRedis();
    const now = () => Date.UTC(2026, 4, 2, 23, 15);
    const linear = market('BTCUSDT');
    const option = {
      ...market('BTC-30DEC26-100000-C'),
      info: { symbolType: '', instrumentKind: 'CALL', expiry: 1798588800, strike: 100000 },
      option: { underlying: 'BTC', expiry: 1798588800, strike: 100000, right: 'call', style: 'european', settlement: 'cash', settleAsset: 'USDC', premiumAsset: 'USDC' },
    } as Market;
    mockExchange.fetchMarkets.mockResolvedValueOnce([linear]).mockResolvedValueOnce([linear, option]);
    const defaultFactory = new PublicExchangeFactory({ redis, now, exchangeOptions: { engine: 'rust' } });
    const combinedOptions = { engine: 'rust' as const, enableInverseMarkets: true, enableOptionMarkets: true };
    const combinedFactory = new PublicExchangeFactory({ redis, now, exchangeOptions: combinedOptions });
    expect((await defaultFactory.get('bybit')).markets).toEqual([linear]);
    expect((await combinedFactory.get('bybit')).markets).toEqual([linear, option]);
    expect(mockExchange.fetchMarkets).toHaveBeenCalledTimes(2);

    const defaultWarm = new PublicExchangeFactory({ redis, now, exchangeOptions: { engine: 'rust', enableInverseMarkets: false, enableOptionMarkets: false } });
    const combinedWarm = new PublicExchangeFactory({ redis, now, exchangeOptions: combinedOptions });
    expect((await defaultWarm.get('bybit')).markets).toEqual([linear]);
    expect((await combinedWarm.get('bybit')).markets).toEqual([linear, option]);
    expect(mockExchange.fetchMarkets).toHaveBeenCalledTimes(2);
    expect(redis.values.has(getMarketCacheKeys('bybit', '2026050223').bucketKey)).toBe(true);
    expect([...redis.values.keys()].filter((key) => key.endsWith(':latest'))).toHaveLength(2);
  });

  it('never uses a default-product stale fallback for another Rust profile', async () => {
    const redis = new MemoryRedis();
    const options = { redis, now: () => Date.UTC(2026, 4, 2, 23, 15) };
    await writeMarketCacheSnapshot({ client: redis, exchange: 'bybit', bucket: '2026050222', markets: [market('LINEAR')], refreshedAt: 1 });
    mockExchange.fetchMarkets.mockRejectedValueOnce(new Error('combined discovery unavailable'));
    const combined = new PublicExchangeFactory({ ...options, exchangeOptions: { engine: 'rust', enableOptionMarkets: true } });
    await expect(combined.get('bybit')).rejects.toThrow('combined discovery unavailable');
    expect(mockExchange.store.update).not.toHaveBeenCalled();
  });

  it('partitions registry-default native Orderly environments without an explicit engine flag', async () => {
    const redis = new MemoryRedis();
    const now = () => Date.UTC(2026, 4, 2, 23, 15);
    mockExchange.fetchMarkets.mockResolvedValueOnce([market('MAINNET')]).mockResolvedValueOnce([market('TESTNET')]);
    const main = new PublicExchangeFactory({ redis, now });
    const test = new PublicExchangeFactory({ redis, now, exchangeOptions: { testnet: true } });
    expect((await main.get('tealstreet-dex')).markets[0].id).toBe('MAINNET');
    expect((await test.get('tealstreet-dex')).markets[0].id).toBe('TESTNET');
    expect(mockExchange.start).toHaveBeenCalledTimes(2);
    expect(mockExchange.fetchMarkets).toHaveBeenCalledTimes(2);
    const warmMain = new PublicExchangeFactory({ redis, now });
    const warmTest = new PublicExchangeFactory({ redis, now, exchangeOptions: { testnet: true } });
    expect((await warmMain.get('tealstreet-dex')).markets[0].id).toBe('MAINNET');
    expect((await warmTest.get('tealstreet-dex')).markets[0].id).toBe('TESTNET');
    expect(mockExchange.start).toHaveBeenCalledTimes(2);
    expect(mockExchange.fetchMarkets).toHaveBeenCalledTimes(2);
  });

  it('preserves legacy cache keys when an explicit Rust request resolves to a TypeScript exchange', async () => {
    const redis = new MemoryRedis();
    const now = () => Date.UTC(2026, 4, 2, 23, 15);
    await writeMarketCacheSnapshot({ client: redis, exchange: 'bitunix', bucket: '2026050223', markets: [market('TS_FALLBACK')], refreshedAt: now() });
    // The SDK falls back to the TS registry when a venue has no Rust implementation.
    createPublicExchange.mockImplementationOnce(async () => mockExchange);
    const factory = new PublicExchangeFactory({ redis, now, exchangeOptions: { engine: 'rust', testnet: true } });
    expect((await factory.get('bitunix')).markets[0].id).toBe('TS_FALLBACK');
    expect(mockExchange.start).not.toHaveBeenCalled();
    expect(mockExchange.fetchMarkets).not.toHaveBeenCalled();
  });

  it('canonicalizes Rust public selections and isolates testnet without changing TypeScript keys', async () => {
    const redis = new MemoryRedis();
    const now = () => Date.UTC(2026, 4, 2, 23, 15);
    mockExchange.fetchMarkets.mockResolvedValueOnce([market('SELECTED')]).mockResolvedValueOnce([market('TESTNET')]);
    const selected = new PublicExchangeFactory({ redis, now, exchangeOptions: { engine: 'rust', polymarket: { marketSlugs: ['b', 'a'] } } });
    expect((await selected.get('polymarket')).markets[0].id).toBe('SELECTED');
    const sortedWarm = new PublicExchangeFactory({ redis, now, exchangeOptions: { engine: 'rust', polymarket: { marketSlugs: ['a', 'b'] } } });
    expect((await sortedWarm.get('polymarket')).markets[0].id).toBe('SELECTED');
    const testnet = new PublicExchangeFactory({ redis, now, exchangeOptions: { engine: 'rust', testnet: true } });
    expect((await testnet.get('polymarket')).markets[0].id).toBe('TESTNET');
    expect(mockExchange.fetchMarkets).toHaveBeenCalledTimes(2);

    await writeMarketCacheSnapshot({ client: redis, exchange: 'bybit', bucket: '2026050223', markets: [market('LEGACY_TS')], refreshedAt: now() });
    const legacy = new PublicExchangeFactory({ redis, now, exchangeOptions: { enableOptionMarkets: true } });
    expect((await legacy.get('bybit')).markets[0].id).toBe('LEGACY_TS');
    expect(mockExchange.fetchMarkets).toHaveBeenCalledTimes(2);
  });

  it('does not publish an exchange whose construction completes after disposal', async () => {
    let complete = (_exchange: typeof mockExchange) => {};
    createPublicExchange.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          complete = resolve;
        }),
    );
    const factory = new PublicExchangeFactory();
    const getting = factory.get('bybit');
    const outcome = expect(getting).rejects.toThrow(/disposed/);
    await factory.dispose('bybit');
    complete(mockExchange);
    await outcome;
    expect(mockExchange.dispose).toHaveBeenCalledTimes(1);
    mockExchange.fetchMarkets.mockResolvedValue([market('BTCUSDT')]);
    const handle = await factory.get('bybit');
    expect(handle.markets[0].id).toBe('BTCUSDT');
    expect(createPublicExchange).toHaveBeenCalledTimes(2);
    await factory.dispose();
  });

  it('does not overwrite a completed forced refresh with an older cache read', async () => {
    const redis = new MemoryRedis();
    const now = Date.UTC(2026, 4, 2, 23, 15);
    await writeMarketCacheSnapshot({
      client: redis,
      exchange: 'bybit',
      bucket: '2026050223',
      markets: [market('CACHED')],
      refreshedAt: 1,
    });
    const originalGet = redis.get.bind(redis);
    let release = () => {};
    let captured = () => {};
    const ready = new Promise<void>((resolve) => {
      captured = resolve;
    });
    const blocked = new Promise<void>((resolve) => {
      release = resolve;
    });
    let first = true;
    redis.get = async (key) => {
      const value = await originalGet(key);
      if (first) {
        first = false;
        captured();
        await blocked;
      }
      return value;
    };
    const factory = new PublicExchangeFactory({ redis, now: () => now });
    const getting = factory.get('bybit');
    await ready;
    mockExchange.fetchMarkets.mockResolvedValue([market('FRESH')]);
    await factory.refreshMarkets('bybit', 'metadata-change');
    release();
    expect((await getting).markets[0].id).toBe('FRESH');
    await factory.dispose();
  });

  it('does not coalesce a new UTC hour into the previous hour refresh', async () => {
    let now = Date.UTC(2026, 4, 2, 23, 59);
    let release = (_markets: Market[]) => {};
    let began = () => {};
    const ready = new Promise<void>((resolve) => {
      began = resolve;
    });
    mockExchange.fetchMarkets
      .mockImplementationOnce(() => {
        began();
        return new Promise<Market[]>((resolve) => {
          release = resolve;
        });
      })
      .mockResolvedValueOnce([market('NEXT_HOUR')]);
    const factory = new PublicExchangeFactory({ now: () => now });
    const previous = factory.refreshMarkets('bybit', 'before-midnight');
    await ready;
    now = Date.UTC(2026, 4, 3, 0, 1);
    const next = factory.get('bybit');
    release([market('PREVIOUS_HOUR')]);
    await previous;
    const handle = await next;
    expect(handle.marketsBucket).toBe('2026050300');
    expect(handle.markets[0].id).toBe('NEXT_HOUR');
    expect(mockExchange.fetchMarkets).toHaveBeenCalledTimes(2);
    await factory.dispose();
  });

  it('does not hydrate stale fallback metadata after disposal during the cache read', async () => {
    const redis = new MemoryRedis();
    await writeMarketCacheSnapshot({
      client: redis,
      exchange: 'bybit',
      bucket: '2026050222',
      markets: [market('STALE')],
      refreshedAt: 1,
    });
    const originalGet = redis.get.bind(redis);
    let release = () => {};
    let began = () => {};
    const ready = new Promise<void>((resolve) => {
      began = resolve;
    });
    const blocked = new Promise<void>((resolve) => {
      release = resolve;
    });
    redis.get = async (key) => {
      const value = await originalGet(key);
      if (key.endsWith(':latest')) {
        began();
        await blocked;
      }
      return value;
    };
    mockExchange.fetchMarkets.mockRejectedValueOnce(new Error('upstream unavailable'));
    const factory = new PublicExchangeFactory({ redis, now: () => Date.UTC(2026, 4, 2, 23, 15) });
    const getting = factory.get('bybit');
    const outcome = expect(getting).rejects.toThrow(/disposed/);
    await ready;
    await factory.dispose('bybit');
    release();
    await outcome;
    expect(mockExchange.store.update).not.toHaveBeenCalled();
  });

  it('keeps a successful forced refresh ahead of an earlier pending stale-fallback read', async () => {
    const redis = new MemoryRedis();
    await writeMarketCacheSnapshot({
      client: redis,
      exchange: 'bybit',
      bucket: '2026050222',
      markets: [market('STALE')],
      refreshedAt: 1,
    });
    const originalGet = redis.get.bind(redis);
    let release = () => {};
    let began = () => {};
    const ready = new Promise<void>((resolve) => {
      began = resolve;
    });
    const blocked = new Promise<void>((resolve) => {
      release = resolve;
    });
    redis.get = async (key) => {
      const value = await originalGet(key);
      if (key.endsWith(':latest')) {
        began();
        await blocked;
      }
      return value;
    };
    mockExchange.fetchMarkets.mockRejectedValueOnce(new Error('first read unavailable'));
    const factory = new PublicExchangeFactory({ redis, now: () => Date.UTC(2026, 4, 2, 23, 15) });
    const getting = factory.get('bybit');
    await ready;
    mockExchange.fetchMarkets.mockResolvedValueOnce([market('FRESH')]);
    await factory.refreshMarkets('bybit', 'retry-succeeded');
    const fresh = await factory.get('bybit');
    expect(fresh.markets[0].id).toBe('FRESH');
    release();
    expect(await getting).toBe(fresh);
    expect(fresh.markets[0].id).toBe('FRESH');
    expect(fresh.marketsBucket).toBe('2026050223');
    expect(mockExchange.store.update).toHaveBeenLastCalledWith(expect.objectContaining({ markets: [market('FRESH')] }));
    await factory.dispose();
  });
});
