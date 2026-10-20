import type { Market } from '@tealstreet/safe-cex/src/types';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { PublicExchangeFactory } from './PublicExchangeFactory';
import { type RedisLikeMarketCacheClient, writeMarketCacheSnapshot } from './marketCache';

const mockExchange = vi.hoisted(() => ({
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

const createPublicExchange = vi.hoisted(() => vi.fn(async () => mockExchange));

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
    createPublicExchange.mockClear();
    mockExchange.fetchMarkets.mockReset();
    mockExchange.dispose.mockClear();
    mockExchange.store.loaded = {
      balance: false,
      orders: false,
      markets: false,
      tickers: false,
      positions: false,
    };
    mockExchange.store.update.mockImplementation((changes) => {
      if (changes.loaded) {
        mockExchange.store.loaded = changes.loaded;
      }
    });
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
});
