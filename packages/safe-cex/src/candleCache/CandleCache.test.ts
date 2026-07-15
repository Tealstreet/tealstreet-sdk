import type { Candle, Timeframe } from '../types';

import { describe, expect, it, vi } from 'vitest';

import { getCandleStartTime } from '../utils/getCandleStartTime';
import { CandleCache, cleanCandles, fetchOHLCVWithExchange, getCandleCacheKey } from './CandleCache';
import { MemoryCandleCacheBackend } from './backends/MemoryCandleCacheBackend';

const createCandle = (timestamp: number): Candle => ({
  timestamp,
  open: 100,
  high: 110,
  low: 90,
  close: 105,
  volume: 1000,
});

const alignTimestamp = (timestamp: number, interval: Timeframe): number => getCandleStartTime(interval, timestamp / 1000) * 1000;

describe('CandleCache', () => {
  it('cleans duplicate candles by timestamp', () => {
    const cleaned = cleanCandles([createCandle(2), createCandle(1), createCandle(1)]);
    expect(cleaned.map((candle) => candle.timestamp)).toEqual([1, 2]);
  });

  it('creates stable cache keys', () => {
    expect(getCandleCacheKey('binance', 'BTCUSDT', '5m')).toBe('binance-BTCUSDT-5m');
  });

  it('keeps clear/version API', () => {
    const cache = new CandleCache();
    const v1 = cache.getVersion();
    cache.reset();
    expect(cache.getVersion()).toBe(v1 + 1);
  });

  it('fetches and reuses cached candles without owning exchange lifecycle', async () => {
    const interval: Timeframe = '5m';
    const intervalMs = 5 * 60 * 1000;
    const cacheKey = getCandleCacheKey('binance', 'BTCUSDT', interval);
    const cache = new CandleCache({ backend: new MemoryCandleCacheBackend() });
    const baseTime = alignTimestamp(1700000000000, interval);

    const fetcher = vi
      .fn()
      .mockResolvedValueOnce(Array.from({ length: 5 }, (_, i) => createCandle(baseTime + i * intervalMs)))
      .mockResolvedValueOnce(Array.from({ length: 3 }, (_, i) => createCandle(baseTime + (5 + i) * intervalMs)));

    const result1 = await cache.fetch({
      cacheKey,
      interval,
      fromMs: baseTime,
      toMs: baseTime + 4 * intervalMs,
      limit: 10,
      fetcher: (opts) => fetcher({ ...opts, symbol: 'BTCUSDT' }),
    });

    expect(result1).toHaveLength(5);
    expect(fetcher).toHaveBeenCalledTimes(1);

    const result2 = await cache.fetch({
      cacheKey,
      interval,
      fromMs: baseTime + 2 * intervalMs,
      toMs: baseTime + 7 * intervalMs,
      limit: 10,
      fetcher: (opts) => fetcher({ ...opts, symbol: 'BTCUSDT' }),
    });

    expect(fetcher).toHaveBeenCalledTimes(2);
    expect(fetcher.mock.calls[1][0]).toMatchObject({
      symbol: 'BTCUSDT',
      interval,
      from: baseTime + 5 * intervalMs,
      to: baseTime + 7 * intervalMs,
      limit: 7,
    });
    expect(result2).toHaveLength(6);
  });

  it('passes fetch requests without fabricating a symbol', async () => {
    const interval: Timeframe = '5m';
    const cache = new CandleCache();
    const cacheKey = getCandleCacheKey('binance', 'BTCUSDT', interval);
    const alignedTime = alignTimestamp(1700000000000, interval);
    const fetcher = vi.fn().mockResolvedValue([]);

    await cache.fetch({
      cacheKey,
      interval,
      fromMs: alignedTime,
      toMs: alignedTime,
      limit: 1,
      fetcher,
      usePersistentBackend: false,
      nowMs: alignedTime + 60 * 1000,
    });

    expect(fetcher).toHaveBeenCalledWith({
      interval,
      from: alignedTime,
      to: alignedTime,
      limit: 1,
    });
  });

  it('uses nowMs consistently when capping future ranges', async () => {
    const interval: Timeframe = '5m';
    const intervalMs = 5 * 60 * 1000;
    const cache = new CandleCache();
    const cacheKey = getCandleCacheKey('binance', 'BTCUSDT', interval);
    const customNow = alignTimestamp(1700000000000, interval) + 10 * intervalMs;
    const fetcher = vi.fn().mockResolvedValue([]);

    // Request one candle past nowMs — the cap should pull `to` back to customNow
    // and credit the limit by exactly the one clipped future candle (10 → 9).
    const result = await cache.fetch({
      cacheKey,
      interval,
      fromMs: customNow - intervalMs,
      toMs: customNow + intervalMs,
      limit: 10,
      fetcher,
      usePersistentBackend: false,
      nowMs: customNow,
    });

    expect(fetcher).toHaveBeenCalledWith({
      interval,
      from: customNow - intervalMs,
      to: customNow,
      limit: 9,
    });
    expect(result).toEqual([]);
  });

  it('short-circuits when the entire requested range is in the future', async () => {
    const interval: Timeframe = '5m';
    const intervalMs = 5 * 60 * 1000;
    const cache = new CandleCache();
    const cacheKey = getCandleCacheKey('binance', 'BTCUSDT', interval);
    const customNow = alignTimestamp(1700000000000, interval) + 10 * intervalMs;
    const fetcher = vi.fn().mockResolvedValue([]);

    // Asking for 10 candles in a 24h window entirely past nowMs — the future-clip
    // should drive `limit` non-positive and short-circuit the fetch entirely so we
    // don't hit the network for candles that don't exist yet.
    const result = await cache.fetch({
      cacheKey,
      interval,
      fromMs: customNow + intervalMs,
      toMs: customNow + 24 * 60 * 60 * 1000,
      limit: 10,
      fetcher,
      usePersistentBackend: false,
      nowMs: customNow,
    });

    expect(fetcher).not.toHaveBeenCalled();
    expect(result).toEqual([]);
  });

  it('supports exchange-backed fetching without making the cache own exchange startup', async () => {
    const interval: Timeframe = '5m';
    const cache = new CandleCache();
    const cacheKey = getCandleCacheKey('bitrue', 'BTCUSDT', interval);
    const alignedTime = alignTimestamp(1700000000000, interval);
    const fetchOHLCV = vi.fn().mockResolvedValue([createCandle(1700000000000)]);
    const callMethodInWorker = vi.fn().mockResolvedValue([createCandle(1700000000000)]);

    await cache.fetchWithExchange({
      cacheKey,
      exchange: {
        name: 'bitrue',
        fetchOHLCV,
        callMethodInWorker,
      } as any,
      symbol: 'BTCUSDT',
      interval,
      fromMs: 1700000000000,
      toMs: 1700000000000,
      limit: 1,
      usePersistentBackend: false,
    });

    expect(callMethodInWorker).toHaveBeenCalledWith('fetchOHLCV', {
      symbol: 'BTCUSDT',
      interval,
      from: alignedTime,
      to: alignedTime,
      limit: 1,
    });
    expect(fetchOHLCV).not.toHaveBeenCalled();
  });

  it('supports read-through shared backends for distributed services', async () => {
    const interval: Timeframe = '5m';
    const intervalMs = 5 * 60 * 1000;
    const cacheKey = getCandleCacheKey('binance', 'BTCUSDT', interval);
    const backend = new MemoryCandleCacheBackend();
    const cacheA = new CandleCache({ backend, backendReadMode: 'read-through' });
    const cacheB = new CandleCache({ backend, backendReadMode: 'read-through' });
    const baseTime = alignTimestamp(1700000000000, interval);

    const fetcherA = vi
      .fn()
      .mockResolvedValueOnce(Array.from({ length: 5 }, (_, i) => createCandle(baseTime + i * intervalMs)));

    const resultA = await cacheA.fetch({
      cacheKey,
      interval,
      fromMs: baseTime,
      toMs: baseTime + 4 * intervalMs,
      limit: 10,
      fetcher: (opts) => fetcherA({ ...opts, symbol: 'BTCUSDT' }),
    });

    expect(resultA).toHaveLength(5);
    expect(fetcherA).toHaveBeenCalledTimes(1);

    const fetcherB = vi.fn().mockResolvedValue([]);
    const resultB = await cacheB.fetch({
      cacheKey,
      interval,
      fromMs: baseTime,
      toMs: baseTime + 4 * intervalMs,
      limit: 10,
      fetcher: (opts) => fetcherB({ ...opts, symbol: 'BTCUSDT' }),
    });

    expect(resultB).toHaveLength(5);
    expect(fetcherB).not.toHaveBeenCalled();

    const backfillFetcher = vi
      .fn()
      .mockResolvedValueOnce(Array.from({ length: 3 }, (_, i) => createCandle(baseTime + (5 + i) * intervalMs)));

    const resultB2 = await cacheB.fetch({
      cacheKey,
      interval,
      fromMs: baseTime + 2 * intervalMs,
      toMs: baseTime + 7 * intervalMs,
      limit: 10,
      fetcher: (opts) => backfillFetcher({ ...opts, symbol: 'BTCUSDT' }),
    });

    expect(resultB2).toHaveLength(6);
    expect(backfillFetcher).toHaveBeenCalledTimes(1);

    const verifyFetcher = vi.fn().mockResolvedValue([]);
    const resultA2 = await cacheA.fetch({
      cacheKey,
      interval,
      fromMs: baseTime + 2 * intervalMs,
      toMs: baseTime + 7 * intervalMs,
      limit: 10,
      fetcher: (opts) => verifyFetcher({ ...opts, symbol: 'BTCUSDT' }),
    });

    expect(resultA2).toHaveLength(6);
    expect(verifyFetcher).not.toHaveBeenCalled();
  });

  it('releases hydration slots after timeout so queued hydrations can proceed', async () => {
    vi.useFakeTimers();

    const interval: Timeframe = '5m';
    const cache = new CandleCache({
      backendReadMode: 'hydrate-once',
      hydrationTimeoutMs: 10,
      maxConcurrentHydrations: 1,
      backend: {
        loadAllChunks: vi
          .fn()
          .mockImplementationOnce(() => new Promise<Map<number, any>>(() => {}))
          .mockResolvedValueOnce(new Map()),
        persistChunks: vi.fn().mockResolvedValue(undefined),
      },
    });
    const baseTime = alignTimestamp(1700000000000, interval);
    const fetcher = vi.fn().mockResolvedValue([]);

    const firstFetch = cache.fetch({
      cacheKey: getCandleCacheKey('binance', 'BTCUSDT', interval),
      interval,
      fromMs: baseTime,
      toMs: baseTime,
      limit: 1,
      fetcher,
    });

    const secondFetch = cache.fetch({
      cacheKey: getCandleCacheKey('binance', 'ETHUSDT', interval),
      interval,
      fromMs: baseTime,
      toMs: baseTime,
      limit: 1,
      fetcher,
    });

    await vi.advanceTimersByTimeAsync(25);
    await expect(firstFetch).resolves.toEqual([]);
    await expect(secondFetch).resolves.toEqual([]);

    const backend = (cache as any).backend;
    expect(backend.loadAllChunks).toHaveBeenCalledTimes(2);

    vi.useRealTimers();
  });

  it('does not let a timed-out hydration overwrite freshly fetched candles', async () => {
    const interval: Timeframe = '5m';
    const intervalSeconds = 5 * 60;
    const baseTime = alignTimestamp(1700000000000, interval);
    const cacheKey = getCandleCacheKey('binance', 'BTCUSDT', interval);
    const chunkSize = 100;
    const candleIndex = Math.floor(baseTime / 1000 / intervalSeconds);
    const chunkKey = Math.floor(candleIndex / chunkSize);
    const offset = candleIndex % chunkSize;
    const staleCandle = { ...createCandle(baseTime), close: 101 };
    const freshCandle = { ...createCandle(baseTime), close: 109 };
    const staleChunk = new Array(chunkSize);
    staleChunk[offset] = staleCandle;

    let resolveHydration!: (chunks: Map<number, any>) => void;
    const hydration = new Promise<Map<number, any>>((resolve) => {
      resolveHydration = resolve;
    });
    const cache = new CandleCache({
      chunkSize,
      hydrationTimeoutMs: 1,
      backend: {
        loadAllChunks: vi.fn().mockReturnValue(hydration),
        persistChunks: vi.fn().mockResolvedValue(undefined),
      },
    });
    const fetcher = vi.fn().mockResolvedValue([freshCandle]);

    await expect(
      cache.fetch({
        cacheKey,
        interval,
        fromMs: baseTime,
        toMs: baseTime,
        limit: 1,
        fetcher,
        nowMs: baseTime + intervalSeconds * 1000,
      }),
    ).resolves.toEqual([freshCandle]);

    resolveHydration(new Map([[chunkKey, staleChunk]]));
    await hydration;
    await Promise.resolve();

    const cached = await cache.fetch({
      cacheKey,
      interval,
      fromMs: baseTime,
      toMs: baseTime,
      limit: 1,
      fetcher,
      nowMs: baseTime + intervalSeconds * 1000,
    });

    expect(cached).toEqual([freshCandle]);
    expect(fetcher).toHaveBeenCalledTimes(1);
  });

  it('falls back to direct exchange fetch when worker routing is disabled', async () => {
    const fetchOHLCV = vi.fn().mockResolvedValue([createCandle(1700000000000)]);

    await fetchOHLCVWithExchange({
      exchange: {
        name: 'binance',
        fetchOHLCV,
        callMethodInWorker: vi.fn(),
      } as any,
      symbol: 'BTCUSDT',
      interval: '5m',
      fromMs: 1700000000000,
      toMs: 1700000000000,
      limit: 1,
      useWorker: false,
    });

    expect(fetchOHLCV).toHaveBeenCalledWith({
      symbol: 'BTCUSDT',
      interval: '5m',
      from: 1700000000000,
      to: 1700000000000,
      limit: 1,
    });
  });
});
