import { describe, expect, it } from 'vitest';
import { formatLargeAmounts } from './formatLargeAmounts';

describe('formatLargeAmounts', () => {
  describe('small numbers (below minSize threshold)', () => {
    it('formats small numbers without suffix', () => {
      expect(formatLargeAmounts(50, 2)).toBe('50.00');
      expect(formatLargeAmounts(999, 2)).toBe('999.00');
      expect(formatLargeAmounts(123.45, 2)).toBe('123.45');
    });

    it('uses largePrecision for small numbers when roundSmallPrecision=true', () => {
      // Default largePrecision is 3, so min(2, 3) = 2
      expect(formatLargeAmounts(50.12345, 5)).toBe('50.123');
    });

    it('uses provided precision when roundSmallPrecision=false', () => {
      expect(formatLargeAmounts(50.12345, 5, { roundSmallPrecision: false })).toBe('50.12345');
    });
  });

  describe('thousands (K suffix)', () => {
    it('formats thousands with K suffix', () => {
      expect(formatLargeAmounts(1500, 2)).toBe('1.500K');
      expect(formatLargeAmounts(5000, 2)).toBe('5.000K');
      expect(formatLargeAmounts(999999, 2)).toBe('999.999K');
    });

    it('uses largePrecision for K suffix', () => {
      expect(formatLargeAmounts(1234, 2, { largePrecision: 2 })).toBe('1.23K');
      expect(formatLargeAmounts(1234, 2, { largePrecision: 1 })).toBe('1.2K');
    });
  });

  describe('millions (M suffix)', () => {
    it('formats millions with M suffix', () => {
      expect(formatLargeAmounts(1500000, 2)).toBe('1.500M');
      expect(formatLargeAmounts(5000000, 2)).toBe('5.000M');
      expect(formatLargeAmounts(500000000, 2)).toBe('500.000M');
    });
  });

  describe('billions (B suffix)', () => {
    it('formats billions with B suffix', () => {
      expect(formatLargeAmounts(1500000000, 2)).toBe('1.500B');
      expect(formatLargeAmounts(5000000000, 2)).toBe('5.000B');
    });
  });

  describe('negative numbers', () => {
    it('handles negative values with appropriate suffix', () => {
      expect(formatLargeAmounts(-1500, 2)).toBe('-1.500K');
      expect(formatLargeAmounts(-1500000, 2)).toBe('-1.500M');
      expect(formatLargeAmounts(-1500000000, 2)).toBe('-1.500B');
      expect(formatLargeAmounts(-50, 2)).toBe('-50.00');
    });
  });

  describe('options', () => {
    describe('minSize option', () => {
      it('respects custom minSize threshold', () => {
        // With minSize=10000, 5000 should not get K suffix
        expect(formatLargeAmounts(5000, 2, { minSize: 10000 })).toBe('5000.00');
        expect(formatLargeAmounts(15000, 2, { minSize: 10000 })).toBe('15.000K');
      });
    });

    describe('largePrecision option', () => {
      it('controls decimal places for large numbers', () => {
        expect(formatLargeAmounts(1234000, 2, { largePrecision: 2 })).toBe('1.23M');
        expect(formatLargeAmounts(1234500, 2, { largePrecision: 4 })).toBe('1.2345M');
      });
    });

    describe('smallPrecision option', () => {
      it('uses smallPrecision when result would round to 0', () => {
        expect(formatLargeAmounts(0.00001, 2, { smallPrecision: 6 })).toBe('0.000010');
      });
    });

    describe('breakpointOffset option', () => {
      it('adjusts breakpoint thresholds', () => {
        // With breakpointOffset=1, K threshold becomes 10,000
        expect(formatLargeAmounts(5000, 2, { breakpointOffset: 1 })).toBe('5000.00');
        expect(formatLargeAmounts(50000, 2, { breakpointOffset: 1 })).toBe('50.000K');
      });
    });
  });

  describe('edge cases', () => {
    it('handles zero', () => {
      expect(formatLargeAmounts(0, 2)).toBe('0.00');
    });

    it('handles very small positive numbers that would round to 0', () => {
      expect(formatLargeAmounts(0.001, 2)).toBe('0.001');
      expect(formatLargeAmounts(0.001, 2, { smallPrecision: 4 })).toBe('0.0010');
      expect(formatLargeAmounts(0.001, 4)).toBe('0.001'); // with higher precision, shows actual value
    });

    it('expands low market precision enough to show non-zero values', () => {
      expect(formatLargeAmounts(0.03, 1, { largePrecision: 1 })).toBe('0.03');
      expect(formatLargeAmounts(-0.03, 1, { largePrecision: 1 })).toBe('-0.03');
    });

    it('does not expose floating-point dust as tiny non-zero values', () => {
      expect(formatLargeAmounts(0.1 + 0.2 - 0.3, 1, { largePrecision: 1 })).toBe('0.0');
      expect(formatLargeAmounts(-0.00000000000009, 1, { largePrecision: 1 })).toBe('0.0');
    });

    it('handles numbers at exact boundaries', () => {
      expect(formatLargeAmounts(1000, 2)).toBe('1000.00'); // Below K threshold (> 1000)
      expect(formatLargeAmounts(1001, 2)).toBe('1.001K'); // At K threshold
    });
  });
});
