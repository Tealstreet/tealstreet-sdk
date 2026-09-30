import jwt from 'jsonwebtoken';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import {
  currentProxyToken,
  getProxyToken,
  isTokenExpired,
  ProxyTokenUnavailableError,
  refreshProxyToken,
  setContextualProxyTokenProvider,
  setProxyToken,
  setTokenRefreshFunction,
} from './accessToken';

vi.mock('./utils/virtualClock', () => ({
  virtualClock: {
    getCurrentTimeMs: vi.fn(() => Date.now()),
  },
}));

vi.mock('./proxyTokenClient', () => ({
  getCurrentToken: vi.fn(() => ''),
  getProxyToken: vi.fn(() => Promise.resolve('')),
  initializeProxyTokenClient: vi.fn(() => Promise.resolve()),
  refreshProxyToken: vi.fn(() => Promise.resolve('')),
  setToken: vi.fn(),
}));

describe('accessToken', () => {
  const createFutureToken = (label: string, now = Date.now()) =>
    jwt.sign({ exp: Math.floor(now / 1000) + 3600, label }, 'test-secret');

  describe('isTokenExpired', () => {
    it('should return true for empty token', () => {
      expect(isTokenExpired('')).toBe(true);
    });

    it('should return true for null/undefined token', () => {
      expect(isTokenExpired(null as any)).toBe(true);
      expect(isTokenExpired(undefined as any)).toBe(true);
    });

    it('should return true for invalid JWT token', () => {
      expect(isTokenExpired('not-a-valid-jwt')).toBe(true);
    });

    it('should return false for valid non-expired token', async () => {
      const { virtualClock } = await import('./utils/virtualClock');
      // Set virtual clock to a fixed time
      const now = 1700000000000; // Nov 2023
      vi.mocked(virtualClock.getCurrentTimeMs).mockReturnValue(now);

      // Create a token that expires in the future
      const futureExpiry = Math.floor(now / 1000) + 3600; // 1 hour from now
      const token = jwt.sign({ exp: futureExpiry }, 'test-secret');

      expect(isTokenExpired(token)).toBe(false);
    });

    it('should return true for expired token', async () => {
      const { virtualClock } = await import('./utils/virtualClock');
      // Set virtual clock to a fixed time
      const now = 1700000000000; // Nov 2023
      vi.mocked(virtualClock.getCurrentTimeMs).mockReturnValue(now);

      // Create a token that expired in the past
      const pastExpiry = Math.floor(now / 1000) - 3600; // 1 hour ago
      const token = jwt.sign({ exp: pastExpiry }, 'test-secret');

      expect(isTokenExpired(token)).toBe(true);
    });

    it('should return true for token without exp claim', () => {
      // Token without expiration (no exp field)
      const payload = { sub: 'user123' };
      const token = jwt.sign(payload, 'test-secret', { noTimestamp: true });

      expect(isTokenExpired(token)).toBe(true);
    });

    it('should return true for token with exp=0', () => {
      const token = jwt.sign({ exp: 0 }, 'test-secret');
      expect(isTokenExpired(token)).toBe(true);
    });
  });

  describe('setProxyToken', () => {
    beforeEach(() => {
      vi.clearAllMocks();
    });

    it('should set the token', async () => {
      const { setToken } = await import('./proxyTokenClient');
      setProxyToken('new-token');
      expect(setToken).toHaveBeenCalledWith('new-token');
    });
  });

  describe('setTokenRefreshFunction', () => {
    afterEach(() => {
      // Reset the custom refresh function
      setTokenRefreshFunction(null);
    });

    it('should accept a custom refresh function', () => {
      const mockFn = vi.fn().mockResolvedValue('new-token');
      expect(() => setTokenRefreshFunction(mockFn)).not.toThrow();
    });

    it('should accept null to clear custom function', () => {
      const mockFn = vi.fn().mockResolvedValue('new-token');
      setTokenRefreshFunction(mockFn);
      expect(() => setTokenRefreshFunction(null)).not.toThrow();
    });
  });

  describe('setContextualProxyTokenProvider', () => {
    beforeEach(() => {
      setProxyToken('');
    });

    afterEach(() => {
      setContextualProxyTokenProvider(null);
      setTokenRefreshFunction(null);
      setProxyToken('');
    });

    it('uses the active contextual provider before the module-global refresh function', async () => {
      const contextToken = createFutureToken('context-token');
      const globalRefresh = vi.fn().mockResolvedValue(createFutureToken('global-token'));
      const contextProvider = vi.fn().mockResolvedValue(contextToken);

      setTokenRefreshFunction(globalRefresh);
      setContextualProxyTokenProvider(() => contextProvider);

      await expect(getProxyToken()).resolves.toBe(contextToken);
      expect(contextProvider).toHaveBeenCalledTimes(1);
      expect(globalRefresh).not.toHaveBeenCalled();
    });

    // The three cases below used to fall back to the module-global token. That
    // is what shipped EXCHANGE-SERVER-27: on a multi-tenant host the global is
    // empty, so the fallback meant sending a signed request with no
    // Authorization header and letting the venue answer 401. An installed
    // provider owns the credential; if it cannot produce one, nothing else may.

    it('throws rather than falling back when the contextual provider returns an empty token', async () => {
      const globalRefresh = vi.fn().mockResolvedValue(createFutureToken('global-token'));
      const contextProvider = vi.fn().mockResolvedValue('');

      setTokenRefreshFunction(globalRefresh);
      setContextualProxyTokenProvider(() => contextProvider);

      await expect(getProxyToken()).rejects.toBeInstanceOf(ProxyTokenUnavailableError);
      expect(contextProvider).toHaveBeenCalledTimes(1);
      expect(globalRefresh).not.toHaveBeenCalled();
    });

    it('throws rather than falling back when the contextual provider returns an expired token', async () => {
      const globalRefresh = vi.fn().mockResolvedValue(createFutureToken('global-token'));
      const contextProvider = vi.fn().mockResolvedValue(jwt.sign({ exp: 1 }, 'test-secret'));

      setTokenRefreshFunction(globalRefresh);
      setContextualProxyTokenProvider(() => contextProvider);

      await expect(getProxyToken()).rejects.toBeInstanceOf(ProxyTokenUnavailableError);
      expect(contextProvider).toHaveBeenCalledTimes(1);
      expect(globalRefresh).not.toHaveBeenCalled();
    });

    it('throws rather than falling back when the contextual provider throws', async () => {
      const globalRefresh = vi.fn().mockResolvedValue(createFutureToken('global-token'));
      const contextProvider = vi.fn().mockRejectedValue(new Error('provider failed'));

      setTokenRefreshFunction(globalRefresh);
      setContextualProxyTokenProvider(() => contextProvider);

      await expect(getProxyToken()).rejects.toBeInstanceOf(ProxyTokenUnavailableError);
      expect(contextProvider).toHaveBeenCalledTimes(1);
      expect(globalRefresh).not.toHaveBeenCalled();
    });

    it('does not leak the provider failure into the thrown message', async () => {
      const contextProvider = vi.fn().mockRejectedValue(new Error('token=eyJhbGciOi.secret.value'));

      setContextualProxyTokenProvider(() => contextProvider);

      await expect(getProxyToken()).rejects.toThrow('proxy token provider failed');
      await expect(getProxyToken()).rejects.not.toThrow(/secret\.value/);
    });

    it('applies the same rule to refreshProxyToken', async () => {
      const globalRefresh = vi.fn().mockResolvedValue(createFutureToken('global-token'));
      const contextProvider = vi.fn().mockResolvedValue('');

      setTokenRefreshFunction(globalRefresh);
      setContextualProxyTokenProvider(() => contextProvider);

      await expect(refreshProxyToken()).rejects.toBeInstanceOf(ProxyTokenUnavailableError);
      expect(globalRefresh).not.toHaveBeenCalled();
    });

    it('uses global refresh after clearing the contextual provider', async () => {
      const globalToken = createFutureToken('global-token');
      const globalRefresh = vi.fn().mockResolvedValue(globalToken);
      const contextProvider = vi.fn().mockResolvedValue(createFutureToken('context-token'));

      setTokenRefreshFunction(globalRefresh);
      setContextualProxyTokenProvider(() => contextProvider);
      setContextualProxyTokenProvider(null);

      await expect(getProxyToken()).resolves.toBe(globalToken);
      expect(contextProvider).not.toHaveBeenCalled();
      expect(globalRefresh).toHaveBeenCalledTimes(1);
    });
  });
});
