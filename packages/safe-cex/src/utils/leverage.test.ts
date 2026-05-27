import { describe, it, expect } from 'vitest';
import {
  calculatePositionAccountLeverage,
  calculateTotalAccountLeverage,
  calculateTotalNotional,
} from './leverage';

describe('leverage', () => {
  describe('calculatePositionAccountLeverage', () => {
    it('should calculate leverage for a single position', () => {
      // $10,000 notional with $1,000 balance = 10x leverage
      expect(calculatePositionAccountLeverage(10000, 1000)).toBe(10);
    });

    it('should return 0 for zero balance', () => {
      expect(calculatePositionAccountLeverage(10000, 0)).toBe(0);
    });

    it('should return 0 for negative balance', () => {
      expect(calculatePositionAccountLeverage(10000, -1000)).toBe(0);
    });

    it('should handle negative notional (absolute value)', () => {
      // Short position with -$10,000 notional should still give 10x
      expect(calculatePositionAccountLeverage(-10000, 1000)).toBe(10);
    });

    it('should return fractional leverage for small positions', () => {
      // $500 notional with $1,000 balance = 0.5x leverage
      expect(calculatePositionAccountLeverage(500, 1000)).toBe(0.5);
    });

    it('should handle zero notional', () => {
      expect(calculatePositionAccountLeverage(0, 1000)).toBe(0);
    });

    it('should handle decimal values', () => {
      expect(calculatePositionAccountLeverage(1500.5, 1000)).toBe(1.5005);
    });
  });

  describe('calculateTotalAccountLeverage', () => {
    it('should calculate leverage across multiple positions', () => {
      const positions = [
        { notional: 5000 },  // Long $5000
        { notional: 3000 },  // Long $3000
      ];
      // Total: $8000 notional / $1000 balance = 8x
      expect(calculateTotalAccountLeverage(positions, 1000)).toBe(8);
    });

    it('should use absolute values for mixed long/short positions', () => {
      const positions = [
        { notional: 5000 },   // Long $5000
        { notional: -3000 },  // Short $3000
      ];
      // Total: |$5000| + |$-3000| = $8000 / $1000 = 8x
      expect(calculateTotalAccountLeverage(positions, 1000)).toBe(8);
    });

    it('should return 0 for zero balance', () => {
      const positions = [{ notional: 5000 }];
      expect(calculateTotalAccountLeverage(positions, 0)).toBe(0);
    });

    it('should return 0 for negative balance', () => {
      const positions = [{ notional: 5000 }];
      expect(calculateTotalAccountLeverage(positions, -1000)).toBe(0);
    });

    it('should return 0 for empty positions array', () => {
      expect(calculateTotalAccountLeverage([], 1000)).toBe(0);
    });

    it('should handle single position', () => {
      const positions = [{ notional: 2000 }];
      expect(calculateTotalAccountLeverage(positions, 1000)).toBe(2);
    });

    it('should handle many positions', () => {
      const positions = [
        { notional: 1000 },
        { notional: 2000 },
        { notional: -1500 },
        { notional: 500 },
        { notional: -1000 },
      ];
      // Total: |1000| + |2000| + |-1500| + |500| + |-1000| = 6000 / 1000 = 6x
      expect(calculateTotalAccountLeverage(positions, 1000)).toBe(6);
    });
  });

  describe('calculateTotalNotional', () => {
    it('should sum absolute notional values', () => {
      const positions = [
        { notional: 5000 },
        { notional: 3000 },
      ];
      expect(calculateTotalNotional(positions)).toBe(8000);
    });

    it('should use absolute values for negative notionals', () => {
      const positions = [
        { notional: 5000 },
        { notional: -3000 },
      ];
      expect(calculateTotalNotional(positions)).toBe(8000);
    });

    it('should return 0 for empty array', () => {
      expect(calculateTotalNotional([])).toBe(0);
    });

    it('should handle single position', () => {
      const positions = [{ notional: 1500 }];
      expect(calculateTotalNotional(positions)).toBe(1500);
    });

    it('should handle all negative positions', () => {
      const positions = [
        { notional: -2000 },
        { notional: -3000 },
      ];
      expect(calculateTotalNotional(positions)).toBe(5000);
    });

    it('should handle zero notional positions', () => {
      const positions = [
        { notional: 1000 },
        { notional: 0 },
        { notional: 2000 },
      ];
      expect(calculateTotalNotional(positions)).toBe(3000);
    });

    it('should handle decimal values', () => {
      const positions = [
        { notional: 1000.5 },
        { notional: -500.25 },
      ];
      expect(calculateTotalNotional(positions)).toBe(1500.75);
    });
  });
});
