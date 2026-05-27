import { describe, expect, it } from 'vitest';
import { getSigDigitsFromPrecision } from './getSigDigitsFromPrecision';

describe('getSigDigitsFromPrecision', () => {
  it('should return 1 for single digit numbers', () => {
    expect(getSigDigitsFromPrecision(1)).toBe(1);
    expect(getSigDigitsFromPrecision(5)).toBe(1);
    expect(getSigDigitsFromPrecision(9)).toBe(1);
  });

  it('should return correct significant digits for decimal numbers', () => {
    expect(getSigDigitsFromPrecision(0.1)).toBe(1);
    expect(getSigDigitsFromPrecision(0.01)).toBe(2); // "0.01" -> "001" -> "01" = 2 digits
    expect(getSigDigitsFromPrecision(0.001)).toBe(3); // "0.001" -> "0001" -> "001" = 3 digits
    expect(getSigDigitsFromPrecision(0.123)).toBe(3);
    expect(getSigDigitsFromPrecision(0.1234)).toBe(4);
  });

  it('should return correct significant digits for whole numbers', () => {
    expect(getSigDigitsFromPrecision(10)).toBe(1);
    expect(getSigDigitsFromPrecision(100)).toBe(1);
    expect(getSigDigitsFromPrecision(123)).toBe(3);
    expect(getSigDigitsFromPrecision(1234)).toBe(4);
  });

  it('should handle scientific notation', () => {
    expect(getSigDigitsFromPrecision(1e-5)).toBe(5); // "0.00001" -> "000001" -> "00001" = 5 digits
    expect(getSigDigitsFromPrecision(1.23e-5)).toBe(7); // "0.0000123" -> "00000123" -> "0000123" = 7 digits
    expect(getSigDigitsFromPrecision(1e5)).toBe(1);
    expect(getSigDigitsFromPrecision(1.23e5)).toBe(3);
    // Test with larger exponents that produce actual 'e' notation in toString()
    expect(getSigDigitsFromPrecision(1e-20)).toBe(20); // Negative exponent, produces '1e-20'
    expect(getSigDigitsFromPrecision(1e21)).toBe(1); // Positive exponent, produces '1e+21'
  });

  it('should return 0 for zero', () => {
    expect(getSigDigitsFromPrecision(0)).toBe(0);
  });

  it('should handle numbers with trailing zeros', () => {
    expect(getSigDigitsFromPrecision(1.20)).toBe(2);
    expect(getSigDigitsFromPrecision(1.200)).toBe(2);
  });

  it('should handle very small numbers', () => {
    expect(getSigDigitsFromPrecision(0.00000123)).toBe(8); // "0.00000123" -> "000000123" -> "00000123" = 8 digits
    expect(getSigDigitsFromPrecision(0.0000000001)).toBe(10); // "0.0000000001" -> "00000000001" -> "0000000001" = 10 digits
  });
});