import jwt from 'jsonwebtoken';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { currentProxyToken, isTokenExpired, setProxyToken, setTokenRefreshFunction } from './accessToken';

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
});
