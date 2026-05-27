/**
 * Unit tests for dYdX fetchOHLCV implementation
 * Tests candle data fetching with various parameters
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { DydxExchange } from '../dydx.exchange';
import { DefaultStore } from '../../../store/store.base';
import { CandleResolution, DydxCandle } from '../dydx.types';
import type { Market } from '../../../types';

describe('DydxExchange - fetchOHLCV', () => {
  let exchange: DydxExchange;
  let store: DefaultStore;
  let mockXhr: any;

  // Mock market data
  const ethMarket: Market = {
    id: 'ETH-USD',
    symbol: 'ETH-USD',
    base: 'ETH',
    quote: 'USD',
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

    exchange = new DydxExchange(
      'dydx',
      {
        key: 'test_wallet_address',
        secret: 'test_mnemonic_phrase',
        testnet: true,
      },
      store
    );

    // Mock the xhr instance
    mockXhr = {
      get: vi.fn(),
    };
    exchange.xhr = mockXhr as any;
  });

  describe('Interval conversion', () => {
    it('should convert 1m interval to ONE_MIN', async () => {
      const mockCandles: DydxCandle[] = [
        {
          startedAt: '2024-01-01T00:00:00.000Z',
          ticker: 'ETH-USD',
          resolution: CandleResolution.ONE_MIN,
          low: '3000',
          high: '3010',
          open: '3005',
          close: '3008',
          baseTokenVolume: '100',
          usdVolume: '300500',
          trades: 50,
          startingOpenInterest: '1000',
        },
      ];

      mockXhr.get.mockResolvedValue({ data: { candles: mockCandles } });

      const candles = await exchange.fetchOHLCV({
        symbol: 'ETH-USD',
        interval: '1m',
        limit: 10,
      });

      expect(mockXhr.get).toHaveBeenCalledWith(
        expect.stringContaining('/v4/candles/perpetualMarkets/ETH-USD'),
        expect.objectContaining({
          params: expect.objectContaining({
            resolution: CandleResolution.ONE_MIN,
            limit: 10,
          }),
        })
      );
      expect(candles).toHaveLength(1);
    });

    it('should convert 5m interval to FIVE_MINS', async () => {
      mockXhr.get.mockResolvedValue({ data: { candles: [] } });

      await exchange.fetchOHLCV({
        symbol: 'ETH-USD',
        interval: '5m',
      });

      expect(mockXhr.get).toHaveBeenCalledWith(
        expect.any(String),
        expect.objectContaining({
          params: expect.objectContaining({
            resolution: CandleResolution.FIVE_MINS,
          }),
        })
      );
    });

    it('should convert 15m interval to FIFTEEN_MINS', async () => {
      mockXhr.get.mockResolvedValue({ data: { candles: [] } });

      await exchange.fetchOHLCV({
        symbol: 'ETH-USD',
        interval: '15m',
      });

      expect(mockXhr.get).toHaveBeenCalledWith(
        expect.any(String),
        expect.objectContaining({
          params: expect.objectContaining({
            resolution: CandleResolution.FIFTEEN_MINS,
          }),
        })
      );
    });

    it('should convert 30m interval to THIRTY_MINS', async () => {
      mockXhr.get.mockResolvedValue({ data: { candles: [] } });

      await exchange.fetchOHLCV({
        symbol: 'ETH-USD',
        interval: '30m',
      });

      expect(mockXhr.get).toHaveBeenCalledWith(
        expect.any(String),
        expect.objectContaining({
          params: expect.objectContaining({
            resolution: CandleResolution.THIRTY_MINS,
          }),
        })
      );
    });

    it('should convert 1h interval to ONE_HOUR', async () => {
      mockXhr.get.mockResolvedValue({ data: { candles: [] } });

      await exchange.fetchOHLCV({
        symbol: 'ETH-USD',
        interval: '1h',
      });

      expect(mockXhr.get).toHaveBeenCalledWith(
        expect.any(String),
        expect.objectContaining({
          params: expect.objectContaining({
            resolution: CandleResolution.ONE_HOUR,
          }),
        })
      );
    });

    it('should convert 4h interval to FOUR_HOURS', async () => {
      mockXhr.get.mockResolvedValue({ data: { candles: [] } });

      await exchange.fetchOHLCV({
        symbol: 'ETH-USD',
        interval: '4h',
      });

      expect(mockXhr.get).toHaveBeenCalledWith(
        expect.any(String),
        expect.objectContaining({
          params: expect.objectContaining({
            resolution: CandleResolution.FOUR_HOURS,
          }),
        })
      );
    });

    it('should convert 1d interval to ONE_DAY', async () => {
      mockXhr.get.mockResolvedValue({ data: { candles: [] } });

      await exchange.fetchOHLCV({
        symbol: 'ETH-USD',
        interval: '1d',
      });

      expect(mockXhr.get).toHaveBeenCalledWith(
        expect.any(String),
        expect.objectContaining({
          params: expect.objectContaining({
            resolution: CandleResolution.ONE_DAY,
          }),
        })
      );
    });
  });

  describe('Time range parameters', () => {
    it('should pass from parameter as ISO string', async () => {
      mockXhr.get.mockResolvedValue({ data: { candles: [] } });

      const fromTime = 1704067200000; // 2024-01-01 00:00:00 UTC
      await exchange.fetchOHLCV({
        symbol: 'ETH-USD',
        interval: '1h',
        from: fromTime,
      });

      expect(mockXhr.get).toHaveBeenCalledWith(
        expect.any(String),
        expect.objectContaining({
          params: expect.objectContaining({
            fromISO: '2024-01-01T00:00:00.000Z',
          }),
        })
      );
    });

    it('should pass to parameter as ISO string', async () => {
      mockXhr.get.mockResolvedValue({ data: { candles: [] } });

      const toTime = 1704153600000; // 2024-01-02 00:00:00 UTC
      await exchange.fetchOHLCV({
        symbol: 'ETH-USD',
        interval: '1h',
        to: toTime,
      });

      expect(mockXhr.get).toHaveBeenCalledWith(
        expect.any(String),
        expect.objectContaining({
          params: expect.objectContaining({
            toISO: '2024-01-02T00:00:00.000Z',
          }),
        })
      );
    });

    it('should pass both from and to parameters', async () => {
      mockXhr.get.mockResolvedValue({ data: { candles: [] } });

      const fromTime = 1704067200000;
      const toTime = 1704153600000;

      await exchange.fetchOHLCV({
        symbol: 'ETH-USD',
        interval: '1h',
        from: fromTime,
        to: toTime,
      });

      expect(mockXhr.get).toHaveBeenCalledWith(
        expect.any(String),
        expect.objectContaining({
          params: expect.objectContaining({
            fromISO: '2024-01-01T00:00:00.000Z',
            toISO: '2024-01-02T00:00:00.000Z',
          }),
        })
      );
    });

    it('should pass limit parameter', async () => {
      mockXhr.get.mockResolvedValue({ data: { candles: [] } });

      await exchange.fetchOHLCV({
        symbol: 'ETH-USD',
        interval: '1h',
        limit: 100,
      });

      expect(mockXhr.get).toHaveBeenCalledWith(
        expect.any(String),
        expect.objectContaining({
          params: expect.objectContaining({
            limit: 100,
          }),
        })
      );
    });
  });

  describe('Data transformation', () => {
    it('should correctly transform dYdX candles to safe-cex format', async () => {
      const mockCandles: DydxCandle[] = [
        {
          startedAt: '2024-01-01T00:00:00.000Z',
          ticker: 'ETH-USD',
          resolution: CandleResolution.ONE_HOUR,
          low: '3000.50',
          high: '3100.75',
          open: '3050.25',
          close: '3080.00',
          baseTokenVolume: '150.5',
          usdVolume: '460000',
          trades: 200,
          startingOpenInterest: '5000',
        },
        {
          startedAt: '2024-01-01T01:00:00.000Z',
          ticker: 'ETH-USD',
          resolution: CandleResolution.ONE_HOUR,
          low: '3070.00',
          high: '3120.50',
          open: '3080.00',
          close: '3110.25',
          baseTokenVolume: '180.25',
          usdVolume: '560000',
          trades: 250,
          startingOpenInterest: '5100',
        },
      ];

      mockXhr.get.mockResolvedValue({ data: { candles: mockCandles } });

      const candles = await exchange.fetchOHLCV({
        symbol: 'ETH-USD',
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

    it('should handle string numbers correctly', async () => {
      const mockCandles: DydxCandle[] = [
        {
          startedAt: '2024-01-01T00:00:00.000Z',
          ticker: 'ETH-USD',
          resolution: CandleResolution.ONE_MIN,
          low: '2999.99',
          high: '3000.01',
          open: '3000.00',
          close: '3000.005',
          baseTokenVolume: '0.123456',
          usdVolume: '370',
          trades: 5,
          startingOpenInterest: '1000',
        },
      ];

      mockXhr.get.mockResolvedValue({ data: { candles: mockCandles } });

      const candles = await exchange.fetchOHLCV({
        symbol: 'ETH-USD',
        interval: '1m',
      });

      expect(candles[0].open).toBeCloseTo(3000.00, 2);
      expect(candles[0].high).toBeCloseTo(3000.01, 2);
      expect(candles[0].low).toBeCloseTo(2999.99, 2);
      expect(candles[0].close).toBeCloseTo(3000.005, 3);
      expect(candles[0].volume).toBeCloseTo(0.123456, 6);
    });
  });

  describe('Error handling', () => {
    it('should return empty array for unknown symbol', async () => {
      const candles = await exchange.fetchOHLCV({
        symbol: 'UNKNOWN-USD',
        interval: '1h',
      });

      expect(candles).toEqual([]);
      expect(mockXhr.get).not.toHaveBeenCalled();
    });

    it('should return empty array when API returns null data', async () => {
      mockXhr.get.mockResolvedValue({ data: null });

      const candles = await exchange.fetchOHLCV({
        symbol: 'ETH-USD',
        interval: '1h',
      });

      expect(candles).toEqual([]);
    });

    it('should return empty array when API returns no candles', async () => {
      mockXhr.get.mockResolvedValue({ data: { candles: null } });

      const candles = await exchange.fetchOHLCV({
        symbol: 'ETH-USD',
        interval: '1h',
      });

      expect(candles).toEqual([]);
    });

    it('should return empty array when API returns empty candles', async () => {
      mockXhr.get.mockResolvedValue({ data: { candles: [] } });

      const candles = await exchange.fetchOHLCV({
        symbol: 'ETH-USD',
        interval: '1h',
      });

      expect(candles).toEqual([]);
    });

    it('should throw error when API request fails', async () => {
      mockXhr.get.mockRejectedValue(new Error('Network error'));

      await expect(
        exchange.fetchOHLCV({
          symbol: 'ETH-USD',
          interval: '1h',
        })
      ).rejects.toThrow('Network error');
    });
  });

  describe('API endpoint construction', () => {
    it('should construct correct endpoint URL', async () => {
      mockXhr.get.mockResolvedValue({ data: { candles: [] } });

      await exchange.fetchOHLCV({
        symbol: 'ETH-USD',
        interval: '1h',
      });

      expect(mockXhr.get).toHaveBeenCalledWith(
        expect.stringContaining('/v4/candles/perpetualMarkets/ETH-USD'),
        expect.any(Object)
      );
    });

    it('should use market ID instead of symbol', async () => {
      // Test that it uses market.id, not opts.symbol directly
      const btcMarket: Market = {
        id: 'BTC-USD',
        symbol: 'BTCUSD', // Different from ID
        base: 'BTC',
        quote: 'USD',
        active: true,
        precision: { amount: 4, price: 2 },
        limits: {
          amount: { min: 0.001, max: 1000 },
          leverage: { min: 1, max: 20 },
        },
      };

      store.update({ markets: [btcMarket] });
      mockXhr.get.mockResolvedValue({ data: { candles: [] } });

      await exchange.fetchOHLCV({
        symbol: 'BTCUSD',
        interval: '1h',
      });

      // Should use market.id (BTC-USD), not symbol (BTCUSD)
      expect(mockXhr.get).toHaveBeenCalledWith(
        expect.stringContaining('/v4/candles/perpetualMarkets/BTC-USD'),
        expect.any(Object)
      );
    });
  });

  describe('Multiple candles', () => {
    it('should handle 100 candles correctly', async () => {
      const mockCandles: DydxCandle[] = Array.from({ length: 100 }, (_, i) => ({
        startedAt: new Date(1704067200000 + i * 60000).toISOString(),
        ticker: 'ETH-USD',
        resolution: CandleResolution.ONE_MIN,
        low: `${3000 + i}`,
        high: `${3010 + i}`,
        open: `${3005 + i}`,
        close: `${3008 + i}`,
        baseTokenVolume: `${100 + i}`,
        usdVolume: `${300000 + i * 1000}`,
        trades: 50 + i,
        startingOpenInterest: '1000',
      }));

      mockXhr.get.mockResolvedValue({ data: { candles: mockCandles } });

      const candles = await exchange.fetchOHLCV({
        symbol: 'ETH-USD',
        interval: '1m',
        limit: 100,
      });

      expect(candles).toHaveLength(100);
      expect(candles[0].open).toBe(3005);
      expect(candles[99].open).toBe(3104);
    });
  });
});
