import { describe, expect, it } from 'vitest';

import { add, adjust, afterDecimal, divide, handleRoundingError, multiply, round, subtract } from './safe-math';

describe('safe-math', () => {
  describe('afterDecimal', () => {
    it('should return 0 for integers', () => {
      expect(afterDecimal(5)).toBe(0);
      expect(afterDecimal(0)).toBe(0);
      expect(afterDecimal(-10)).toBe(0);
      expect(afterDecimal(1000000)).toBe(0);
    });

    it('should count decimal places for floats', () => {
      expect(afterDecimal(1.5)).toBe(1);
      expect(afterDecimal(1.25)).toBe(2);
      expect(afterDecimal(1.125)).toBe(3);
      expect(afterDecimal(0.0001)).toBe(4);
      expect(afterDecimal(0.00001)).toBe(5);
    });

    it('should handle string inputs', () => {
      expect(afterDecimal('1.5')).toBe(1);
      expect(afterDecimal('100')).toBe(0);
      expect(afterDecimal('0.001')).toBe(3);
    });

    it('should handle scientific notation', () => {
      expect(afterDecimal(1e-5)).toBe(5);
      expect(afterDecimal(1e-10)).toBe(10);
      expect(afterDecimal('1e-3')).toBe(3);
      expect(afterDecimal(2.5e-7)).toBe(8);
      expect(afterDecimal(1.25e-7)).toBe(9);
      expect(afterDecimal('2.5E-7')).toBe(8);
      expect(afterDecimal('2.5e+3')).toBe(0);
    });

    it('should return 2 for null/undefined-like values', () => {
      expect(afterDecimal(null as unknown as number)).toBe(2);
      expect(afterDecimal(undefined as unknown as number)).toBe(2);
    });

    it('should handle negative decimals', () => {
      expect(afterDecimal(-0.5)).toBe(1);
      expect(afterDecimal(-0.001)).toBe(3);
    });
  });

  describe('adjust', () => {
    it('should adjust value to step size', () => {
      expect(adjust(1.234, 0.01)).toBe(1.23);
      expect(adjust(1.235, 0.01)).toBe(1.24);
      expect(adjust(1.236, 0.01)).toBe(1.24);
    });

    it('should handle step size of 1', () => {
      expect(adjust(1.4, 1)).toBe(1);
      expect(adjust(1.5, 1)).toBe(2);
      expect(adjust(1.6, 1)).toBe(2);
    });

    it('should handle small step sizes', () => {
      expect(adjust(0.123456, 0.0001)).toBe(0.1235);
      expect(adjust(0.123456, 0.00001)).toBe(0.12346);
    });

    it('should handle string step sizes', () => {
      expect(adjust(1.234, '0.01')).toBe(1.23);
      expect(adjust(1.235, '0.1')).toBe(1.2);
    });

    it('should handle larger step sizes', () => {
      expect(adjust(7, 5)).toBe(5);
      expect(adjust(8, 5)).toBe(10);
      expect(adjust(12, 5)).toBe(10);
      expect(adjust(13, 5)).toBe(15);
    });

    it('should handle zero', () => {
      expect(adjust(0, 0.01)).toBe(0);
      expect(adjust(0, 1)).toBe(0);
    });

    it('should handle negative values', () => {
      expect(adjust(-1.234, 0.01)).toBe(-1.23);
      expect(adjust(-1.235, 0.01)).toBe(-1.24);
    });
  });

  describe('add', () => {
    it('should add integers correctly', () => {
      expect(add(1, 2)).toBe(3);
      expect(add(0, 0)).toBe(0);
      expect(add(-1, 1)).toBe(0);
    });

    it('should handle floating point precision', () => {
      // Classic floating point issue: 0.1 + 0.2 !== 0.3
      expect(add(0.1, 0.2)).toBe(0.3);
      expect(add(0.01, 0.02)).toBe(0.03);
    });

    it('should handle different decimal places', () => {
      expect(add(1.1, 2.22)).toBe(3.32);
      expect(add(0.001, 0.1)).toBe(0.101);
    });

    it('should handle negative numbers', () => {
      expect(add(-0.1, -0.2)).toBe(-0.3);
      expect(add(-0.5, 0.3)).toBe(-0.2);
    });

    it('should handle large numbers', () => {
      expect(add(1000000.1, 2000000.2)).toBe(3000000.3);
    });
  });

  describe('subtract', () => {
    it('should subtract integers correctly', () => {
      expect(subtract(5, 3)).toBe(2);
      expect(subtract(0, 0)).toBe(0);
      expect(subtract(-1, -1)).toBe(0);
    });

    it('should handle floating point precision', () => {
      expect(subtract(0.3, 0.1)).toBe(0.2);
      expect(subtract(0.03, 0.01)).toBe(0.02);
    });

    it('should handle different decimal places', () => {
      expect(subtract(3.33, 1.1)).toBe(2.23);
      expect(subtract(0.101, 0.001)).toBe(0.1);
    });

    it('should handle negative results', () => {
      expect(subtract(0.1, 0.3)).toBe(-0.2);
      expect(subtract(-0.5, 0.3)).toBe(-0.8);
    });
  });

  describe('multiply', () => {
    it('should preserve more precision for small magnitudes', () => {
      // multiply is designed for cases like 0.0002 * 0.0003 = 0.00000006
      expect(multiply(0.0002, 0.0003)).toBe(0.00000006);
      expect(multiply(0.001, 0.001)).toBe(0.000001);
    });

    it('should work for regular values too', () => {
      expect(multiply(2, 3)).toBe(6);
      expect(multiply(0.5, 0.5)).toBe(0.25);
    });

    it('should handle very small numbers', () => {
      expect(multiply(0.00001, 0.00001)).toBe(0.0000000001);
    });
  });

  describe('divide', () => {
    it('should handle integer division with minimum decimals', () => {
      // divide uses minDecimals=8 by default, so integer division works
      expect(divide(1, 3)).toBeCloseTo(0.33333333, 8);
      expect(divide(1, 4)).toBe(0.25);
      expect(divide(10, 3)).toBeCloseTo(3.33333333, 8);
    });

    it('should preserve precision for integer inputs', () => {
      expect(divide(1000, 3)).toBeCloseTo(333.33333333, 8);
      expect(divide(7, 3)).toBeCloseTo(2.33333333, 8);
    });

    it('should work with decimal inputs', () => {
      expect(divide(0.5, 0.2)).toBe(2.5);
      expect(divide(1.5, 3)).toBe(0.5);
    });

    it('should handle zero dividend', () => {
      expect(divide(0, 5)).toBe(0);
      expect(divide(0, 0.001)).toBe(0);
    });

    it('should handle negative numbers', () => {
      expect(divide(-1, 3)).toBeCloseTo(-0.33333333, 8);
      expect(divide(-1, -3)).toBeCloseTo(0.33333333, 8);
      expect(divide(1, -3)).toBeCloseTo(-0.33333333, 8);
    });

    it('should allow custom minDecimals', () => {
      // With minDecimals=2, result rounds to 2 decimals
      expect(divide(1, 3, 2)).toBe(0.33);
      expect(divide(1, 3, 4)).toBe(0.3333);
    });

    it('should use higher precision when inputs have more decimals', () => {
      // Input has 4 decimals, so result uses 4 decimals (> minDecimals of 2)
      // 1.0001 / 2 = 0.50005, rounded to 4 decimals = 0.5001
      expect(divide(1.0001, 2, 2)).toBe(0.5001);
    });
  });

  describe('handleRoundingError', () => {
    it('should fix common floating point errors', () => {
      const badValue = 0.1 + 0.2; // 0.30000000000000004
      expect(handleRoundingError(badValue)).toBe(0.3);
    });

    it('should return 0 for NaN', () => {
      expect(handleRoundingError(NaN)).toBe(0);
    });

    it('should handle Infinity', () => {
      // handleRoundingError returns Infinity as-is since isNotNumber only checks NaN
      expect(handleRoundingError(Infinity)).toBe(Infinity);
      expect(handleRoundingError(-Infinity)).toBe(-Infinity);
    });

    it('should handle normal values', () => {
      expect(handleRoundingError(1.5)).toBe(1.5);
      expect(handleRoundingError(0)).toBe(0);
      expect(handleRoundingError(-1.5)).toBe(-1.5);
    });

    it('should limit precision to 10 decimal places', () => {
      const longDecimal = 1.123456789012345;
      const result = handleRoundingError(longDecimal);
      expect(result.toString().split('.')[1]?.length || 0).toBeLessThanOrEqual(10);
    });
  });

  describe('round', () => {
    it('should return 0 for NaN', () => {
      expect(round(NaN)).toBe(0);
    });

    it('should handle Infinity', () => {
      // round returns Infinity as-is since isNotNumber only checks NaN
      expect(round(Infinity)).toBe(Infinity);
      expect(round(-Infinity)).toBe(-Infinity);
    });

    it('should handle zero', () => {
      expect(round(0)).toBe(0);
    });

    it('should use 2 significant digits for values >= 1000', () => {
      expect(round(1234.5678)).toBe(1234.57);
      expect(round(9999.999)).toBe(10000);
    });

    it('should use 3 significant digits for values >= 10', () => {
      expect(round(12.3456)).toBe(12.346);
      expect(round(99.9999)).toBe(100);
    });

    it('should use 4 significant digits for values >= 1', () => {
      expect(round(1.23456)).toBe(1.2346);
      expect(round(9.99999)).toBe(10);
    });

    it('should use 5 significant digits for values >= 0.0001', () => {
      expect(round(0.123456)).toBe(0.12346);
      expect(round(0.000123)).toBe(0.00012);
    });

    it('should use 6 significant digits for values >= 0.00001', () => {
      expect(round(0.0000123456)).toBe(0.000012);
    });

    it('should use 7 significant digits for values >= 0.000001', () => {
      expect(round(0.00000123456)).toBe(0.0000012);
    });

    it('should use 8 significant digits for very small values', () => {
      expect(round(0.000000123456)).toBe(0.00000012);
    });

    it('should handle negative values', () => {
      expect(round(-1234.5678)).toBe(-1234.57);
      expect(round(-0.123456)).toBe(-0.12346);
    });
  });
});
