import { describe, expect, it } from 'vitest';

import type { Market } from '@tealstreet/safe-cex/src/types';

import {
  readLatestMarketCacheSnapshot,
  readMarketCacheSnapshot,
  type RedisLikeMarketCacheClient,
  writeMarketCacheSnapshot,
} from './marketCache';

class MemoryRedis implements RedisLikeMarketCacheClient {
  readonly values = new Map<string, string>();
  readonly setCalls: Array<{ key: string; value: string; args: Array<string | number> }> = [];

  async get(key: string): Promise<string | null> {
    return this.values.get(key) ?? null;
  }

  async set(key: string, value: string, ...args: Array<string | number>): Promise<void> {
    this.setCalls.push({ key, value, args });
    this.values.set(key, value);
  }

  async del(...keys: string[]): Promise<void> {
    for (const key of keys) {
      this.values.delete(key);
    }
  }
}

const market = { id: 'BTCUSDT', symbol: 'BTCUSDT' } as Market;

describe('publicExchange market cache', () => {
  it('writes bucket snapshots and latest pointers with TTL args', async () => {
    const redis = new MemoryRedis();

    const snapshot = await writeMarketCacheSnapshot({
      client: redis,
      exchange: 'bybit',
      bucket: '2026050223',
      markets: [market],
      namespace: 'safe-cex',
      refreshedAt: 1_777_777_777,
      ttlMs: 123,
    });

    expect(await readMarketCacheSnapshot(redis, 'bybit', '2026050223')).toEqual(snapshot);
    expect(await readLatestMarketCacheSnapshot(redis, 'bybit')).toEqual(snapshot);
    expect(redis.setCalls.map((call) => call.args)).toEqual([
      ['PX', 123],
      ['PX', 123],
    ]);
  });

  it('does not return latest when the pointed bucket is missing', async () => {
    const redis = new MemoryRedis();
    await writeMarketCacheSnapshot({
      client: redis,
      exchange: 'bybit',
      bucket: '2026050223',
      markets: [market],
    });

    redis.values.delete('safe-cex:markets:v1:bybit:2026050223');

    expect(await readLatestMarketCacheSnapshot(redis, 'bybit')).toBeNull();
  });
});
