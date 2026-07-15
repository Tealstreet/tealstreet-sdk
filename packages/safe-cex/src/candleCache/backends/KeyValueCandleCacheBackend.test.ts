import type { Candle } from '../../types';
import type { CandleCacheKeyValueStore } from '../types';

import { describe, expect, it, vi } from 'vitest';

import { KeyValueCandleCacheBackend } from './KeyValueCandleCacheBackend';

const candleAtIndex = (index: number, intervalSeconds: number): Candle => ({
  timestamp: index * intervalSeconds * 1000,
  open: index,
  high: index,
  low: index,
  close: index,
  volume: index,
});

describe('KeyValueCandleCacheBackend', () => {
  it('skips candles that no longer belong to their persisted chunk key', async () => {
    const intervalSeconds = 60;
    const chunkSize = 2;
    const valid = candleAtIndex(20, intervalSeconds);
    const wrongChunk = candleAtIndex(22, intervalSeconds);
    const store: CandleCacheKeyValueStore = {
      get: vi.fn().mockResolvedValue(
        JSON.stringify({
          candles: [valid, wrongChunk],
          lastAccessed: Date.now(),
        }),
      ),
      set: vi.fn(),
      delete: vi.fn(),
      listKeys: vi.fn(),
    };
    const backend = new KeyValueCandleCacheBackend(store);

    const chunks = await backend.loadChunks('binance-BTCUSDT-1m', [10], {
      chunkSize,
      intervalSeconds,
    });

    expect(chunks.get(10)).toEqual([valid, undefined]);
  });

  it('does not expose an empty chunk when every stored candle belongs elsewhere', async () => {
    const intervalSeconds = 60;
    const store: CandleCacheKeyValueStore = {
      get: vi.fn().mockResolvedValue(
        JSON.stringify({
          candles: [candleAtIndex(22, intervalSeconds)],
          lastAccessed: Date.now(),
        }),
      ),
      set: vi.fn(),
      delete: vi.fn(),
      listKeys: vi.fn(),
    };
    const backend = new KeyValueCandleCacheBackend(store);

    await expect(
      backend.loadChunks('binance-BTCUSDT-1m', [10], {
        chunkSize: 2,
        intervalSeconds,
      }),
    ).resolves.toEqual(new Map());
  });
});
