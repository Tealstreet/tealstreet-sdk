/**
 * Unit tests for Backpack fetchOHLCV implementation
 * Tests candle data fetching with various parameters
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { BackpackExchange } from '../backpack.exchange';
import { DefaultStore } from '../../../store/store.base';
import { BackpackKline } from '../backpack.types';
import type { Market } from '../../../types';

// Helper to convert timestamp to ISO date string (format: "2024-01-01 00:00:00")
const toISODateString = (ts: number): string => {
  const d = new Date(ts);
  return d.toISOString().replace('T', ' ').replace('Z', '').slice(0, 19);
};

describe('BackpackExchange - fetchOHLCV', () => {
  let exchange: BackpackExchange;
  let store: DefaultStore;
  let mockXhr: any;

  // Mock market data
  const solMarket: Market = {
    id: 'SOL_USDC',
    symbol: 'SOL_USDC',
    base: 'SOL',
    quote: 'USDC',
    active: true,
    precision: {
      amount: 2,
      price: 2,
    },
    limits: {
      amount: {
        min: 0.01,
        max: 100000,
      },
      leverage: {
        min: 1,
        max: 10,
      },
    },
  };

  beforeEach(() => {
    store = new DefaultStore();
    store.update({ markets: [solMarket] });

    exchange = new BackpackExchange(
      'backpack',
      {
        key: 'test_api_key',
        secret: 'test_secret',
        testnet: false,
      },
      store
    );

    // Mock the xhr instance
    mockXhr = {
      get: vi.fn(),
    };
    exchange.xhr = mockXhr as any;
  });

  describe('Interval support', () => {
    it('should pass 1m interval correctly', async () => {
      mockXhr.get.mockResolvedValue({ data: [] });

      await exchange.fetchOHLCV({
        symbol: 'SOL_USDC',
        interval: '1m',
      });

      expect(mockXhr.get).toHaveBeenCalledWith(
        '/api/v1/klines',
        expect.objectContaining({
          params: expect.objectContaining({
            symbol: 'SOL_USDC',
            interval: '1m',
          }),
        })
      );
    });

    it('should pass 3m interval correctly', async () => {
      mockXhr.get.mockResolvedValue({ data: [] });

      await exchange.fetchOHLCV({
        symbol: 'SOL_USDC',
        interval: '3m',
      });

      expect(mockXhr.get).toHaveBeenCalledWith(
        '/api/v1/klines',
        expect.objectContaining({
          params: expect.objectContaining({
            interval: '3m',
          }),
        })
      );
    });

    it('should pass 5m interval correctly', async () => {
      mockXhr.get.mockResolvedValue({ data: [] });

      await exchange.fetchOHLCV({
        symbol: 'SOL_USDC',
        interval: '5m',
      });

      expect(mockXhr.get).toHaveBeenCalledWith(
        '/api/v1/klines',
        expect.objectContaining({
          params: expect.objectContaining({
            interval: '5m',
          }),
        })
      );
    });

    it('should pass 15m interval correctly', async () => {
      mockXhr.get.mockResolvedValue({ data: [] });

      await exchange.fetchOHLCV({
        symbol: 'SOL_USDC',
        interval: '15m',
      });

      expect(mockXhr.get).toHaveBeenCalledWith(
        '/api/v1/klines',
        expect.objectContaining({
          params: expect.objectContaining({
            interval: '15m',
          }),
        })
      );
    });

    it('should pass 30m interval correctly', async () => {
      mockXhr.get.mockResolvedValue({ data: [] });

      await exchange.fetchOHLCV({
        symbol: 'SOL_USDC',
        interval: '30m',
      });

      expect(mockXhr.get).toHaveBeenCalledWith(
        '/api/v1/klines',
        expect.objectContaining({
          params: expect.objectContaining({
            interval: '30m',
          }),
        })
      );
    });

    it('should pass 1h interval correctly', async () => {
      mockXhr.get.mockResolvedValue({ data: [] });

      await exchange.fetchOHLCV({
        symbol: 'SOL_USDC',
        interval: '1h',
      });

      expect(mockXhr.get).toHaveBeenCalledWith(
        '/api/v1/klines',
        expect.objectContaining({
          params: expect.objectContaining({
            interval: '1h',
          }),
        })
      );
    });

    it('should pass 4h interval correctly', async () => {
      mockXhr.get.mockResolvedValue({ data: [] });

      await exchange.fetchOHLCV({
        symbol: 'SOL_USDC',
        interval: '4h',
      });

      expect(mockXhr.get).toHaveBeenCalledWith(
        '/api/v1/klines',
        expect.objectContaining({
          params: expect.objectContaining({
            interval: '4h',
          }),
        })
      );
    });

    it('should pass 1d interval correctly', async () => {
      mockXhr.get.mockResolvedValue({ data: [] });

      await exchange.fetchOHLCV({
        symbol: 'SOL_USDC',
        interval: '1d',
      });

      expect(mockXhr.get).toHaveBeenCalledWith(
        '/api/v1/klines',
        expect.objectContaining({
          params: expect.objectContaining({
            interval: '1d',
          }),
        })
      );
    });

    it('should pass 1w interval correctly', async () => {
      mockXhr.get.mockResolvedValue({ data: [] });

      await exchange.fetchOHLCV({
        symbol: 'SOL_USDC',
        interval: '1w',
      });

      expect(mockXhr.get).toHaveBeenCalledWith(
        '/api/v1/klines',
        expect.objectContaining({
          params: expect.objectContaining({
            interval: '1w',
          }),
        })
      );
    });
  });

  describe('Time range parameters', () => {
    it('should pass from parameter as startTime in seconds', async () => {
      mockXhr.get.mockResolvedValue({ data: [] });

      const fromTime = 1704067200000; // 2024-01-01 00:00:00 UTC
      await exchange.fetchOHLCV({
        symbol: 'SOL_USDC',
        interval: '1h',
        from: fromTime,
      });

      expect(mockXhr.get).toHaveBeenCalledWith(
        '/api/v1/klines',
        expect.objectContaining({
          params: expect.objectContaining({
            startTime: Math.floor(fromTime / 1000), // API expects seconds
          }),
        })
      );
    });

    it('should pass to parameter as endTime in seconds', async () => {
      mockXhr.get.mockResolvedValue({ data: [] });

      const toTime = 1704153600000; // 2024-01-02 00:00:00 UTC
      await exchange.fetchOHLCV({
        symbol: 'SOL_USDC',
        interval: '1h',
        to: toTime,
      });

      expect(mockXhr.get).toHaveBeenCalledWith(
        '/api/v1/klines',
        expect.objectContaining({
          params: expect.objectContaining({
            endTime: Math.floor(toTime / 1000), // API expects seconds
          }),
        })
      );
    });

    it('should pass both from and to parameters in seconds', async () => {
      mockXhr.get.mockResolvedValue({ data: [] });

      const fromTime = 1704067200000;
      const toTime = 1704153600000;

      await exchange.fetchOHLCV({
        symbol: 'SOL_USDC',
        interval: '1h',
        from: fromTime,
        to: toTime,
      });

      expect(mockXhr.get).toHaveBeenCalledWith(
        '/api/v1/klines',
        expect.objectContaining({
          params: expect.objectContaining({
            startTime: Math.floor(fromTime / 1000),
            endTime: Math.floor(toTime / 1000),
          }),
        })
      );
    });
  });

  describe('Data transformation', () => {
    it('should correctly transform Backpack klines to safe-cex format', async () => {
      const mockKlines: BackpackKline[] = [
        {
          start: '2024-01-01 00:00:00', // ISO date string
          open: '100.50',
          high: '105.75',
          low: '99.25',
          close: '103.00',
          volume: '15000.50',
          quoteVolume: '1500000',
          end: '2024-01-01 01:00:00',
          trades: '150',
        },
        {
          start: '2024-01-01 01:00:00',
          open: '103.00',
          high: '108.25',
          low: '102.50',
          close: '106.75',
          volume: '18500.25',
          quoteVolume: '1900000',
          end: '2024-01-01 02:00:00',
          trades: '200',
        },
      ];

      mockXhr.get.mockResolvedValue({ data: mockKlines });

      const candles = await exchange.fetchOHLCV({
        symbol: 'SOL_USDC',
        interval: '1h',
      });

      expect(candles).toHaveLength(2);

      // First candle - timestamp parsed from ISO string
      expect(candles[0].timestamp).toBe(new Date('2024-01-01T00:00:00Z').getTime());
      expect(candles[0].open).toBe(100.50);
      expect(candles[0].high).toBe(105.75);
      expect(candles[0].low).toBe(99.25);
      expect(candles[0].close).toBe(103.00);
      expect(candles[0].volume).toBe(15000.50);

      // Second candle
      expect(candles[1].timestamp).toBe(new Date('2024-01-01T01:00:00Z').getTime());
      expect(candles[1].open).toBe(103.00);
      expect(candles[1].high).toBe(108.25);
      expect(candles[1].low).toBe(102.50);
      expect(candles[1].close).toBe(106.75);
      expect(candles[1].volume).toBe(18500.25);
    });

    it('should handle string numbers with high precision', async () => {
      const mockKlines: BackpackKline[] = [
        {
          start: '2024-01-01 00:00:00',
          open: '100.123456',
          high: '100.987654',
          low: '99.111111',
          close: '100.555555',
          volume: '12345.678901',
          quoteVolume: '1234567.89',
          end: '2024-01-01 01:00:00',
          trades: '50',
        },
      ];

      mockXhr.get.mockResolvedValue({ data: mockKlines });

      const candles = await exchange.fetchOHLCV({
        symbol: 'SOL_USDC',
        interval: '1m',
      });

      expect(candles[0].open).toBeCloseTo(100.123456, 6);
      expect(candles[0].high).toBeCloseTo(100.987654, 6);
      expect(candles[0].low).toBeCloseTo(99.111111, 6);
      expect(candles[0].close).toBeCloseTo(100.555555, 6);
      expect(candles[0].volume).toBeCloseTo(12345.678901, 6);
    });

    it('should handle zero volume candles', async () => {
      const mockKlines: BackpackKline[] = [
        {
          start: '2024-01-01 00:00:00',
          open: '100.00',
          high: '100.00',
          low: '100.00',
          close: '100.00',
          volume: '0',
          quoteVolume: '0',
          end: '2024-01-01 01:00:00',
          trades: '0',
        },
      ];

      mockXhr.get.mockResolvedValue({ data: mockKlines });

      const candles = await exchange.fetchOHLCV({
        symbol: 'SOL_USDC',
        interval: '1m',
      });

      expect(candles[0].volume).toBe(0);
      expect(candles[0].open).toBe(100.00);
    });
  });

  describe('Empty results', () => {
    it('should return empty array when no candles', async () => {
      mockXhr.get.mockResolvedValue({ data: [] });

      const candles = await exchange.fetchOHLCV({
        symbol: 'SOL_USDC',
        interval: '1h',
      });

      expect(candles).toEqual([]);
    });

    it('should handle empty response gracefully', async () => {
      mockXhr.get.mockResolvedValue({ data: [] });

      const candles = await exchange.fetchOHLCV({
        symbol: 'SOL_USDC',
        interval: '1m',
        from: 1704067200000,
        to: 1704070800000,
      });

      expect(candles).toEqual([]);
    });
  });

  describe('Error handling', () => {
    it('should throw error when API request fails', async () => {
      mockXhr.get.mockRejectedValue(new Error('Network error'));

      await expect(
        exchange.fetchOHLCV({
          symbol: 'SOL_USDC',
          interval: '1h',
        })
      ).rejects.toThrow('Network error');
    });

    it('should throw error on 400 response', async () => {
      mockXhr.get.mockRejectedValue(new Error('Bad Request'));

      await expect(
        exchange.fetchOHLCV({
          symbol: 'INVALID',
          interval: '1h',
        })
      ).rejects.toThrow('Bad Request');
    });

    it('should throw error on 500 response', async () => {
      mockXhr.get.mockRejectedValue(new Error('Internal Server Error'));

      await expect(
        exchange.fetchOHLCV({
          symbol: 'SOL_USDC',
          interval: '1h',
        })
      ).rejects.toThrow('Internal Server Error');
    });
  });

  describe('API endpoint', () => {
    it('should call correct endpoint', async () => {
      mockXhr.get.mockResolvedValue({ data: [] });

      await exchange.fetchOHLCV({
        symbol: 'SOL_USDC',
        interval: '1h',
      });

      expect(mockXhr.get).toHaveBeenCalledWith('/api/v1/klines', expect.any(Object));
    });

    it('should include symbol in request params', async () => {
      mockXhr.get.mockResolvedValue({ data: [] });

      await exchange.fetchOHLCV({
        symbol: 'SOL_USDC',
        interval: '1h',
      });

      expect(mockXhr.get).toHaveBeenCalledWith(
        expect.any(String),
        expect.objectContaining({
          params: expect.objectContaining({
            symbol: 'SOL_USDC',
          }),
        })
      );
    });

    it('should handle different symbols correctly', async () => {
      const btcMarket: Market = {
        id: 'BTC_USDC',
        symbol: 'BTC_USDC',
        base: 'BTC',
        quote: 'USDC',
        active: true,
        precision: { amount: 5, price: 2 },
        limits: {
          amount: { min: 0.00001, max: 1000 },
          leverage: { min: 1, max: 10 },
        },
      };

      store.update({ markets: [solMarket, btcMarket] });
      mockXhr.get.mockResolvedValue({ data: [] });

      await exchange.fetchOHLCV({
        symbol: 'BTC_USDC',
        interval: '1h',
      });

      expect(mockXhr.get).toHaveBeenCalledWith(
        expect.any(String),
        expect.objectContaining({
          params: expect.objectContaining({
            symbol: 'BTC_USDC',
          }),
        })
      );
    });
  });

  describe('Multiple candles', () => {
    it('should handle 100 candles correctly', async () => {
      const baseTime = new Date('2024-01-01T00:00:00Z').getTime();
      const mockKlines: BackpackKline[] = Array.from({ length: 100 }, (_, i) => ({
        start: toISODateString(baseTime + i * 60000),
        open: `${100 + i}`,
        high: `${105 + i}`,
        low: `${99 + i}`,
        close: `${103 + i}`,
        volume: `${1000 + i * 10}`,
        quoteVolume: `${100000 + i * 1000}`,
        end: toISODateString(baseTime + (i + 1) * 60000),
        trades: `${50 + i}`,
      }));

      mockXhr.get.mockResolvedValue({ data: mockKlines });

      const candles = await exchange.fetchOHLCV({
        symbol: 'SOL_USDC',
        interval: '1m',
        limit: 100,
      });

      expect(candles).toHaveLength(100);
      expect(candles[0].open).toBe(100);
      expect(candles[99].open).toBe(199);
    });

    it('should handle 1000 candles correctly', async () => {
      const baseTime = new Date('2024-01-01T00:00:00Z').getTime();
      const mockKlines: BackpackKline[] = Array.from({ length: 1000 }, (_, i) => ({
        start: toISODateString(baseTime + i * 60000),
        open: `${100 + (i % 100)}`,
        high: `${105 + (i % 100)}`,
        low: `${99 + (i % 100)}`,
        close: `${103 + (i % 100)}`,
        volume: `${1000 + i}`,
        quoteVolume: `${100000 + i * 100}`,
        end: toISODateString(baseTime + (i + 1) * 60000),
        trades: `${50 + i}`,
      }));

      mockXhr.get.mockResolvedValue({ data: mockKlines });

      const candles = await exchange.fetchOHLCV({
        symbol: 'SOL_USDC',
        interval: '1m',
        limit: 1000,
      });

      expect(candles).toHaveLength(1000);
    });
  });

  describe('Timestamp handling', () => {
    it('should parse ISO date string start timestamp to milliseconds', async () => {
      const mockKlines: BackpackKline[] = [
        {
          start: '2024-01-01 00:00:00',
          open: '100',
          high: '101',
          low: '99',
          close: '100.5',
          volume: '1000',
          quoteVolume: '100000',
          end: '2024-01-01 01:00:00',
          trades: '10',
        },
      ];

      mockXhr.get.mockResolvedValue({ data: mockKlines });

      const candles = await exchange.fetchOHLCV({
        symbol: 'SOL_USDC',
        interval: '1h',
      });

      // ISO string "2024-01-01 00:00:00" + 'Z' parsed to UTC timestamp
      expect(candles[0].timestamp).toBe(new Date('2024-01-01T00:00:00Z').getTime());
    });

    it('should handle ISO date strings with different times', async () => {
      const mockKlines: BackpackKline[] = [
        {
          start: '2024-06-15 14:30:00',
          open: '100',
          high: '101',
          low: '99',
          close: '100.5',
          volume: '1000',
          quoteVolume: '100000',
          end: '2024-06-15 15:30:00',
          trades: '10',
        },
      ];

      mockXhr.get.mockResolvedValue({ data: mockKlines });

      const candles = await exchange.fetchOHLCV({
        symbol: 'SOL_USDC',
        interval: '1h',
      });

      expect(candles[0].timestamp).toBe(new Date('2024-06-15T14:30:00Z').getTime());
    });
  });
});
