import { describe, it, expect, vi } from 'vitest';
import { ChaserOrderInstance } from './ChaserOrderInstance';
import { OrderSide } from '../../types';
import { Exchange } from '../../exchanges/base';

const exchange = {
  store: { tickers: [], tickersBySymbol: {}, markets: [], marketsBySymbol: {} },
  emitter: { on: vi.fn(), off: vi.fn(), emit: vi.fn() },
} as unknown as Exchange;

describe('ChaserOrderInstance.shouldTerminate', () => {
  describe('no termination configured', () => {
    it('returns false even at extreme prices', () => {
      const inst = new ChaserOrderInstance(exchange, {
        symbol: 'BTCUSDT',
        size: 1,
        side: OrderSide.Buy,
        maxAttempts: 1000,
        delay: 1000,
      });
      expect(inst.shouldTerminate(0)).toBe(false);
      expect(inst.shouldTerminate(50000)).toBe(false);
      expect(inst.shouldTerminate(Number.POSITIVE_INFINITY)).toBe(false);
    });
  });

  describe('absolute price', () => {
    it('buy: terminates when live >= threshold', () => {
      const inst = new ChaserOrderInstance(exchange, {
        symbol: 'BTCUSDT',
        size: 1,
        side: OrderSide.Buy,
        maxAttempts: 1000,
        delay: 1000,
        terminate: { type: 'price', value: 49500, anchorPrice: 50000 },
      });
      expect(inst.shouldTerminate(49000)).toBe(false);
      expect(inst.shouldTerminate(49499.99)).toBe(false);
      expect(inst.shouldTerminate(49500)).toBe(true);
      expect(inst.shouldTerminate(50000)).toBe(true);
    });

    it('sell: terminates when live <= threshold', () => {
      const inst = new ChaserOrderInstance(exchange, {
        symbol: 'BTCUSDT',
        size: 1,
        side: OrderSide.Sell,
        maxAttempts: 1000,
        delay: 1000,
        terminate: { type: 'price', value: 50500, anchorPrice: 50000 },
      });
      expect(inst.shouldTerminate(50500)).toBe(true);
      expect(inst.shouldTerminate(50000)).toBe(true);
      expect(inst.shouldTerminate(50500.01)).toBe(false);
    });
  });

  describe('percent', () => {
    it('buy 1%: terminates when live >= anchor * 1.01', () => {
      const inst = new ChaserOrderInstance(exchange, {
        symbol: 'BTCUSDT',
        size: 1,
        side: OrderSide.Buy,
        maxAttempts: 1000,
        delay: 1000,
        terminate: { type: 'percent', value: 1, anchorPrice: 50000 },
      });
      // threshold = 50000 * 1.01 = 50500
      expect(inst.shouldTerminate(50499.99)).toBe(false);
      expect(inst.shouldTerminate(50500)).toBe(true);
      expect(inst.shouldTerminate(60000)).toBe(true);
    });

    it('sell 1%: terminates when live <= anchor * 0.99', () => {
      const inst = new ChaserOrderInstance(exchange, {
        symbol: 'BTCUSDT',
        size: 1,
        side: OrderSide.Sell,
        maxAttempts: 1000,
        delay: 1000,
        terminate: { type: 'percent', value: 1, anchorPrice: 50000 },
      });
      // threshold = 50000 * 0.99 = 49500
      expect(inst.shouldTerminate(49500.01)).toBe(false);
      expect(inst.shouldTerminate(49500)).toBe(true);
      expect(inst.shouldTerminate(40000)).toBe(true);
    });

    it('handles fractional percent', () => {
      const inst = new ChaserOrderInstance(exchange, {
        symbol: 'BTCUSDT',
        size: 1,
        side: OrderSide.Buy,
        maxAttempts: 1000,
        delay: 1000,
        terminate: { type: 'percent', value: 0.1, anchorPrice: 50000 },
      });
      // threshold = 50000 * 1.001 = 50050
      expect(inst.shouldTerminate(50049)).toBe(false);
      expect(inst.shouldTerminate(50050)).toBe(true);
    });
  });

  describe('usd', () => {
    it('buy $100: terminates when live >= anchor + 100', () => {
      const inst = new ChaserOrderInstance(exchange, {
        symbol: 'BTCUSDT',
        size: 1,
        side: OrderSide.Buy,
        maxAttempts: 1000,
        delay: 1000,
        terminate: { type: 'usd', value: 100, anchorPrice: 50000 },
      });
      expect(inst.shouldTerminate(50099)).toBe(false);
      expect(inst.shouldTerminate(50100)).toBe(true);
    });

    it('sell $50: terminates when live <= anchor - 50', () => {
      const inst = new ChaserOrderInstance(exchange, {
        symbol: 'BTCUSDT',
        size: 1,
        side: OrderSide.Sell,
        maxAttempts: 1000,
        delay: 1000,
        terminate: { type: 'usd', value: 50, anchorPrice: 50000 },
      });
      expect(inst.shouldTerminate(49951)).toBe(false);
      expect(inst.shouldTerminate(49950)).toBe(true);
    });
  });

  describe('guards', () => {
    it('returns false on non-finite or non-positive live price', () => {
      const inst = new ChaserOrderInstance(exchange, {
        symbol: 'BTCUSDT',
        size: 1,
        side: OrderSide.Buy,
        maxAttempts: 1000,
        delay: 1000,
        terminate: { type: 'price', value: 1, anchorPrice: 50000 },
      });
      expect(inst.shouldTerminate(0)).toBe(false);
      expect(inst.shouldTerminate(-100)).toBe(false);
      expect(inst.shouldTerminate(NaN)).toBe(false);
      // Non-finite live prices should not fire termination either —
      // a feed glitch shouldn't tear down the chaser.
      expect(inst.shouldTerminate(Number.POSITIVE_INFINITY)).toBe(false);
    });
  });
});
