import { describe, expect, it, vi } from 'vitest';
import { CandleCache } from './CandleCache';
import type { Candle } from '../types';

const timestamp = 1699999800000;
const candle = (close = 105): Candle => ({ timestamp, open: 100, high: 110, low: 90, close, volume: 1000 });
const request = (cacheKey: string, fetcher = async (): Promise<Candle[]> => [], nowMs = timestamp + 600000) => ({
  cacheKey, interval: '5m' as const, fromMs: timestamp, toMs: timestamp, limit: 1, nowMs, fetcher,
});
const flush = async () => { for (let tick = 0; tick < 12; tick++) await Promise.resolve(); };

describe('hydration timeout oracle proof', () => {
  it('releases the cap, leaves an empty key retryable, and permits an old late writer', async () => {
    vi.useFakeTimers();
    vi.setSystemTime(0);
    try {
      let resolveOld!: (chunks: Map<number, Array<Candle | undefined>>) => void;
      const oldLoad = new Promise<Map<number, Array<Candle | undefined>>>(resolve => { resolveOld = resolve; });
      const loadAllChunks = vi.fn().mockReturnValueOnce(oldLoad).mockResolvedValue(new Map());
      const onHydrationTimeout = vi.fn();
      const cache = new CandleCache({
        backend: { loadAllChunks, persistChunks: async () => {} },
        maxConcurrentHydrations: 1, hydrationTimeoutMs: 10, onHydrationTimeout,
      });
      const first = cache.fetch(request('timed-out'));
      const queued = cache.fetch(request('queued'));
      expect(loadAllChunks).toHaveBeenCalledTimes(1);
      expect((cache as any).hydrationRunning).toBe(1);
      expect((cache as any).hydrationQueue).toHaveLength(1);
      await vi.advanceTimersByTimeAsync(9);
      expect(loadAllChunks).toHaveBeenCalledTimes(1);
      await vi.advanceTimersByTimeAsync(2);
      expect(loadAllChunks).toHaveBeenCalledTimes(2);
      expect(onHydrationTimeout).toHaveBeenCalledExactlyOnceWith('timed-out');
      expect((cache as any).hydrationRunning).toBe(0);
      expect((cache as any).hydratedCacheKeys.has('timed-out')).toBe(false);
      expect((cache as any).chunkedCandleCache.has('timed-out')).toBe(false);
      await expect(Promise.all([first, queued])).resolves.toEqual([[], []]);
      await expect(cache.fetch(request('timed-out'))).resolves.toEqual([]);
      expect(loadAllChunks).toHaveBeenCalledTimes(3);
      expect((cache as any).hydratedCacheKeys.has('timed-out')).toBe(true);
      const index = Math.floor(timestamp / 300000);
      const stale = candle(101);
      const chunk = new Array<Candle | undefined>(100);
      chunk[index % 100] = stale;
      resolveOld(new Map([[Math.floor(index / 100), chunk]]));
      await flush();
      expect(cache.getCachedCandles('timed-out', '5m', timestamp, timestamp, 1, timestamp + 600000)
        .contiguousBehindCandles).toEqual([stale]);
      expect((cache as any).hydrationRunning).toBe(0);
      await expect(cache.fetch(request('after-late'))).resolves.toEqual([]);
      expect(loadAllChunks).toHaveBeenCalledTimes(4);
      expect((cache as any).hydrationRunning).toBe(0);
    } finally { vi.useRealTimers(); }
  });

  it('preserves the unhydrated empty-memory-map retry suppression', async () => {
    vi.useFakeTimers();
    vi.setSystemTime(0);
    try {
      const loadAllChunks = vi.fn()
        .mockImplementationOnce(() => new Promise<Map<number, Array<Candle | undefined>>>(() => {}))
        .mockResolvedValue(new Map());
      const cache = new CandleCache({
        backend: { loadAllChunks, persistChunks: async () => {} },
        maxConcurrentHydrations: 1, hydrationTimeoutMs: 10,
      });
      const first = cache.fetch(request('timed-out', async () => [candle()], timestamp + 60000));
      await vi.advanceTimersByTimeAsync(11);
      await expect(first).resolves.toEqual([candle()]);
      expect((cache as any).hydrationRunning).toBe(0);
      expect((cache as any).hydratedCacheKeys.has('timed-out')).toBe(false);
      expect((cache as any).chunkedCandleCache.get('timed-out').size).toBe(0);
      for (let retry = 0; retry < 3; retry++) {
        await expect(cache.fetch(request('timed-out', undefined, timestamp + 60000))).resolves.toEqual([]);
      }
      expect(loadAllChunks).toHaveBeenCalledTimes(1);
      cache.reset({ cacheKey: 'timed-out' });
      await expect(cache.fetch(request('timed-out', undefined, timestamp + 60000))).resolves.toEqual([]);
      expect(loadAllChunks).toHaveBeenCalledTimes(2);
    } finally { vi.useRealTimers(); }
  });
});
