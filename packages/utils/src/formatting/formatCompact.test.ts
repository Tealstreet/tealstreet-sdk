import { describe, expect, it } from 'vitest';
import { formatPriceCompact, formatSizeCompact } from './formatCompact';

describe('formatPriceCompact', () => {
  describe('large prices (>= 10000)', () => {
    it('formats with thousand separator and $ prefix', () => {
      expect(formatPriceCompact(97234.56)).toBe('$97,234.56');
      expect(formatPriceCompact(100000)).toBe('$100,000.00');
      expect(formatPriceCompact(1234567.89)).toBe('$1,234,567.89');
    });

    it('limits decimals to 2 for large prices', () => {
      expect(formatPriceCompact(10000.12345)).toBe('$10,000.12');
    });
  });

  describe('medium prices (1 - 10000)', () => {
    it('formats with $ prefix and 2 decimals by default', () => {
      expect(formatPriceCompact(1000)).toBe('$1000.00');
      expect(formatPriceCompact(99.99)).toBe('$99.99');
      expect(formatPriceCompact(1.5)).toBe('$1.50');
    });
  });

  describe('small prices (< 1)', () => {
    it('uses 4 decimals by default for small prices', () => {
      expect(formatPriceCompact(0.1234)).toBe('$0.1234');
      expect(formatPriceCompact(0.0001)).toBe('$0.0001');
    });
  });

  describe('with precision parameter', () => {
    it('uses calculated decimals from precision', () => {
      expect(formatPriceCompact(0.00012345, 0.00000001)).toBe('$0.00012345');
      expect(formatPriceCompact(89631.5, 0.1)).toBe('$89,631.5');
    });

    it('limits decimals for large values even with precision', () => {
      expect(formatPriceCompact(50000, 0.0001)).toBe('$50,000.00');
    });
  });

  describe('edge cases', () => {
    it('handles zero', () => {
      expect(formatPriceCompact(0)).toBe('$0.0000');
    });

    it('handles very small numbers', () => {
      expect(formatPriceCompact(0.00000001, 0.00000001)).toBe('$0.00000001');
    });
  });
});

describe('formatSizeCompact', () => {
  describe('millions (>= 1M)', () => {
    it('formats with M suffix', () => {
      expect(formatSizeCompact(1500000)).toBe('1.50M');
      expect(formatSizeCompact(5000000)).toBe('5.00M');
    });
  });

  describe('ten thousands (>= 10k)', () => {
    it('formats with k suffix', () => {
      expect(formatSizeCompact(15000)).toBe('15.0k');
      expect(formatSizeCompact(99999)).toBe('100.0k');
    });
  });

  describe('smaller values (< 10k)', () => {
    it('formats without suffix', () => {
      expect(formatSizeCompact(5000)).toBe('5000.00');
      expect(formatSizeCompact(100)).toBe('100.00');
    });

    it('uses 4 decimals for values less than 1', () => {
      expect(formatSizeCompact(0.005)).toBe('0.0050');
      expect(formatSizeCompact(0.1234)).toBe('0.1234');
    });
  });

  describe('with precision parameter', () => {
    it('uses calculated decimals from precision', () => {
      expect(formatSizeCompact(0.00012345, 0.00000001)).toBe('0.00012345');
      expect(formatSizeCompact(100, 0.001)).toBe('100.000');
    });

    it.each([1, 10])('uses no decimal places for a whole-unit step of %s', (precision) => {
      expect(formatSizeCompact(400, precision)).toBe('400');
    });
  });

  describe('edge cases', () => {
    it('handles zero', () => {
      expect(formatSizeCompact(0)).toBe('0.0000');
    });

    it('handles boundary values', () => {
      expect(formatSizeCompact(9999)).toBe('9999.00');
      expect(formatSizeCompact(10000)).toBe('10.0k');
      expect(formatSizeCompact(999999)).toBe('1000.0k');
      expect(formatSizeCompact(1000000)).toBe('1.00M');
    });
  });
});
