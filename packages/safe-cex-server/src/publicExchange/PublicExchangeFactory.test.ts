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
});
