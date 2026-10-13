import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { loop } from './loop';

describe('loop', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  describe('without minInterval', () => {
    it('should schedule function via setTimeout', async () => {
      const fn = vi.fn();

      // loop is async and schedules the callback
      const promise = loop(fn);

      // Function not called yet (scheduled via setTimeout 0)
      expect(fn).not.toHaveBeenCalled();

      // Advance timers to execute setTimeout(0)
      await vi.runAllTimersAsync();

      expect(fn).toHaveBeenCalledTimes(1);
    });
  });

  it('never schedules the function when minInterval is Infinity', async () => {
    const fn = vi.fn();

    await loop(fn, Number.POSITIVE_INFINITY);
    await vi.runAllTimersAsync();

    expect(fn).not.toHaveBeenCalled();
  });

  describe('with minInterval', () => {
    it('should wait for minInterval before scheduling function', async () => {
      const fn = vi.fn();

      loop(fn, 1000);

      expect(fn).not.toHaveBeenCalled();

      // After 999ms, still in sleep
      await vi.advanceTimersByTimeAsync(999);
      expect(fn).not.toHaveBeenCalled();

      // After sleep completes, callback is scheduled via setTimeout(0)
      await vi.advanceTimersByTimeAsync(1);
      // Now run the scheduled callback
      await vi.runAllTimersAsync();
      expect(fn).toHaveBeenCalledTimes(1);
    });

    it('should handle zero minInterval', async () => {
      const fn = vi.fn();

      loop(fn, 0);

      // Run all timers (sleep(0) + setTimeout(0))
      await vi.runAllTimersAsync();
      expect(fn).toHaveBeenCalledTimes(1);
    });
  });

  describe('return value', () => {
    it('should return a promise', () => {
      const fn = vi.fn();
      const result = loop(fn);
      expect(result).toBeInstanceOf(Promise);
    });
  });
});
