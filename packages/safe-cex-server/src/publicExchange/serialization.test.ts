import { describe, expect, it } from 'vitest';

import type { Market } from '@tealstreet/safe-cex/src/types';

import {
  buildMarketCacheSnapshot,
  latestPointerFromSnapshot,
  parseLatestMarketPointer,
  parseMarketCacheSnapshot,
  serializeLatestMarketPointer,
  serializeMarketCacheSnapshot,
} from './serialization';

const market = (overrides: Partial<Market> = {}): Market =>
  ({
    id: 'BTCUSDT',
    symbol: 'BTCUSDT',
    base: 'BTC',
    quote: 'USDT',
    active: true,
    precision: { price: 0.1, amount: 0.001 },
    limits: {},
    ...overrides,
  }) as Market;

describe('publicExchange serialization', () => {
  it('round-trips market snapshots and latest pointers', () => {
    const snapshot = buildMarketCacheSnapshot({
      exchange: 'bybit',
      bucket: '2026050223',
      markets: [market()],
      refreshedAt: 1_777_777_777,
    });

    expect(
      parseMarketCacheSnapshot(serializeMarketCacheSnapshot(snapshot), { exchange: 'bybit', bucket: '2026050223' }),
    ).toEqual(snapshot);

    const pointer = latestPointerFromSnapshot(snapshot);
    expect(parseLatestMarketPointer(serializeLatestMarketPointer(pointer))).toEqual(pointer);
  });

  it('rejects snapshots with mismatched counts and hashes', () => {
    const snapshot = buildMarketCacheSnapshot({
      exchange: 'bybit',
      bucket: '2026050223',
      markets: [market()],
      refreshedAt: 1_777_777_777,
    });

    expect(parseMarketCacheSnapshot(JSON.stringify({ ...snapshot, marketCount: 2 }))).toBeNull();
    expect(parseMarketCacheSnapshot(JSON.stringify({ ...snapshot, contentHash: 'not-the-hash' }))).toBeNull();
  });

  it('rejects invalid latest pointers', () => {
    expect(parseLatestMarketPointer(JSON.stringify({ schemaVersion: 1, bucket: 'bad' }))).toBeNull();
    expect(parseLatestMarketPointer('not-json')).toBeNull();
  });
});
