/**
 * Unit tests for GRVT fetchOHLCV implementation
 * Tests candle data fetching with various parameters
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { GrvtExchange } from '../grvt.exchange';
import { DefaultStore } from '../../../store/store.base';
import { GrvtCandlestickInterval, GrvtKline } from '../grvt.types';
import type { Market } from '../../../types';

describe('GrvtExchange - fetchOHLCV', () => {
  let exchange: GrvtExchange;
  let store: DefaultStore;
  let mockXhr: any;

  // Mock market data
  const ethMarket: Market = {
    id: 'ETH_USDT_Perp',
    symbol: 'ETH_USDT_Perp',
    base: 'ETH',
    quote: 'USDT',
    active: true,
    precision: {
      amount: 4,
      price: 2,
    },
    limits: {
      amount: {
        min: 0.001,
        max: 10000,
      },
      leverage: {
        min: 1,
        max: 20,
      },
    },
  };

  beforeEach(() => {
    store = new DefaultStore();
    store.update({ markets: [ethMarket] });

    exchange = new GrvtExchange(
      'grvt',
      {
        key: 'test_api_key',
        secret: 'test_secret',
        testnet: true,
      },
      store
    );

    // Mock the xhr instance
    mockXhr = {
      post: vi.fn(),
    };
    exchange.xhr = mockXhr as any;
  });

  describe('Interval conversion', () => {
    it('should convert 1m interval to CI_1_M', async () => {
      const mockKlines: GrvtKline[] = [
        {
          timestamp: '2024-01-01T00:00:00.000Z',
          open: '3000',
          high: '3010',
          low: '2995',
          close: '3005',
          volume: '100',
        },
      ];

      mockXhr.post.mockResolvedValue({ data: { result: mockKlines } });

      await exchange.fetchOHLCV({
        symbol: 'ETH_USDT_Perp',
        interval: '1m',
      });

      expect(mockXhr.post).toHaveBeenCalledWith(
        '/lite/v1/kline',
        expect.objectContaining({
          instrument_id: 'ETH_USDT_Perp',
          interval: GrvtCandlestickInterval.CI_1_M,
        })
      );
    });

    it('should convert 3m interval to CI_3_M', async () => {
      mockXhr.post.mockResolvedValue({ data: { result: [] } });

      await exchange.fetchOHLCV({
        symbol: 'ETH_USDT_Perp',
        interval: '3m',
      });

      expect(mockXhr.post).toHaveBeenCalledWith(
        expect.any(String),
        expect.objectContaining({
          interval: GrvtCandlestickInterval.CI_3_M,
        })
      );
    });

    it('should convert 5m interval to CI_5_M', async () => {
      mockXhr.post.mockResolvedValue({ data: { result: [] } });

      await exchange.fetchOHLCV({
        symbol: 'ETH_USDT_Perp',
        interval: '5m',
      });

      expect(mockXhr.post).toHaveBeenCalledWith(
        expect.any(String),
        expect.objectContaining({
          interval: GrvtCandlestickInterval.CI_5_M,
        })
      );
    });

    it('should convert 15m interval to CI_15_M', async () => {
      mockXhr.post.mockResolvedValue({ data: { result: [] } });

      await exchange.fetchOHLCV({
        symbol: 'ETH_USDT_Perp',
        interval: '15m',
      });

      expect(mockXhr.post).toHaveBeenCalledWith(
        expect.any(String),
        expect.objectContaining({
          interval: GrvtCandlestickInterval.CI_15_M,
        })
      );
    });

    it('should convert 30m interval to CI_30_M', async () => {
      mockXhr.post.mockResolvedValue({ data: { result: [] } });

      await exchange.fetchOHLCV({
        symbol: 'ETH_USDT_Perp',
        interval: '30m',
      });

      expect(mockXhr.post).toHaveBeenCalledWith(
        expect.any(String),
        expect.objectContaining({
          interval: GrvtCandlestickInterval.CI_30_M,
        })
      );
    });

    it('should convert 1h interval to CI_1_H', async () => {
      mockXhr.post.mockResolvedValue({ data: { result: [] } });

      await exchange.fetchOHLCV({
        symbol: 'ETH_USDT_Perp',
        interval: '1h',
      });

      expect(mockXhr.post).toHaveBeenCalledWith(
        expect.any(String),
        expect.objectContaining({
          interval: GrvtCandlestickInterval.CI_1_H,
        })
      );
    });

    it('should convert 2h interval to CI_2_H', async () => {
      mockXhr.post.mockResolvedValue({ data: { result: [] } });

      await exchange.fetchOHLCV({
        symbol: 'ETH_USDT_Perp',
        interval: '2h',
      });

      expect(mockXhr.post).toHaveBeenCalledWith(
        expect.any(String),
        expect.objectContaining({
          interval: GrvtCandlestickInterval.CI_2_H,
        })
      );
    });

    it('should convert 4h interval to CI_4_H', async () => {
      mockXhr.post.mockResolvedValue({ data: { result: [] } });

      await exchange.fetchOHLCV({
        symbol: 'ETH_USDT_Perp',
        interval: '4h',
      });

      expect(mockXhr.post).toHaveBeenCalledWith(
        expect.any(String),
        expect.objectContaining({
          interval: GrvtCandlestickInterval.CI_4_H,
        })
      );
    });

    it('should convert 6h interval to CI_6_H', async () => {
      mockXhr.post.mockResolvedValue({ data: { result: [] } });

      await exchange.fetchOHLCV({
        symbol: 'ETH_USDT_Perp',
        interval: '6h',
      });

      expect(mockXhr.post).toHaveBeenCalledWith(
        expect.any(String),
        expect.objectContaining({
          interval: GrvtCandlestickInterval.CI_6_H,
        })
      );
    });

    it('should convert 12h interval to CI_12_H', async () => {
      mockXhr.post.mockResolvedValue({ data: { result: [] } });

      await exchange.fetchOHLCV({
        symbol: 'ETH_USDT_Perp',
        interval: '12h',
      });

      expect(mockXhr.post).toHaveBeenCalledWith(
        expect.any(String),
        expect.objectContaining({
          interval: GrvtCandlestickInterval.CI_12_H,
        })
      );
    });

    it('should convert 1d interval to CI_1_D', async () => {
      mockXhr.post.mockResolvedValue({ data: { result: [] } });

      await exchange.fetchOHLCV({
        symbol: 'ETH_USDT_Perp',
        interval: '1d',
      });

      expect(mockXhr.post).toHaveBeenCalledWith(
        expect.any(String),
        expect.objectContaining({
          interval: GrvtCandlestickInterval.CI_1_D,
        })
      );
    });

    it('should convert 1w interval to CI_1_W', async () => {
      mockXhr.post.mockResolvedValue({ data: { result: [] } });

      await exchange.fetchOHLCV({
        symbol: 'ETH_USDT_Perp',
        interval: '1w',
      });

      expect(mockXhr.post).toHaveBeenCalledWith(
        expect.any(String),
        expect.objectContaining({
          interval: GrvtCandlestickInterval.CI_1_W,
        })
      );
    });
  });

  describe('Time range parameters', () => {
    it('should pass from parameter as ISO string', async () => {
      mockXhr.post.mockResolvedValue({ data: { result: [] } });

      const fromTime = 1704067200000; // 2024-01-01 00:00:00 UTC
      await exchange.fetchOHLCV({
        symbol: 'ETH_USDT_Perp',
        interval: '1h',
        from: fromTime,
      });

      expect(mockXhr.post).toHaveBeenCalledWith(
        expect.any(String),
        expect.objectContaining({
          start_time: '2024-01-01T00:00:00.000Z',
        })
      );
    });

    it('should pass to parameter as ISO string', async () => {
      mockXhr.post.mockResolvedValue({ data: { result: [] } });

      const toTime = 1704153600000; // 2024-01-02 00:00:00 UTC
      await exchange.fetchOHLCV({
        symbol: 'ETH_USDT_Perp',
        interval: '1h',
        to: toTime,
      });

      expect(mockXhr.post).toHaveBeenCalledWith(
        expect.any(String),
        expect.objectContaining({
          end_time: '2024-01-02T00:00:00.000Z',
        })
      );
    });

    it('should pass both from and to parameters', async () => {
      mockXhr.post.mockResolvedValue({ data: { result: [] } });

      const fromTime = 1704067200000;
      const toTime = 1704153600000;

      await exchange.fetchOHLCV({
        symbol: 'ETH_USDT_Perp',
        interval: '1h',
        from: fromTime,
        to: toTime,
      });

      expect(mockXhr.post).toHaveBeenCalledWith(
        expect.any(String),
        expect.objectContaining({
          start_time: '2024-01-01T00:00:00.000Z',
          end_time: '2024-01-02T00:00:00.000Z',
        })
      );
    });

    it('should pass limit parameter', async () => {
      mockXhr.post.mockResolvedValue({ data: { result: [] } });

      await exchange.fetchOHLCV({
        symbol: 'ETH_USDT_Perp',
        interval: '1h',
        limit: 100,
      });

      expect(mockXhr.post).toHaveBeenCalledWith(
        expect.any(String),
        expect.objectContaining({
          limit: 100,
        })
      );
    });
  });

  describe('Data transformation', () => {
    it('should correctly transform GRVT klines to safe-cex format', async () => {
      const mockKlines: GrvtKline[] = [
        {
          timestamp: '2024-01-01T00:00:00.000Z',
          open: '3050.25',
          high: '3100.75',
          low: '3000.50',
          close: '3080.00',
          volume: '150.5',
        },
        {
          timestamp: '2024-01-01T01:00:00.000Z',
          open: '3080.00',
          high: '3120.50',
          low: '3070.00',
          close: '3110.25',
          volume: '180.25',
        },
      ];

      mockXhr.post.mockResolvedValue({ data: { result: mockKlines } });

      const candles = await exchange.fetchOHLCV({
        symbol: 'ETH_USDT_Perp',
        interval: '1h',
      });

      expect(candles).toHaveLength(2);

      // First candle
      expect(candles[0].timestamp).toBe(new Date('2024-01-01T00:00:00.000Z').getTime());
      expect(candles[0].open).toBe(3050.25);
      expect(candles[0].high).toBe(3100.75);
      expect(candles[0].low).toBe(3000.50);
      expect(candles[0].close).toBe(3080.00);
      expect(candles[0].volume).toBe(150.5);

      // Second candle
      expect(candles[1].timestamp).toBe(new Date('2024-01-01T01:00:00.000Z').getTime());
      expect(candles[1].open).toBe(3080.00);
      expect(candles[1].high).toBe(3120.50);
      expect(candles[1].low).toBe(3070.00);
      expect(candles[1].close).toBe(3110.25);
      expect(candles[1].volume).toBe(180.25);
    });

    it('should handle string numbers with high precision', async () => {
      const mockKlines: GrvtKline[] = [
        {
          timestamp: '2024-01-01T00:00:00.000Z',
          open: '3000.123456',
          high: '3000.987654',
          low: '2999.111111',
          close: '3000.555555',
          volume: '12345.678901',
        },
      ];

      mockXhr.post.mockResolvedValue({ data: { result: mockKlines } });

      const candles = await exchange.fetchOHLCV({
        symbol: 'ETH_USDT_Perp',
        interval: '1m',
      });

      expect(candles[0].open).toBeCloseTo(3000.123456, 6);
      expect(candles[0].high).toBeCloseTo(3000.987654, 6);
      expect(candles[0].low).toBeCloseTo(2999.111111, 6);
      expect(candles[0].close).toBeCloseTo(3000.555555, 6);
      expect(candles[0].volume).toBeCloseTo(12345.678901, 6);
    });
  });

  describe('Error handling', () => {
    it('should return empty array for unknown symbol', async () => {
      const candles = await exchange.fetchOHLCV({
        symbol: 'UNKNOWN_USDT_Perp',
        interval: '1h',
      });

      expect(candles).toEqual([]);
      expect(mockXhr.post).not.toHaveBeenCalled();
    });

    it('should return empty array when API returns null data', async () => {
      mockXhr.post.mockResolvedValue({ data: null });

      const candles = await exchange.fetchOHLCV({
        symbol: 'ETH_USDT_Perp',
        interval: '1h',
      });

      expect(candles).toEqual([]);
    });

    it('should return empty array when API returns no result', async () => {
      mockXhr.post.mockResolvedValue({ data: { result: null } });

      const candles = await exchange.fetchOHLCV({
        symbol: 'ETH_USDT_Perp',
        interval: '1h',
      });

      expect(candles).toEqual([]);
    });

    it('should return empty array when API returns empty result', async () => {
      mockXhr.post.mockResolvedValue({ data: { result: [] } });

      const candles = await exchange.fetchOHLCV({
        symbol: 'ETH_USDT_Perp',
        interval: '1h',
      });

      expect(candles).toEqual([]);
    });

    it('should throw error when API request fails', async () => {
      mockXhr.post.mockRejectedValue(new Error('Network error'));

      await expect(
        exchange.fetchOHLCV({
          symbol: 'ETH_USDT_Perp',
          interval: '1h',
        })
      ).rejects.toThrow('Network error');
    });
  });

  describe('API endpoint', () => {
    it('should call correct endpoint', async () => {
      mockXhr.post.mockResolvedValue({ data: { result: [] } });

      await exchange.fetchOHLCV({
        symbol: 'ETH_USDT_Perp',
        interval: '1h',
      });

      expect(mockXhr.post).toHaveBeenCalledWith('/lite/v1/kline', expect.any(Object));
    });

    it('should use instrument_id in request', async () => {
      mockXhr.post.mockResolvedValue({ data: { result: [] } });

      await exchange.fetchOHLCV({
        symbol: 'ETH_USDT_Perp',
        interval: '1h',
      });

      expect(mockXhr.post).toHaveBeenCalledWith(
        expect.any(String),
        expect.objectContaining({
          instrument_id: 'ETH_USDT_Perp',
        })
      );
    });

    it('should use market ID instead of symbol', async () => {
      // Test that it uses market.id
      const btcMarket: Market = {
        id: 'BTC_USDT_Perp',
        symbol: 'BTCUSDT', // Different from ID
        base: 'BTC',
        quote: 'USDT',
        active: true,
        precision: { amount: 5, price: 2 },
        limits: {
          amount: { min: 0.001, max: 1000 },
          leverage: { min: 1, max: 20 },
        },
      };

      store.update({ markets: [btcMarket] });
      mockXhr.post.mockResolvedValue({ data: { result: [] } });

      await exchange.fetchOHLCV({
        symbol: 'BTCUSDT',
        interval: '1h',
      });

      // Should use market.id (BTC_USDT_Perp), not symbol (BTCUSDT)
      expect(mockXhr.post).toHaveBeenCalledWith(
        expect.any(String),
        expect.objectContaining({
          instrument_id: 'BTC_USDT_Perp',
        })
      );
    });
  });

  describe('Multiple candles', () => {
    it('should handle 100 candles correctly', async () => {
      const mockKlines: GrvtKline[] = Array.from({ length: 100 }, (_, i) => ({
        timestamp: new Date(1704067200000 + i * 60000).toISOString(),
        open: `${3000 + i}`,
        high: `${3010 + i}`,
        low: `${2995 + i}`,
        close: `${3005 + i}`,
        volume: `${100 + i}`,
      }));

      mockXhr.post.mockResolvedValue({ data: { result: mockKlines } });

      const candles = await exchange.fetchOHLCV({
        symbol: 'ETH_USDT_Perp',
        interval: '1m',
        limit: 100,
      });

      expect(candles).toHaveLength(100);
      expect(candles[0].open).toBe(3000);
      expect(candles[99].open).toBe(3099);
    });

    it('should handle 500 candles correctly', async () => {
      const mockKlines: GrvtKline[] = Array.from({ length: 500 }, (_, i) => ({
        timestamp: new Date(1704067200000 + i * 60000).toISOString(),
        open: `${3000 + (i % 100)}`,
        high: `${3010 + (i % 100)}`,
        low: `${2995 + (i % 100)}`,
        close: `${3005 + (i % 100)}`,
        volume: `${100 + i}`,
      }));

      mockXhr.post.mockResolvedValue({ data: { result: mockKlines } });

      const candles = await exchange.fetchOHLCV({
        symbol: 'ETH_USDT_Perp',
        interval: '1m',
        limit: 500,
      });

      expect(candles).toHaveLength(500);
    });
  });

  describe('Timestamp handling', () => {
    it('should convert ISO timestamp to milliseconds', async () => {
      const mockKlines: GrvtKline[] = [
        {
          timestamp: '2024-01-01T00:00:00.000Z',
          open: '3000',
          high: '3010',
          low: '2995',
          close: '3005',
          volume: '100',
        },
      ];

      mockXhr.post.mockResolvedValue({ data: { result: mockKlines } });

      const candles = await exchange.fetchOHLCV({
        symbol: 'ETH_USDT_Perp',
        interval: '1h',
      });

      expect(candles[0].timestamp).toBe(1704067200000);
    });

    it('should handle ISO timestamps with different formats', async () => {
      const mockKlines: GrvtKline[] = [
        {
          timestamp: '2024-01-01T12:34:56.789Z',
          open: '3000',
          high: '3010',
          low: '2995',
          close: '3005',
          volume: '100',
        },
      ];

      mockXhr.post.mockResolvedValue({ data: { result: mockKlines } });

      const candles = await exchange.fetchOHLCV({
        symbol: 'ETH_USDT_Perp',
        interval: '1h',
      });

      expect(candles[0].timestamp).toBe(new Date('2024-01-01T12:34:56.789Z').getTime());
    });
  });
});
