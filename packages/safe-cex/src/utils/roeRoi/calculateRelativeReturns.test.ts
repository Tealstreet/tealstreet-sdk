import { describe, it, expect } from 'vitest';
import { calculateRelativeReturns } from './calculateRelativeReturns';
import { Position, PositionSide } from '../../types';

// Helper to create mock position
const createPosition = (overrides: Partial<Position> = {}): Position => ({
  id: 'pos1',
  symbol: 'BTCUSDT',
  side: PositionSide.Long,
  entryPrice: 50000,
  notional: 50000,
  leverage: 10,
  unrealizedPnl: 0,
  contracts: 1,
  liquidationPrice: 45000,
  ...overrides,
});

describe('calculateRelativeReturns', () => {
  describe('Long positions', () => {
    it('should calculate positive returns for price increase', () => {
      const position = createPosition({
        side: PositionSide.Long,
        entryPrice: 50000,
        leverage: 10,
      });

      // Exit at $51000 = $1000 profit
      const result = calculateRelativeReturns(position, 51000);

      // upnl = 51000 - 50000 = 1000
      // roi = (1000 / 50000) * 100 = 2%
      // roe = (1000 / 50000) * 10 * 100 = 20%
      expect(result.roi).toBe(2);
      expect(result.roe).toBe(20);
    });

    it('should calculate negative returns for price decrease', () => {
      const position = createPosition({
        side: PositionSide.Long,
        entryPrice: 50000,
        leverage: 10,
      });

      // Exit at $49000 = $1000 loss
      const result = calculateRelativeReturns(position, 49000);

      // upnl = 49000 - 50000 = -1000
      // roi = (-1000 / 50000) * 100 = -2%
      // roe = (-1000 / 50000) * 10 * 100 = -20%
      expect(result.roi).toBe(-2);
      expect(result.roe).toBe(-20);
    });

    it('should return zero when exit price equals entry price', () => {
      const position = createPosition({
        side: PositionSide.Long,
        entryPrice: 50000,
        leverage: 10,
      });

      const result = calculateRelativeReturns(position, 50000);

      expect(result.roi).toBe(0);
      expect(result.roe).toBe(0);
    });
  });

  describe('Short positions', () => {
    it('should calculate positive returns for price decrease', () => {
      const position = createPosition({
        side: PositionSide.Short,
        entryPrice: 50000,
        leverage: 10,
      });

      // Exit at $49000 = $1000 profit for short
      const result = calculateRelativeReturns(position, 49000);

      // upnl = 50000 - 49000 = 1000
      // roi = (1000 / 50000) * 100 = 2%
      // roe = (1000 / 50000) * 10 * 100 = 20%
      expect(result.roi).toBe(2);
      expect(result.roe).toBe(20);
    });

    it('should calculate negative returns for price increase', () => {
      const position = createPosition({
        side: PositionSide.Short,
        entryPrice: 50000,
        leverage: 10,
      });

      // Exit at $51000 = $1000 loss for short
      const result = calculateRelativeReturns(position, 51000);

      // upnl = 50000 - 51000 = -1000
      // roi = (-1000 / 50000) * 100 = -2%
      // roe = (-1000 / 50000) * 10 * 100 = -20%
      expect(result.roi).toBe(-2);
      expect(result.roe).toBe(-20);
    });
  });

  describe('leverage impact', () => {
    it('should scale ROE with leverage', () => {
      const lowLev = createPosition({
        side: PositionSide.Long,
        entryPrice: 50000,
        leverage: 5,
      });

      const highLev = createPosition({
        side: PositionSide.Long,
        entryPrice: 50000,
        leverage: 20,
      });

      const lowResult = calculateRelativeReturns(lowLev, 51000);
      const highResult = calculateRelativeReturns(highLev, 51000);

      // ROE should scale: highLev.roe = lowLev.roe * 4
      expect(highResult.roe).toBe(lowResult.roe * 4);
    });

    it('should not scale ROI with leverage', () => {
      const lowLev = createPosition({
        side: PositionSide.Long,
        entryPrice: 50000,
        leverage: 5,
      });

      const highLev = createPosition({
        side: PositionSide.Long,
        entryPrice: 50000,
        leverage: 50,
      });

      const lowResult = calculateRelativeReturns(lowLev, 51000);
      const highResult = calculateRelativeReturns(highLev, 51000);

      expect(highResult.roi).toBe(lowResult.roi);
    });
  });

  describe('edge cases', () => {
    it('should return 0 when entry price is 0', () => {
      const position = createPosition({
        entryPrice: 0,
      });

      const result = calculateRelativeReturns(position, 50000);

      expect(result.roe).toBe(0);
      expect(result.roi).toBe(0);
    });

    it('should handle very small price movements', () => {
      const position = createPosition({
        side: PositionSide.Long,
        entryPrice: 50000,
        leverage: 10,
      });

      // Exit at $50001 = $1 profit
      const result = calculateRelativeReturns(position, 50001);

      // upnl = 1
      // roi = (1 / 50000) * 100 = 0.002%
      expect(result.roi).toBeCloseTo(0.002, 4);
    });

    it('should handle 1x leverage', () => {
      const position = createPosition({
        side: PositionSide.Long,
        entryPrice: 50000,
        leverage: 1,
      });

      const result = calculateRelativeReturns(position, 51000);

      // At 1x leverage, ROE = ROI
      expect(result.roe).toBe(result.roi);
    });

    it('should ignore unrealizedPnl from position', () => {
      const position = createPosition({
        side: PositionSide.Long,
        entryPrice: 50000,
        leverage: 10,
        unrealizedPnl: 99999, // Should be ignored
      });

      // Exit at $51000 = $1000 profit based on price
      const result = calculateRelativeReturns(position, 51000);

      // Should calculate based on exit price, not position.unrealizedPnl
      expect(result.roi).toBe(2);
    });
  });
});
