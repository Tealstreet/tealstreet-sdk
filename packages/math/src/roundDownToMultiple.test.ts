import { describe, expect, it, vi } from 'vitest';
import { roundDownToMultiple } from './roundDownToMultiple';

// Mock handleRoundingError to test the core logic
vi.mock('./handleRoundingError', () => ({
  handleRoundingError: (value: number) => value
}));

describe('roundDownToMultiple', () => {
  it('should round down to the nearest multiple for positive numbers', () => {
    expect(roundDownToMultiple(10, 3)).toBe(9);
    expect(roundDownToMultiple(11, 3)).toBe(9);
    expect(roundDownToMultiple(12, 3)).toBe(12);
    expect(roundDownToMultiple(13, 3)).toBe(12);
    expect(roundDownToMultiple(14, 3)).toBe(12);
    expect(roundDownToMultiple(15, 3)).toBe(15);
  });

  it('should work with decimal multiples', () => {
    expect(roundDownToMultiple(1.7, 0.5)).toBeCloseTo(1.5);
    expect(roundDownToMultiple(1.8, 0.5)).toBeCloseTo(1.5);
    expect(roundDownToMultiple(2.0, 0.5)).toBeCloseTo(2.0);
    expect(roundDownToMultiple(2.2, 0.5)).toBeCloseTo(2.0);
    expect(roundDownToMultiple(2.5, 0.5)).toBeCloseTo(2.5);
  });

  it('should work with small decimal multiples', () => {
    expect(roundDownToMultiple(0.123, 0.01)).toBeCloseTo(0.12);
    expect(roundDownToMultiple(0.127, 0.01)).toBeCloseTo(0.12);
    expect(roundDownToMultiple(0.129, 0.01)).toBeCloseTo(0.12);
    expect(roundDownToMultiple(0.13, 0.01)).toBeCloseTo(0.13);
  });

  it('should work with large numbers', () => {
    expect(roundDownToMultiple(1005, 10)).toBe(1000);
    expect(roundDownToMultiple(1009, 10)).toBe(1000);
    expect(roundDownToMultiple(1010, 10)).toBe(1010);
    expect(roundDownToMultiple(10000, 100)).toBe(10000);
    expect(roundDownToMultiple(10050, 100)).toBe(10000);
    expect(roundDownToMultiple(10099, 100)).toBe(10000);
  });

  it('should handle negative numbers', () => {
    expect(roundDownToMultiple(-10, 3)).toBe(-12);
    expect(roundDownToMultiple(-11, 3)).toBe(-12);
    expect(roundDownToMultiple(-12, 3)).toBe(-12);
    expect(roundDownToMultiple(-13, 3)).toBe(-15);
    expect(roundDownToMultiple(-14, 3)).toBe(-15);
    expect(roundDownToMultiple(-15, 3)).toBe(-15);
  });

  it('should handle negative multiples', () => {
    // Note: Negative multiples behave differently - Math.floor((n + m/100000) / m) * m
    // For (10, -3): Math.floor((10 + (-3)/100000) / -3) * -3 = Math.floor(-3.333...) * -3 = -4 * -3 = 12
    expect(roundDownToMultiple(10, -3)).toBeCloseTo(12);
    expect(roundDownToMultiple(11, -3)).toBeCloseTo(12);
    expect(roundDownToMultiple(-10, -3)).toBeCloseTo(-9); // Math.floor(3.333...) * -3 = 3 * -3 = -9
    expect(roundDownToMultiple(-11, -3)).toBeCloseTo(-9); // Math.floor(3.666...) * -3 = 3 * -3 = -9
  });

  it('should handle zero input', () => {
    expect(roundDownToMultiple(0, 5)).toBe(0);
    expect(roundDownToMultiple(0, 0.1)).toBe(0);
    expect(roundDownToMultiple(0, 100)).toBe(0);
  });

  it('should handle numbers that are exact multiples', () => {
    expect(roundDownToMultiple(10, 5)).toBe(10);
    expect(roundDownToMultiple(100, 10)).toBe(100);
    expect(roundDownToMultiple(0.5, 0.1)).toBeCloseTo(0.5);
    expect(roundDownToMultiple(1.5, 0.5)).toBeCloseTo(1.5);
  });

  it('should handle very small numbers close to zero', () => {
    expect(roundDownToMultiple(0.0001, 0.001)).toBeCloseTo(0);
    expect(roundDownToMultiple(0.0009, 0.001)).toBeCloseTo(0);
    expect(roundDownToMultiple(0.001, 0.001)).toBeCloseTo(0.001);
    expect(roundDownToMultiple(0.0019, 0.001)).toBeCloseTo(0.001);
  });

  it('should handle edge cases with floating point precision', () => {
    // The function adds m/100000 to prevent floating point rounding errors
    expect(roundDownToMultiple(0.3, 0.1)).toBeCloseTo(0.3);
    expect(roundDownToMultiple(0.30000001, 0.1)).toBeCloseTo(0.3);
    // 0.29999999 is very close to 0.3, and with the m/100000 adjustment it rounds to 0.3
    expect(roundDownToMultiple(0.29999999, 0.1)).toBeCloseTo(0.3);
    
    // Test cases that might cause issues without the adjustment
    expect(roundDownToMultiple(1.1, 0.1)).toBeCloseTo(1.1);
    expect(roundDownToMultiple(2.2, 0.1)).toBeCloseTo(2.2);
    expect(roundDownToMultiple(3.3, 0.1)).toBeCloseTo(3.3);
  });

  it('should work with multiple of 1', () => {
    expect(roundDownToMultiple(5.7, 1)).toBe(5);
    expect(roundDownToMultiple(5.0, 1)).toBe(5);
    expect(roundDownToMultiple(5.9999, 1)).toBe(5);
    expect(roundDownToMultiple(-5.7, 1)).toBe(-6);
    expect(roundDownToMultiple(-5.0, 1)).toBe(-5);
  });

  it('should handle very large multiples', () => {
    expect(roundDownToMultiple(999, 1000)).toBe(0);
    expect(roundDownToMultiple(1000, 1000)).toBe(1000);
    expect(roundDownToMultiple(1999, 1000)).toBe(1000);
    expect(roundDownToMultiple(2000, 1000)).toBe(2000);
  });

  it('should handle fractional results', () => {
    expect(roundDownToMultiple(0.75, 0.25)).toBeCloseTo(0.75);
    expect(roundDownToMultiple(0.74, 0.25)).toBeCloseTo(0.5);
    expect(roundDownToMultiple(0.26, 0.25)).toBeCloseTo(0.25);
    expect(roundDownToMultiple(0.24, 0.25)).toBeCloseTo(0);
  });
});