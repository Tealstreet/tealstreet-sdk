import { describe, expect, it, vi } from 'vitest';
import { CandleCache } from './CandleCache';

describe('throwing hydration cap proof', () => {
  it('releases two throwing slots so a queued successful hydration runs without a timeout', async () => {
    vi.useFakeTimers();
    try {
      const loadAllChunks = vi.fn()
        .mockImplementationOnce(() => { throw new Error('first hydration throws'); })
        .mockImplementationOnce(() => { throw new Error('second hydration throws'); })
        .mockResolvedValueOnce(new Map());
      const onHydrationTimeout = vi.fn();
      const cache = new CandleCache({
        backend: { loadAllChunks, persistChunks: async () => {} },
        maxConcurrentHydrations: 2,
        onHydrationTimeout,
      });
      const fetches = ['a', 'b', 'queued-success'].map(cacheKey => cache.fetch({
        cacheKey, interval: '5m', fromMs: 1699999800000, toMs: 1699999800000,
        limit: 1, nowMs: 1700000100000, fetcher: async () => [],
      }));
      expect(loadAllChunks).toHaveBeenCalledTimes(2);
      expect((cache as any).hydrationRunning).toBe(2);
      expect((cache as any).hydrationQueue).toHaveLength(1);
      for (let tick = 0; tick < 12; tick++) await Promise.resolve();
      expect(loadAllChunks).toHaveBeenCalledTimes(3);
      expect((cache as any).hydrationRunning).toBe(0);
      expect((cache as any).hydrationQueue).toHaveLength(0);
      expect(onHydrationTimeout).not.toHaveBeenCalled();
      await expect(Promise.all(fetches)).resolves.toEqual([[], [], []]);
    } finally { vi.useRealTimers(); }
  });
});
