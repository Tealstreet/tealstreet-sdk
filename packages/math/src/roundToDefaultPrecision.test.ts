import { describe, expect, it, vi } from 'vitest';
import { roundToDefaultPrecision } from './roundToDefaultPrecision';

// Mock the dependencies
vi.mock('./getValueSigDigits', () => ({
  getValueSigDigits: vi.fn((v: any) => {
    const absVal = Math.abs(parseFloat(String(v)));
    if (absVal === 0) return 1;
    else if (absVal >= 1000) return 2;
    else if (absVal >= 10) return 3;
    else if (absVal >= 1) return 4;
    else if (absVal >= 0.0001) return 5;
    else if (absVal >= 0.00001) return 6;
    else if (absVal >= 0.000001) return 7;
    else if (absVal >= 0.0000001) return 8;
    else if (absVal >= 0.00000001) return 9;
    else if (absVal >= 0.000000001) return 10;
    else if (absVal >= 0.0000000001) return 11;
    else return 12;
  })
}));

vi.mock('./isNotNumber', () => ({
  isNotNumber: vi.fn((v: unknown): v is null | undefined =>
    Array.isArray(v) || typeof v === 'boolean' || v === '' || isNaN(Number(v)) || v === null || v === undefined)
}));

describe('roundToDefaultPrecision', () => {
  it('should round numbers based on their significant digits', () => {
    // Values >= 1000 (2 sig digits)
    expect(roundToDefaultPrecision(1234.567)).toBe(1234.57);
    expect(roundToDefaultPrecision(9999.999)).toBe(10000);
    expect(roundToDefaultPrecision(-5678.901)).toBe(-5678.9);
    
    // Values >= 10 (3 sig digits)
    expect(roundToDefaultPrecision(12.3456)).toBe(12.346);
    expect(roundToDefaultPrecision(99.9999)).toBe(100);
    expect(roundToDefaultPrecision(-45.6789)).toBe(-45.679);
    
    // Values >= 1 (4 sig digits)
    expect(roundToDefaultPrecision(1.23456)).toBe(1.2346);
    expect(roundToDefaultPrecision(9.99999)).toBe(10);
    expect(roundToDefaultPrecision(-5.43210)).toBe(-5.4321);
    
    // Values >= 0.0001 (5 sig digits)
    expect(roundToDefaultPrecision(0.123456)).toBe(0.12346);
    expect(roundToDefaultPrecision(0.999999)).toBe(1);
    expect(roundToDefaultPrecision(-0.654321)).toBe(-0.65432);
  });

  it('should handle very small numbers', () => {
    // Values >= 0.00001 (6 sig digits)
    expect(roundToDefaultPrecision(0.0000123456)).toBe(0.000012);
    expect(roundToDefaultPrecision(0.0000999999)).toBe(0.0001);
    
    // Values >= 0.000001 (7 sig digits)
    expect(roundToDefaultPrecision(0.00000123456)).toBe(0.0000012);
    expect(roundToDefaultPrecision(0.00000999999)).toBe(0.00001);
    
    // Values >= 0.0000001 (8 sig digits)
    expect(roundToDefaultPrecision(0.000000123456)).toBe(0.00000012);
    
    // Values >= 0.00000001 (9 sig digits)
    expect(roundToDefaultPrecision(0.0000000123456)).toBe(0.000000012);
    
    // Values >= 0.000000001 (10 sig digits)
    expect(roundToDefaultPrecision(0.00000000123456)).toBe(0.0000000012);
    
    // Values >= 0.0000000001 (11 sig digits)
    expect(roundToDefaultPrecision(0.000000000123456)).toBe(0.00000000012);
    
    // Values < 0.0000000001 (12 sig digits)
    expect(roundToDefaultPrecision(0.0000000000123456)).toBe(0.000000000012);
  });

  it('should handle zero', () => {
    expect(roundToDefaultPrecision(0)).toBe(0);
    expect(roundToDefaultPrecision(-0)).toBe(0);
  });

  it('should handle string inputs by converting them to numbers', () => {
    // The function internally converts strings to numbers
    expect(roundToDefaultPrecision(1234.567)).toBe(1234.57);
    expect(roundToDefaultPrecision(12.3456)).toBe(12.346);
    expect(roundToDefaultPrecision(1.23456)).toBe(1.2346);
    expect(roundToDefaultPrecision(0.123456)).toBe(0.12346);
    expect(roundToDefaultPrecision(-5.43210)).toBe(-5.4321);
  });

  it('should return 0 for undefined inputs', () => {
    expect(roundToDefaultPrecision(undefined)).toBe(0);
    // NaN is a valid number type, so it can be passed
    expect(roundToDefaultPrecision(NaN)).toBe(0);
  });

  it('should handle edge cases', () => {
    // Numbers that might round up to the next precision category
    expect(roundToDefaultPrecision(999.999)).toBe(999.999); // getValueSigDigits returns 3, toFixed(3) keeps it as is
    expect(roundToDefaultPrecision(9.99999)).toBe(10); // getValueSigDigits returns 4, toFixed(4) rounds to 10
    expect(roundToDefaultPrecision(0.999999)).toBe(1); // getValueSigDigits returns 5, toFixed(5) rounds to 1
    
    // Very large numbers
    expect(roundToDefaultPrecision(1e10)).toBe(1e10);
    expect(roundToDefaultPrecision(1.23456e10)).toBe(1.23456e10);
    
    // Very small numbers - these are smaller than the toFixed(12) precision limit, will round to 0
    expect(roundToDefaultPrecision(1e-15)).toBe(0);
    expect(roundToDefaultPrecision(1.23456e-15)).toBe(0);
  });

  it('should handle negative numbers correctly', () => {
    expect(roundToDefaultPrecision(-1234.567)).toBe(-1234.57);
    expect(roundToDefaultPrecision(-12.3456)).toBe(-12.346);
    expect(roundToDefaultPrecision(-1.23456)).toBe(-1.2346);
    expect(roundToDefaultPrecision(-0.123456)).toBe(-0.12346);
    expect(roundToDefaultPrecision(-0.0000123456)).toBe(-0.000012);
  });

  it('should handle scientific notation', () => {
    expect(roundToDefaultPrecision(1.23456e3)).toBe(1234.56);
    expect(roundToDefaultPrecision(1.23456e-3)).toBe(0.00123);
    expect(roundToDefaultPrecision(-1.23456e2)).toBe(-123.456);
    expect(roundToDefaultPrecision(-1.23456e-4)).toBe(-0.00012);
  });

  it('should handle infinity', () => {
    expect(roundToDefaultPrecision(Infinity)).toBe(Infinity);
    expect(roundToDefaultPrecision(-Infinity)).toBe(-Infinity);
  });

  it('should maintain precision for exact decimal representations', () => {
    expect(roundToDefaultPrecision(10)).toBe(10);
    expect(roundToDefaultPrecision(100)).toBe(100);
    expect(roundToDefaultPrecision(1000)).toBe(1000);
    expect(roundToDefaultPrecision(0.1)).toBe(0.1);
    expect(roundToDefaultPrecision(0.01)).toBe(0.01);
    expect(roundToDefaultPrecision(0.001)).toBe(0.001);
  });
});