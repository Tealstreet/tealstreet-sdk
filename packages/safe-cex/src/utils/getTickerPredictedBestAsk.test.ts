import { describe, expect, it } from 'vitest';

import { Market, Ticker } from '../types';
import { getTickerPredictedBestAsk } from './getTickerPredictedBestAsk';

const createTicker = (overrides?: Partial<Ticker>): Ticker =>
  ({
    symbol: 'BTCUSDT',
    last: 50000,
    bid: undefined,
    ask: undefined,
    high: 51000,
    low: 49000,
    open: 49500,
    close: 50000,
    volume: 1000,
    ...overrides,
  }) as Ticker;

const createMarket = (overrides?: Partial<Market>): Market =>
  ({
    id: 'BTCUSDT',
    symbol: 'BTCUSDT',
    base: 'BTC',
    quote: 'USDT',
    active: true,
    precision: { amount: 0.01, price: 0.01 },
    limits: { amount: { min: 0.001, max: 1000 }, leverage: { min: 1, max: 100 } },
    isInverse: false,
    ...overrides,
  }) as Market;

describe('getTickerPredictedBestAsk', () => {
  describe('when ask is available', () => {
    it('should return the ask price directly', () => {
      const ticker = createTicker({ ask: 50010 });
      const result = getTickerPredictedBestAsk(ticker);
      expect(result).toBe(50010);
    });

    it('should return ask even when market is provided', () => {
      const ticker = createTicker({ ask: 50010 });
      const market = createMarket();
      const result = getTickerPredictedBestAsk(ticker, market);
      expect(result).toBe(50010);
    });
  });

  describe('when ask is not available', () => {
    it('should return last + price precision when market is provided', () => {
      const ticker = createTicker({ last: 50000, ask: undefined });
      const market = createMarket({ precision: { amount: 0.01, price: 0.01 } });
      const result = getTickerPredictedBestAsk(ticker, market);
      expect(result).toBe(50000.01);
    });

    it('should use price precision instead of amount precision', () => {
      const ticker = createTicker({ last: 50000, ask: undefined });
      const market = createMarket({ precision: { amount: 1, price: 0.01 } });
      const result = getTickerPredictedBestAsk(ticker, market);
      expect(result).toBe(50000.01);
    });

    it('should return last when market is undefined', () => {
      const ticker = createTicker({ last: 50000, ask: undefined });
      const result = getTickerPredictedBestAsk(ticker, undefined);
      expect(result).toBe(50000);
    });

    it('should return last when market is null', () => {
      const ticker = createTicker({ last: 50000, ask: undefined });
      const result = getTickerPredictedBestAsk(ticker, null);
      expect(result).toBe(50000);
    });

    it('should return last when market has no precision', () => {
      const ticker = createTicker({ last: 50000, ask: undefined });
      const market = createMarket({ precision: undefined as any });
      const result = getTickerPredictedBestAsk(ticker, market);
      expect(result).toBe(50000);
    });

    it('should still use price precision when amount precision is missing', () => {
      const ticker = createTicker({ last: 50000, ask: undefined });
      const market = createMarket({ precision: { amount: undefined as any, price: 0.01 } });
      const result = getTickerPredictedBestAsk(ticker, market);
      expect(result).toBe(50000.01);
    });
  });

  describe('edge cases', () => {
    it('should handle zero ask', () => {
      // Zero is falsy but should still be returned if explicitly set
      const ticker = createTicker({ ask: 0 });
      const result = getTickerPredictedBestAsk(ticker);
      // Since 0 is falsy, it falls through to last + precision logic
      expect(result).toBe(50000);
    });

    it('should handle undefined last', () => {
      const ticker = createTicker({ last: undefined as any, ask: undefined });
      const market = createMarket();
      const result = getTickerPredictedBestAsk(ticker, market);
      expect(result).toBeUndefined();
    });
  });
});
