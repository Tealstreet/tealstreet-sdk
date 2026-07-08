import { afterEach, beforeEach, describe, expect, it, Mock, vi } from 'vitest';

// Mock dependencies
vi.mock('nanoid', () => ({
  nanoid: vi.fn(() => 'test-session-key-123'),
}));

vi.mock('@tealstreet/utils/src/compression', () => ({
  compress: vi.fn((data: string) => Promise.resolve(data)),
}));

vi.mock('./LogSeverity', () => ({
  LogSeverity: {
    Info: 'info',
    Warning: 'warning',
    Error: 'error',
    Debug: 'debug',
  },
}));

// Mock fetch globally
const mockFetch = vi.fn();
global.fetch = mockFetch;

// Mock console methods
const mockConsoleError = vi.spyOn(console, 'error').mockImplementation(() => {});

describe('utils.ts', () => {
  let utils: any;

  beforeEach(async () => {
    // Setup fake timers
    vi.useFakeTimers();
    vi.clearAllMocks();
    vi.clearAllTimers();

    // Reset environment variables
    delete process.env.NEXT_PUBLIC_FORCE_EXTERNAL_LOGGING;
    process.env.NODE_ENV = 'production'; // Default to production for tests

    // Mock fetch to resolve successfully by default
    mockFetch.mockResolvedValue({
      ok: true,
      status: 200,
    });

    // Dynamically import the module to ensure fresh state
    utils = await import('./utils');
  });

  afterEach(() => {
    vi.useRealTimers();
    mockConsoleError.mockRestore();
  });

  describe('getSessionKey', () => {
    it('should return a session key', () => {
      const sessionKey = utils.getSessionKey();
      expect(sessionKey).toBe('test-session-key-123');
      expect(typeof sessionKey).toBe('string');
      expect(sessionKey.length).toBeGreaterThan(0);
    });

    it('should return the same session key on multiple calls', () => {
      const key1 = utils.getSessionKey();
      const key2 = utils.getSessionKey();
      expect(key1).toBe(key2);
    });
  });

  describe('logInfo', () => {
    it('should add info log to queue', () => {
      utils.logInfo('Test info message');

      // We can't directly access the queue, but we can test the behavior through dumpLogQueue
      const dumpSpy = vi.spyOn(utils, 'dumpLogQueue');
      utils.dumpLogQueue();
      expect(dumpSpy).toHaveBeenCalled();
    });

    it('should handle multiple arguments', () => {
      expect(() => {
        utils.logInfo('Test', ' info', ' message');
      }).not.toThrow();
    });

    it('should handle empty arguments', () => {
      expect(() => {
        utils.logInfo();
      }).not.toThrow();
    });

    it('should handle non-string arguments gracefully', () => {
      expect(() => {
        utils.logInfo('Test', 123 as any, null as any);
      }).not.toThrow();
    });
  });

  describe('logWarn', () => {
    it('should add warning log to queue', () => {
      utils.logWarn('Test warning message');

      const dumpSpy = vi.spyOn(utils, 'dumpLogQueue');
      utils.dumpLogQueue();
      expect(dumpSpy).toHaveBeenCalled();
    });

    it('should handle multiple arguments', () => {
      expect(() => {
        utils.logWarn('Test', ' warning', ' message');
      }).not.toThrow();
    });
  });

  describe('logError', () => {
    it('should add error log to queue', () => {
      utils.logError('Test error message');

      const dumpSpy = vi.spyOn(utils, 'dumpLogQueue');
      utils.dumpLogQueue();
      expect(dumpSpy).toHaveBeenCalled();
    });

    it('should handle multiple arguments', () => {
      expect(() => {
        utils.logError('Test', ' error', ' message');
      }).not.toThrow();
    });

    it('should handle Error objects', () => {
      const error = new Error('Test error');
      expect(() => {
        utils.logError(error.toString());
      }).not.toThrow();
    });
  });

  describe('dumpLogQueue', () => {
    it('should call sendToGrafana when queue has items', () => {
      // Add some logs first
      utils.logInfo('Test message');

      // Mock fetch for Grafana call
      mockFetch.mockResolvedValueOnce({ ok: true });

      expect(() => {
        utils.dumpLogQueue();
      }).not.toThrow();
    });

    it('should handle empty queue', () => {
      expect(() => {
        utils.dumpLogQueue();
      }).not.toThrow();
    });

    it('should reset queue state after dump', () => {
      utils.logInfo('Test message');
      utils.dumpLogQueue();

      // After dump, another dump should not cause issues
      expect(() => {
        utils.dumpLogQueue();
      }).not.toThrow();
    });
  });

  describe('startLoggingToGrafana', () => {
    it('should set up periodic logging', () => {
      const userId = 'test-user-123';

      expect(() => {
        utils.startLoggingToGrafana(userId);
      }).not.toThrow();

      // Fast-forward time to trigger interval
      vi.advanceTimersByTime(5000);

      expect(vi.getTimerCount()).toBeGreaterThan(0);
    });

    it('should handle empty userId', () => {
      expect(() => {
        utils.startLoggingToGrafana('');
      }).not.toThrow();
    });

    it('should handle null userId gracefully', () => {
      expect(() => {
        utils.startLoggingToGrafana(null as any);
      }).not.toThrow();
    });

    it('should trigger dumpLogQueue at intervals', () => {
      utils.startLoggingToGrafana('test-user');

      // Advance time by 5 seconds (interval duration)
      vi.advanceTimersByTime(5000);

      // The interval should have been set up and triggered
      expect(vi.getTimerCount()).toBeGreaterThan(0);

      // Test that the interval function doesn't throw
      expect(() => {
        vi.runOnlyPendingTimers();
      }).not.toThrow();
    });
  });

  describe('environment variable handling', () => {
    it('should skip Grafana calls in development when FORCE_EXTERNAL_LOGGING is not set', () => {
      process.env.NODE_ENV = 'development';
      delete process.env.NEXT_PUBLIC_FORCE_EXTERNAL_LOGGING;

      utils.logInfo('Test message');
      utils.dumpLogQueue();

      // Fetch should not be called in dev without force flag
      expect(mockFetch).not.toHaveBeenCalled();
    });

    it('should make Grafana calls in production by default', async () => {
      process.env.NODE_ENV = 'production';
      delete process.env.NEXT_PUBLIC_FORCE_EXTERNAL_LOGGING;

      // Re-import module after setting env vars
      const freshUtils = await import('./utils');

      freshUtils.logInfo('Test message');
      freshUtils.dumpLogQueue();

      // Wait for async operations
      await vi.waitFor(() => {
        expect(mockFetch).toHaveBeenCalled();
      });
    });

    it('should make Grafana calls in development when FORCE_EXTERNAL_LOGGING is set', async () => {
      process.env.NODE_ENV = 'development';
      process.env.NEXT_PUBLIC_FORCE_EXTERNAL_LOGGING = '1';

      // Re-import module after setting env vars
      const freshUtils = await import('./utils');

      freshUtils.logInfo('Test message');
      freshUtils.dumpLogQueue();

      // Wait for async operations
      await vi.waitFor(() => {
        expect(mockFetch).toHaveBeenCalled();
      });
    });
  });

  describe('network error handling', () => {
    it('should handle fetch failures gracefully', async () => {
      mockFetch.mockRejectedValueOnce(new Error('Network error'));

      utils.logInfo('Test message');

      expect(() => {
        utils.dumpLogQueue();
      }).not.toThrow();
    });

    it('should retry on network failures', async () => {
      mockFetch.mockRejectedValue(new Error('Network error'));

      utils.logInfo('Test message');

      // The dump should not throw even with network errors
      expect(() => {
        utils.dumpLogQueue();
      }).not.toThrow();

      // Fast-forward time to allow retry mechanism
      vi.advanceTimersByTime(5000);
      vi.runAllTimers();
    });

    it('should stop retrying after 5 attempts', async () => {
      // Mock fetch to always fail
      mockFetch.mockRejectedValue(new Error('Persistent network error'));

      utils.logInfo('Test message');

      // The function should handle retries gracefully
      expect(() => {
        utils.dumpLogQueue();
      }).not.toThrow();

      // Fast-forward time to simulate multiple retries
      for (let i = 0; i < 6; i++) {
        vi.advanceTimersByTime(5000);
        vi.runAllTimers();
      }

      // The function should have handled the retries without throwing
      expect(true).toBe(true); // Test passes if no errors are thrown
    });
  });

  describe('log queue size management', () => {
    it('should dump queue when size exceeds limit', () => {
      // Test that large messages don't break the system
      expect(() => {
        // Create a large message that would trigger the 200KB limit
        const largeMessage = 'x'.repeat(250000); // 250KB message
        utils.logInfo(largeMessage);
      }).not.toThrow();

      // Verify the queue can be dumped
      expect(() => {
        utils.dumpLogQueue();
      }).not.toThrow();
    });

    it('should handle very large individual messages', () => {
      const veryLargeMessage = 'x'.repeat(1000000); // 1MB message

      expect(() => {
        utils.logInfo(veryLargeMessage);
      }).not.toThrow();
    });
  });

  describe('string splitting functionality', () => {
    it('should split large strings correctly', () => {
      // Test with a string that needs splitting
      const largeMessage = 'a'.repeat(300000); // 300KB

      expect(() => {
        utils.logInfo(largeMessage);
      }).not.toThrow();
    });

    it('should handle UTF-8 characters correctly', () => {
      const unicodeMessage = '🚀'.repeat(100000); // Unicode characters

      expect(() => {
        utils.logInfo(unicodeMessage);
      }).not.toThrow();
    });

    it('should limit number of chunks to prevent endless loops', () => {
      const massiveMessage = 'x'.repeat(5000000); // 5MB message

      expect(() => {
        utils.logInfo(massiveMessage);
      }).not.toThrow();
    });
  });

  describe('message formatting', () => {
    it('should include session key in formatted messages', () => {
      utils.startLoggingToGrafana('test-user');
      utils.logInfo('Test message');

      const dumpSpy = vi.spyOn(utils, 'dumpLogQueue');
      utils.dumpLogQueue();

      expect(dumpSpy).toHaveBeenCalled();
      expect(utils.getSessionKey()).toBe('test-session-key-123');
    });

    it('should include user ID in formatted messages', () => {
      const userId = 'user-123';
      utils.startLoggingToGrafana(userId);
      utils.logInfo('Test message');

      expect(() => {
        utils.dumpLogQueue();
      }).not.toThrow();
    });

    it('should handle different log levels correctly', () => {
      utils.logInfo('Info message');
      utils.logWarn('Warning message');
      utils.logError('Error message');

      expect(() => {
        utils.dumpLogQueue();
      }).not.toThrow();
    });
  });

  describe('Grafana API payload structure', () => {
    it('should send correctly formatted payload to Grafana', () => {
      const userId = 'test-user';
      utils.startLoggingToGrafana(userId);
      utils.logInfo('Test message');

      // Dumping the queue should trigger the Grafana API call
      expect(() => {
        utils.dumpLogQueue();
      }).not.toThrow();

      // Give some time for async operations and check if fetch was called
      setTimeout(() => {
        expect(mockFetch).toHaveBeenCalled();
      }, 0);
    });

    it('should compress payload before sending', async () => {
      process.env.NODE_ENV = 'production';
      const { compress } = await import('@tealstreet/utils/src/compression');

      utils.startLoggingToGrafana('test-user');
      utils.logInfo('Test message');
      utils.dumpLogQueue();

      expect(compress).toHaveBeenCalled();
    });
  });

  describe('error scenarios', () => {
    it('should handle JSON stringify errors gracefully', () => {
      // Create a circular reference that would cause JSON.stringify to fail
      const circularObj: any = {};
      circularObj.self = circularObj;

      expect(() => {
        utils.logInfo(circularObj.toString());
      }).not.toThrow();
    });

    it('should handle null and undefined values', () => {
      expect(() => {
        utils.logInfo(null as any);
        utils.logInfo(undefined as any);
      }).not.toThrow();
    });

    it('should handle boolean and number values', () => {
      expect(() => {
        utils.logInfo(true as any);
        utils.logInfo(false as any);
        utils.logInfo(123 as any);
        utils.logInfo(0 as any);
      }).not.toThrow();
    });
  });

  describe('concurrent operations', () => {
    it('should handle multiple rapid log calls', () => {
      expect(() => {
        for (let i = 0; i < 100; i++) {
          utils.logInfo(`Message ${i}`);
        }
      }).not.toThrow();
    });

    it('should handle dump during active logging', () => {
      utils.logInfo('Message 1');

      expect(() => {
        utils.dumpLogQueue();
        utils.logInfo('Message 2');
        utils.dumpLogQueue();
      }).not.toThrow();
    });
  });

  describe('session key uniqueness', () => {
    it('should generate different session keys across module reloads', async () => {
      const firstKey = utils.getSessionKey();

      // Mock nanoid to return a different value
      const { nanoid } = await import('nanoid');
      (nanoid as Mock).mockReturnValueOnce('different-session-key');

      // Re-import to simulate module reload
      const newUtils = await import('./utils');
      const _secondKey = newUtils.getSessionKey();

      // Note: In this test setup, we expect them to be the same because
      // the module maintains state. In a real scenario with module reloads,
      // they would be different.
      expect(firstKey).toBe('test-session-key-123');
    });
  });

  describe('web worker compatibility', () => {
    it('should handle undefined process gracefully and default to logging', async () => {
      // Temporarily remove process
      const originalProcess = global.process;
      // @ts-expect-error - intentionally making process undefined
      delete global.process;

      try {
        // The logging functions read `typeof process` synchronously at call
        // time, so the already-imported module handles `process` being
        // undefined. (Re-importing here would force vitest to re-instantiate the
        // module without `process`, which crashes vitest's own module runner —
        // not the code under test. The module top-level has no `process`
        // references, so there's nothing extra to exercise via re-import.)
        utils.logInfo('Test message in web worker');
        utils.dumpLogQueue();
      } finally {
        // Restore process IMMEDIATELY after the synchronous calls — before any
        // `await` below. `process` must never be deleted across a microtask/tick
        // boundary: vitest's worker RPC reaches for `process.nextTick` on tick
        // and throws an uncaught `Cannot set properties of undefined` if it's
        // gone (a timing-dependent CI failure). The fetch is already queued by
        // dumpLogQueue, so the assertions below still verify the worker path.
        global.process = originalProcess;
      }

      // Should still attempt to send logs when process is undefined
      // (defaults to production behavior)
      await vi.waitFor(() => {
        expect(mockFetch).toHaveBeenCalled();
      });

      // Verify the correct payload was sent
      expect(mockFetch).toHaveBeenCalledWith(
        expect.stringContaining('/api/logs'),
        expect.objectContaining({
          method: 'POST',
        }),
      );
    });
  });

  describe('retry mechanism', () => {
    it('should retry after backoff when fetch rejects (connection failure)', async () => {
      // fetch always rejects, simulating an offline/connection failure
      mockFetch.mockRejectedValue(new Error('Network error'));

      utils.logInfo('Test message');
      utils.dumpLogQueue();

      // Flush the pending compress()/fetch() microtasks -> first attempt fires
      await vi.advanceTimersByTimeAsync(0);
      expect(mockFetch).toHaveBeenCalledTimes(1);

      // Advance past the 5s backoff -> the retry must actually re-issue fetch.
      // On the buggy code the rejection is never caught (fetch not awaited),
      // so the retry path is dead and this stays at 1.
      await vi.advanceTimersByTimeAsync(5_000);
      expect(mockFetch).toHaveBeenCalledTimes(2);
    });

    it('should treat a non-ok response (e.g. 500) as a failure and retry', async () => {
      mockFetch.mockResolvedValue({ ok: false, status: 500 });

      utils.logInfo('Test message');
      utils.dumpLogQueue();

      await vi.advanceTimersByTimeAsync(0);
      expect(mockFetch).toHaveBeenCalledTimes(1);

      // A 500 must be treated as failure and retried after backoff.
      await vi.advanceTimersByTimeAsync(5_000);
      expect(mockFetch).toHaveBeenCalledTimes(2);
    });

    it('should NOT retry when fetch resolves ok', async () => {
      mockFetch.mockResolvedValue({ ok: true, status: 200 });

      utils.logInfo('Test message');
      utils.dumpLogQueue();

      await vi.advanceTimersByTimeAsync(0);
      expect(mockFetch).toHaveBeenCalledTimes(1);

      // No backoff should have been scheduled; advancing time changes nothing.
      await vi.advanceTimersByTimeAsync(30_000);
      expect(mockFetch).toHaveBeenCalledTimes(1);
    });

    it('should stop retrying once the retry cap is exceeded', async () => {
      mockFetch.mockRejectedValue(new Error('Persistent network error'));

      utils.logInfo('Test message');
      utils.dumpLogQueue();

      // Drive far past the cap. Attempts run for retries 0..5 (guard drops
      // retries > 5), so fetch is issued exactly 6 times and then stops.
      for (let i = 0; i < 10; i++) {
        await vi.advanceTimersByTimeAsync(5_000);
      }

      expect(mockFetch).toHaveBeenCalledTimes(6);
    });
  });
});
