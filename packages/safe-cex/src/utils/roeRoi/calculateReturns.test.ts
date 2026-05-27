import { describe, it, expect } from 'vitest';
import { calculateReturns } from './calculateReturns';

describe('calculateReturns', () => {
  describe('ROE calculation (with leverage)', () => {
    it('should calculate positive ROE for profit', () => {
      const position = {
        unrealizedPnl: 100,
        contracts: 1,
        entryPrice: 50000,
        leverage: 10,
      };

      const result = calculateReturns(position);

      // margin = 50000 * 1 = 50000
      // roe = (100 / 50000) * 10 * 100 = 2%
      expect(result.roe).toBe(2);
    });

    it('should calculate negative ROE for loss', () => {
      const position = {
        unrealizedPnl: -500,
        contracts: 1,
        entryPrice: 50000,
        leverage: 10,
      };

      const result = calculateReturns(position);

      // margin = 50000
      // roe = (-500 / 50000) * 10 * 100 = -10%
      expect(result.roe).toBe(-10);
    });

    it('should scale ROE with leverage', () => {
      const position = {
        unrealizedPnl: 100,
        contracts: 1,
        entryPrice: 50000,
        leverage: 20, // 20x leverage
      };

      const result = calculateReturns(position);

      // roe = (100 / 50000) * 20 * 100 = 4%
      expect(result.roe).toBe(4);
    });
  });

  describe('ROI calculation (without leverage)', () => {
    it('should calculate positive ROI for profit', () => {
      const position = {
        unrealizedPnl: 100,
        contracts: 1,
        entryPrice: 50000,
        leverage: 10,
      };

      const result = calculateReturns(position);

      // roi = (100 / 50000) * 100 = 0.2%
      expect(result.roi).toBe(0.2);
    });

    it('should calculate negative ROI for loss', () => {
      const position = {
        unrealizedPnl: -500,
        contracts: 1,
        entryPrice: 50000,
        leverage: 10,
      };

      const result = calculateReturns(position);

      // roi = (-500 / 50000) * 100 = -1%
      expect(result.roi).toBe(-1);
    });

    it('should not scale ROI with leverage', () => {
      const basePnl = {
        unrealizedPnl: 100,
        contracts: 1,
        entryPrice: 50000,
        leverage: 5,
      };

      const highLeverage = { ...basePnl, leverage: 50 };

      // ROI should be the same regardless of leverage
      expect(calculateReturns(basePnl).roi).toBe(calculateReturns(highLeverage).roi);
    });
  });

  describe('overrideUPNL parameter', () => {
    it('should use overrideUPNL instead of position upnl', () => {
      const position = {
        unrealizedPnl: 100,
        contracts: 1,
        entryPrice: 50000,
        leverage: 10,
      };

      const result = calculateReturns(position, 500);

      // Should use 500 instead of 100
      // roe = (500 / 50000) * 10 * 100 = 10%
      expect(result.roe).toBe(10);
    });

    it('should handle zero overrideUPNL', () => {
      const position = {
        unrealizedPnl: 100,
        contracts: 1,
        entryPrice: 50000,
        leverage: 10,
      };

      // 0 is falsy, so it should use position.unrealizedPnl
      const result = calculateReturns(position, 0);

      // Uses position.unrealizedPnl = 100
      expect(result.roe).toBe(2);
    });
  });

  describe('edge cases', () => {
    it('should return 0 when margin is 0 (zero entry price)', () => {
      const position = {
        unrealizedPnl: 100,
        contracts: 1,
        entryPrice: 0,
        leverage: 10,
      };

      const result = calculateReturns(position);

      expect(result.roe).toBe(0);
      expect(result.roi).toBe(0);
    });

    it('should return 0 when margin is 0 (zero contracts)', () => {
      const position = {
        unrealizedPnl: 100,
        contracts: 0,
        entryPrice: 50000,
        leverage: 10,
      };

      const result = calculateReturns(position);

      expect(result.roe).toBe(0);
      expect(result.roi).toBe(0);
    });

    it('should handle fractional contracts', () => {
      const position = {
        unrealizedPnl: 10,
        contracts: 0.1,
        entryPrice: 50000,
        leverage: 10,
      };

      const result = calculateReturns(position);

      // margin = 50000 * 0.1 = 5000
      // roe = (10 / 5000) * 10 * 100 = 2%
      expect(result.roe).toBe(2);
    });

    it('should handle 1x leverage', () => {
      const position = {
        unrealizedPnl: 100,
        contracts: 1,
        entryPrice: 50000,
        leverage: 1,
      };

      const result = calculateReturns(position);

      // At 1x leverage, ROE = ROI
      expect(result.roe).toBe(result.roi);
    });
  });
});
