import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { ThrottleManager } from './ThrottleManager';

describe('ThrottleManager', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  describe('immediate execution', () => {
    it('should execute immediately on first call', () => {
      const fn = vi.fn();
      const manager = new ThrottleManager(fn, 1000, () => 'key');

      manager.call();

      expect(fn).toHaveBeenCalledTimes(1);
    });

    it('should pass arguments to function', () => {
      const fn = vi.fn();
      const manager = new ThrottleManager(fn, 1000, (a: string) => a);

      manager.call('test-arg');

      expect(fn).toHaveBeenCalledWith('test-arg');
    });

    it('should execute immediately after delay has passed', () => {
      const fn = vi.fn();
      const manager = new ThrottleManager(fn, 1000, () => 'key');

      manager.call();
      vi.advanceTimersByTime(1000);
      manager.call();

      expect(fn).toHaveBeenCalledTimes(2);
    });
  });

  describe('throttling behavior', () => {
    it('should not execute immediately on subsequent calls within delay', () => {
      const fn = vi.fn();
      const manager = new ThrottleManager(fn, 1000, () => 'key');

      manager.call();
      manager.call();
      manager.call();

      expect(fn).toHaveBeenCalledTimes(1);
    });

    it('should execute tail call after delay', () => {
      const fn = vi.fn();
      const manager = new ThrottleManager(fn, 1000, () => 'key');

      manager.call();
      manager.call();
      vi.advanceTimersByTime(1000);

      expect(fn).toHaveBeenCalledTimes(2);
    });

    it('should use latest arguments for tail call', () => {
      const fn = vi.fn();
      const manager = new ThrottleManager(fn, 1000, (a: string) => 'key');

      manager.call('first');
      manager.call('second');
      manager.call('third');
      vi.advanceTimersByTime(1000);

      expect(fn).toHaveBeenCalledTimes(2);
      expect(fn).toHaveBeenLastCalledWith('third');
    });
  });

  describe('key-based throttling', () => {
    it('should throttle separately for different keys', () => {
      const fn = vi.fn();
      const manager = new ThrottleManager(fn, 1000, (key: string) => key);

      manager.call('key1');
      manager.call('key2');
      manager.call('key1');
      manager.call('key2');

      expect(fn).toHaveBeenCalledTimes(2);
    });

    it('should execute tail calls for each key', () => {
      const fn = vi.fn();
      const manager = new ThrottleManager(fn, 1000, (key: string) => key);

      manager.call('key1');
      manager.call('key2');
      manager.call('key1');
      manager.call('key2');
      vi.advanceTimersByTime(1000);

      expect(fn).toHaveBeenCalledTimes(4);
    });
  });

  describe('clear', () => {
    it('should clear all pending throttle timers', () => {
      const fn = vi.fn();
      const manager = new ThrottleManager(fn, 1000, () => 'key');

      manager.call();
      manager.call();
      manager.clear();
      vi.advanceTimersByTime(1000);

      // Only the initial immediate call should have happened
      expect(fn).toHaveBeenCalledTimes(1);
    });

    it('should allow new calls after clear', () => {
      const fn = vi.fn();
      const manager = new ThrottleManager(fn, 1000, () => 'key');

      manager.call();
      manager.clear();
      manager.call();

      expect(fn).toHaveBeenCalledTimes(2);
    });

    it('should clear multiple keys', () => {
      const fn = vi.fn();
      const manager = new ThrottleManager(fn, 1000, (key: string) => key);

      manager.call('key1');
      manager.call('key2');
      manager.call('key1');
      manager.call('key2');
      manager.clear();
      vi.advanceTimersByTime(1000);

      expect(fn).toHaveBeenCalledTimes(2);
    });
  });

  describe('timer cleanup', () => {
    it('should clean up key after delay', () => {
      const fn = vi.fn();
      const manager = new ThrottleManager(fn, 1000, () => 'key');

      manager.call();
      vi.advanceTimersByTime(1000);

      // After cleanup, the next call should be immediate
      manager.call();
      expect(fn).toHaveBeenCalledTimes(2);
    });
  });

  describe('multiple arguments', () => {
    it('should handle multiple arguments', () => {
      const fn = vi.fn();
      const manager = new ThrottleManager(fn, 1000, (a: string, b: number) => `${a}-${b}`);

      manager.call('test', 1);
      expect(fn).toHaveBeenCalledWith('test', 1);
    });

    it('should use key function with multiple arguments', () => {
      const fn = vi.fn();
      const manager = new ThrottleManager(fn, 1000, (a: string, b: number) => `${a}-${b}`);

      manager.call('test', 1);
      manager.call('test', 2); // Different key
      manager.call('other', 1); // Different key

      expect(fn).toHaveBeenCalledTimes(3);
    });
  });
});
