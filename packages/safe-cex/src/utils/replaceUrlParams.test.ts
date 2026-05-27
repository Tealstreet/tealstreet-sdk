import { describe, expect, it } from 'vitest';

import { replaceUrlParams } from './replaceUrlParams';

describe('replaceUrlParams', () => {
  describe('basic functionality', () => {
    it('should replace a single param in url', () => {
      const result = replaceUrlParams('/api/{symbol}/orders', { symbol: 'BTCUSDT' });
      expect(result.url).toBe('/api/BTCUSDT/orders');
      expect(result.params).toEqual({});
    });

    it('should replace multiple params in url', () => {
      const result = replaceUrlParams('/api/{exchange}/{symbol}/orders', {
        exchange: 'binance',
        symbol: 'BTCUSDT',
      });
      expect(result.url).toBe('/api/binance/BTCUSDT/orders');
      expect(result.params).toEqual({});
    });

    it('should keep non-path params in params object', () => {
      const result = replaceUrlParams('/api/{symbol}/orders', {
        symbol: 'BTCUSDT',
        limit: 100,
        side: 'buy',
      });
      expect(result.url).toBe('/api/BTCUSDT/orders');
      expect(result.params).toEqual({ limit: 100, side: 'buy' });
    });

    it('should handle numeric param values', () => {
      const result = replaceUrlParams('/api/orders/{orderId}', { orderId: 12345 });
      expect(result.url).toBe('/api/orders/12345');
      expect(result.params).toEqual({});
    });
  });

  describe('edge cases', () => {
    it('should return original url and params when no placeholders', () => {
      const result = replaceUrlParams('/api/orders', { limit: 10 });
      expect(result.url).toBe('/api/orders');
      expect(result.params).toEqual({ limit: 10 });
    });

    it('should handle undefined url', () => {
      const result = replaceUrlParams(undefined, { symbol: 'BTCUSDT' });
      expect(result.url).toBeUndefined();
      expect(result.params).toEqual({ symbol: 'BTCUSDT' });
    });

    it('should handle empty params', () => {
      const result = replaceUrlParams('/api/{symbol}/orders', {});
      expect(result.url).toBe('/api/{symbol}/orders');
      expect(result.params).toEqual({});
    });

    it('should handle no params argument', () => {
      const result = replaceUrlParams('/api/{symbol}/orders');
      expect(result.url).toBe('/api/{symbol}/orders');
      expect(result.params).toEqual({});
    });

    it('should not replace param if not in params object', () => {
      const result = replaceUrlParams('/api/{symbol}/{side}/orders', {
        symbol: 'BTCUSDT',
      });
      expect(result.url).toBe('/api/BTCUSDT/{side}/orders');
      expect(result.params).toEqual({});
    });

    it('should not mutate original params object', () => {
      const originalParams = { symbol: 'BTCUSDT', limit: 10 };
      replaceUrlParams('/api/{symbol}/orders', originalParams);
      expect(originalParams).toEqual({ symbol: 'BTCUSDT', limit: 10 });
    });
  });

  describe('param matching', () => {
    it('should match alphanumeric param names', () => {
      const result = replaceUrlParams('/api/{symbol123}/orders', { symbol123: 'BTCUSDT' });
      expect(result.url).toBe('/api/BTCUSDT/orders');
    });

    it('should match camelCase param names', () => {
      const result = replaceUrlParams('/api/{orderType}/orders', { orderType: 'limit' });
      expect(result.url).toBe('/api/limit/orders');
    });

    it('should match underscore param names', () => {
      const result = replaceUrlParams('/api/{order_type}/orders', { order_type: 'limit' });
      expect(result.url).toBe('/api/limit/orders');
    });

    it('should replace all occurrences of the same placeholder', () => {
      const result = replaceUrlParams('/api/{symbol}/{symbol}/orders', { symbol: 'BTCUSDT' });
      expect(result.url).toBe('/api/BTCUSDT/BTCUSDT/orders');
    });
  });
});
