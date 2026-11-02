import jwt from 'jsonwebtoken';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

// Mock dependencies before import
vi.mock('./utils/virtualClock', () => ({
  virtualClock: {
    getCurrentTimeMs: vi.fn(() => Date.now()),
  },
}));

// Mock BroadcastChannel
const mockPostMessage = vi.fn();
vi.stubGlobal(
  'BroadcastChannel',
  class MockBroadcastChannel {
    postMessage = mockPostMessage;
    onmessage: ((event: MessageEvent) => void) | null = null;
    close = vi.fn();
  },
);

describe('proxyTokenClient', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.resetModules();
  });

  describe('isTokenExpired', () => {
    it('shares one implementation with the public access-token facade', async () => {
      const accessToken = await import('./accessToken');
      const proxyTokenClient = await import('./proxyTokenClient');

      expect(accessToken.isTokenExpired).toBe(proxyTokenClient.isTokenExpired);
    });

    it('should return true for empty token', async () => {
      const { isTokenExpired } = await import('./proxyTokenClient');
      expect(isTokenExpired('')).toBe(true);
    });

    it('should return true for invalid token', async () => {
      const { isTokenExpired } = await import('./proxyTokenClient');
      expect(isTokenExpired('not-a-valid-jwt')).toBe(true);
    });

    it('should return false for non-expired token', async () => {
      const { isTokenExpired } = await import('./proxyTokenClient');
      const { virtualClock } = await import('./utils/virtualClock');

      const now = 1700000000000;
      vi.mocked(virtualClock.getCurrentTimeMs).mockReturnValue(now);

      const futureExpiry = Math.floor(now / 1000) + 3600;
      const token = jwt.sign({ exp: futureExpiry }, 'test-secret');

      expect(isTokenExpired(token)).toBe(false);
    });

    it('should return true for expired token', async () => {
      const { isTokenExpired } = await import('./proxyTokenClient');
      const { virtualClock } = await import('./utils/virtualClock');

      const now = 1700000000000;
      vi.mocked(virtualClock.getCurrentTimeMs).mockReturnValue(now);

      const pastExpiry = Math.floor(now / 1000) - 3600;
      const token = jwt.sign({ exp: pastExpiry }, 'test-secret');

      expect(isTokenExpired(token)).toBe(true);
    });
  });

  describe('getCurrentToken', () => {
    it('should return empty string initially', async () => {
      const { getCurrentToken } = await import('./proxyTokenClient');
      expect(getCurrentToken()).toBe('');
    });
  });

  describe('setToken', () => {
    it('should set the token', async () => {
      const { setToken, getCurrentToken } = await import('./proxyTokenClient');

      const futureExpiry = Math.floor(Date.now() / 1000) + 3600;
      const token = jwt.sign({ exp: futureExpiry }, 'test-secret');

      setToken(token);
      expect(getCurrentToken()).toBe(token);
    });

    it('should handle empty token', async () => {
      const { setToken, getCurrentToken } = await import('./proxyTokenClient');

      // First set a token
      const token = jwt.sign({ exp: Math.floor(Date.now() / 1000) + 3600 }, 'test-secret');
      setToken(token);

      // Then clear it
      setToken('');
      expect(getCurrentToken()).toBe('');
    });

    it('should handle invalid token gracefully', async () => {
      const { setToken, getCurrentToken } = await import('./proxyTokenClient');

      // Should not throw
      setToken('invalid-token');
      expect(getCurrentToken()).toBe('invalid-token');
    });
  });

  describe('onTokenUpdate', () => {
    it('should register callback and return unsubscribe function', async () => {
      const { onTokenUpdate } = await import('./proxyTokenClient');

      const callback = vi.fn();
      const unsubscribe = onTokenUpdate(callback);

      expect(typeof unsubscribe).toBe('function');
    });

    it('should unsubscribe when unsubscribe function is called', async () => {
      const { onTokenUpdate } = await import('./proxyTokenClient');

      const callback = vi.fn();
      const unsubscribe = onTokenUpdate(callback);

      // Call unsubscribe twice should not throw
      unsubscribe();
      expect(() => unsubscribe()).not.toThrow();
    });
  });
});

describe('proxyTokenClient hung fetch', () => {
  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it('aborts a /api/proxykey fetch after 10 s and frees the single flight for the next caller', async () => {
    vi.resetModules();
    vi.useFakeTimers();
    const signals: AbortSignal[] = [];
    const fetchMock = vi.fn(
      (_url: string, init: RequestInit) =>
        new Promise<Response>((_resolve, reject) => {
          signals.push(init.signal!);
          init.signal?.addEventListener('abort', () => reject(new DOMException('aborted', 'AbortError')));
        }),
    );
    vi.stubGlobal('fetch', fetchMock);
    const { refreshProxyToken } = await import('./proxyTokenClient');
    const first = refreshProxyToken();
    await vi.advanceTimersByTimeAsync(9_999);
    expect(signals[0].aborted).toBe(false);
    await vi.advanceTimersByTimeAsync(1);
    await expect(first).resolves.toBe('');
    expect(signals[0].aborted).toBe(true);
    // The single flight was released: the next caller fetches again instead of joining the hung one.
    void refreshProxyToken();
    expect(fetchMock).toHaveBeenCalledTimes(2);
  });
});
