import { describe, it, expect } from 'vitest';
import {
  integerPrecisionToAmount,
  countDecimals,
  decimalToPrecision,
  parsePrecision,
  precisionToString,
} from './precision';

describe('ccxt_helpers/precision', () => {
  describe('integerPrecisionToAmount', () => {
    it('should convert integer precision to decimal string', () => {
      expect(integerPrecisionToAmount(10000)).toBe('0.0001');
      expect(integerPrecisionToAmount(1000)).toBe('0.001');
      expect(integerPrecisionToAmount(100)).toBe('0.01');
      expect(integerPrecisionToAmount(10)).toBe('0.1');
    });

    it('should return "1" for precision 0', () => {
      expect(integerPrecisionToAmount(0)).toBe('1');
    });

    it('should handle string input', () => {
      expect(integerPrecisionToAmount('10000')).toBe('0.0001');
      expect(integerPrecisionToAmount('100')).toBe('0.01');
    });

    it('should return undefined for undefined/null', () => {
      expect(integerPrecisionToAmount(undefined)).toBeUndefined();
      expect(integerPrecisionToAmount(null as any)).toBeUndefined();
    });

    it('should handle precision of 1', () => {
      expect(integerPrecisionToAmount(1)).toBe('1');
    });

    it('should handle large precisions (returns scientific notation)', () => {
      // NOTE: JavaScript toString() returns scientific notation for very small numbers
      expect(integerPrecisionToAmount(100000000)).toBe('1e-8');
    });
  });

  describe('countDecimals', () => {
    it('should count decimal places', () => {
      expect(countDecimals('0.0001')).toBe(4);
      expect(countDecimals('0.01')).toBe(2);
      expect(countDecimals('0.1')).toBe(1);
      expect(countDecimals('1.5')).toBe(1);
      expect(countDecimals('1.25')).toBe(2);
    });

    it('should return 0 for integers', () => {
      expect(countDecimals('1')).toBe(0);
      expect(countDecimals('100')).toBe(0);
      expect(countDecimals(42)).toBe(0);
    });

    it('should handle numbers with trailing zeros', () => {
      // Trailing zeros should be removed
      expect(countDecimals('1.500')).toBe(1);
      expect(countDecimals('0.0100')).toBe(2);
    });

    it('should return 0 for undefined/null', () => {
      expect(countDecimals(undefined)).toBe(0);
      expect(countDecimals(null as any)).toBe(0);
    });

    it('should handle scientific notation', () => {
      expect(countDecimals('1e-4')).toBe(4);
      expect(countDecimals('1E-8')).toBe(8);
      expect(countDecimals('2.5e-3')).toBe(4); // 2.5 has 1 decimal + 3 from exponent
    });

    it('should handle number input', () => {
      expect(countDecimals(0.0001)).toBe(4);
      expect(countDecimals(0.01)).toBe(2);
    });
  });

  describe('decimalToPrecision', () => {
    it('should return number of decimal places', () => {
      expect(decimalToPrecision('0.0001')).toBe(4);
      expect(decimalToPrecision('0.01')).toBe(2);
      expect(decimalToPrecision('0.1')).toBe(1);
    });

    it('should return 0 for 1', () => {
      expect(decimalToPrecision('1')).toBe(0);
      expect(decimalToPrecision(1)).toBe(0);
    });

    it('should return 0 for undefined/null', () => {
      expect(decimalToPrecision(undefined)).toBe(0);
      expect(decimalToPrecision(null as any)).toBe(0);
    });

    it('should handle number input', () => {
      expect(decimalToPrecision(0.0001)).toBe(4);
      expect(decimalToPrecision(0.01)).toBe(2);
    });
  });

  describe('parsePrecision', () => {
    describe('decimal mode (default)', () => {
      it('should return decimal places count', () => {
        expect(parsePrecision('0.0001')).toBe(4);
        expect(parsePrecision('0.01')).toBe(2);
        expect(parsePrecision('0.1')).toBe(1);
      });

      it('should return 0 for undefined/null', () => {
        expect(parsePrecision(undefined)).toBe(0);
        expect(parsePrecision(null as any)).toBe(0);
      });
    });

    describe('integer mode (isInteger=true)', () => {
      it('should convert integer precision then count decimals', () => {
        expect(parsePrecision(10000, true)).toBe(4);
        expect(parsePrecision(100, true)).toBe(2);
        expect(parsePrecision(10, true)).toBe(1);
      });

      it('should return 0 for precision 1', () => {
        expect(parsePrecision(1, true)).toBe(0);
      });

      it('should handle string input', () => {
        expect(parsePrecision('10000', true)).toBe(4);
      });
    });
  });

  describe('precisionToString', () => {
    it('should convert precision to decimal string', () => {
      expect(precisionToString(0)).toBe('1');
      expect(precisionToString(1)).toBe('0.1');
      expect(precisionToString(2)).toBe('0.01');
      expect(precisionToString(4)).toBe('0.0001');
      expect(precisionToString(8)).toBe('0.00000001');
    });

    it('should handle large precision values', () => {
      expect(precisionToString(10)).toBe('0.0000000001');
    });
  });
});
