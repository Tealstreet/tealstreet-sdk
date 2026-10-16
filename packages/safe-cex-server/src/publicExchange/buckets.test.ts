import { describe, expect, it } from 'vitest';

import { getMarketCacheKeys, getPreviousUtcHourMarketBucket, getUtcHourMarketBucket } from './buckets';

describe('publicExchange buckets', () => {
  it('quantizes timestamps to UTC-hour buckets', () => {
    const bucket = getUtcHourMarketBucket(new Date('2026-05-02T23:59:59.999Z'));

    expect(bucket).toEqual({
      bucket: '2026050223',
      startMs: Date.UTC(2026, 4, 2, 23, 0, 0, 0),
      endMs: Date.UTC(2026, 4, 3, 0, 0, 0, 0),
    });
  });

  it('calculates previous UTC-hour buckets across day boundaries', () => {
    expect(getPreviousUtcHourMarketBucket(new Date('2026-05-02T00:15:00.000Z')).bucket).toBe('2026050123');
  });

  it('builds the planned Redis key shapes', () => {
    expect(getMarketCacheKeys('bybit', '2026050223', 'safe-cex')).toEqual({
      bucketKey: 'safe-cex:markets:v1:bybit:2026050223',
      latestKey: 'safe-cex:markets:v1:bybit:latest',
      lockKey: 'safe-cex:markets:v1:bybit:lock:2026050223',
    });
  });
});
