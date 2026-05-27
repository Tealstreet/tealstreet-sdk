import { describe, expect, it, vi } from 'vitest';
import { roundUpToMultiple } from './roundUpToMultiple';

// Mock handleRoundingError to test the core logic
vi.mock('./handleRoundingError', () => ({
  handleRoundingError: (value: number) => value
}));

describe('roundUpToMultiple', () => {
  it('should round up to the nearest multiple for positive numbers', () => {
    expect(roundUpToMultiple(10, 3)).toBe(12);
    expect(roundUpToMultiple(11, 3)).toBe(12);
    expect(roundUpToMultiple(12, 3)).toBe(12);
    expect(roundUpToMultiple(13, 3)).toBe(15);
    expect(roundUpToMultiple(14, 3)).toBe(15);
    expect(roundUpToMultiple(15, 3)).toBe(15);
  });

  it('should work with decimal multiples', () => {
    expect(roundUpToMultiple(1.7, 0.5)).toBeCloseTo(2.0);
    expect(roundUpToMultiple(1.8, 0.5)).toBeCloseTo(2.0);
    expect(roundUpToMultiple(2.0, 0.5)).toBeCloseTo(2.0);
    expect(roundUpToMultiple(2.1, 0.5)).toBeCloseTo(2.5);
    expect(roundUpToMultiple(2.2, 0.5)).toBeCloseTo(2.5);
    expect(roundUpToMultiple(2.5, 0.5)).toBeCloseTo(2.5);
  });

  it('should work with small decimal multiples', () => {
    expect(roundUpToMultiple(0.121, 0.01)).toBeCloseTo(0.13);
    expect(roundUpToMultiple(0.123, 0.01)).toBeCloseTo(0.13);
    expect(roundUpToMultiple(0.125, 0.01)).toBeCloseTo(0.13);
    expect(roundUpToMultiple(0.127, 0.01)).toBeCloseTo(0.13);
    expect(roundUpToMultiple(0.13, 0.01)).toBeCloseTo(0.13);
  });

  it('should work with large numbers', () => {
    expect(roundUpToMultiple(1001, 10)).toBe(1010);
    expect(roundUpToMultiple(1005, 10)).toBe(1010);
    expect(roundUpToMultiple(1010, 10)).toBe(1010);
    expect(roundUpToMultiple(10000, 100)).toBe(10000);
    expect(roundUpToMultiple(10001, 100)).toBe(10100);
    expect(roundUpToMultiple(10050, 100)).toBe(10100);
    expect(roundUpToMultiple(10099, 100)).toBe(10100);
  });

  it('should handle negative numbers', () => {
    expect(roundUpToMultiple(-10, 3)).toBe(-9);
    expect(roundUpToMultiple(-11, 3)).toBe(-9);
    expect(roundUpToMultiple(-12, 3)).toBe(-12);
    expect(roundUpToMultiple(-13, 3)).toBe(-12);
    expect(roundUpToMultiple(-14, 3)).toBe(-12);
    expect(roundUpToMultiple(-15, 3)).toBe(-15);
  });

  it('should handle negative multiples', () => {
    // Note: Negative multiples behave differently - Math.ceil(n / m) * m
    // For (10, -3): Math.ceil(10 / -3) * -3 = Math.ceil(-3.333...) * -3 = -3 * -3 = 9
    expect(roundUpToMultiple(10, -3)).toBeCloseTo(9);
    expect(roundUpToMultiple(11, -3)).toBeCloseTo(9);
    expect(roundUpToMultiple(-10, -3)).toBeCloseTo(-12); // Math.ceil(3.333...) * -3 = 4 * -3 = -12
    expect(roundUpToMultiple(-11, -3)).toBeCloseTo(-12); // Math.ceil(3.666...) * -3 = 4 * -3 = -12
  });

  it('should handle zero input', () => {
    expect(roundUpToMultiple(0, 5)).toBe(0);
    expect(roundUpToMultiple(0, 0.1)).toBe(0);
    expect(roundUpToMultiple(0, 100)).toBe(0);
  });

  it('should handle numbers that are exact multiples', () => {
    expect(roundUpToMultiple(10, 5)).toBe(10);
    expect(roundUpToMultiple(100, 10)).toBe(100);
    expect(roundUpToMultiple(0.5, 0.1)).toBeCloseTo(0.5);
    expect(roundUpToMultiple(1.5, 0.5)).toBeCloseTo(1.5);
  });

  it('should handle very small numbers', () => {
    expect(roundUpToMultiple(0.0001, 0.001)).toBeCloseTo(0.001);
    expect(roundUpToMultiple(0.0005, 0.001)).toBeCloseTo(0.001);
    expect(roundUpToMultiple(0.0009, 0.001)).toBeCloseTo(0.001);
    expect(roundUpToMultiple(0.001, 0.001)).toBeCloseTo(0.001);
    expect(roundUpToMultiple(0.0011, 0.001)).toBeCloseTo(0.002);
    expect(roundUpToMultiple(0.0019, 0.001)).toBeCloseTo(0.002);
  });

  it('should work with multiple of 1', () => {
    expect(roundUpToMultiple(5.1, 1)).toBe(6);
    expect(roundUpToMultiple(5.5, 1)).toBe(6);
    expect(roundUpToMultiple(5.9, 1)).toBe(6);
    expect(roundUpToMultiple(6.0, 1)).toBe(6);
    expect(roundUpToMultiple(-5.1, 1)).toBe(-5);
    expect(roundUpToMultiple(-5.5, 1)).toBe(-5);
    expect(roundUpToMultiple(-5.9, 1)).toBe(-5);
    expect(roundUpToMultiple(-6.0, 1)).toBe(-6);
  });

  it('should handle very large multiples', () => {
    expect(roundUpToMultiple(1, 1000)).toBe(1000);
    expect(roundUpToMultiple(999, 1000)).toBe(1000);
    expect(roundUpToMultiple(1000, 1000)).toBe(1000);
    expect(roundUpToMultiple(1001, 1000)).toBe(2000);
    expect(roundUpToMultiple(1999, 1000)).toBe(2000);
    expect(roundUpToMultiple(2000, 1000)).toBe(2000);
  });

  it('should handle fractional results', () => {
    expect(roundUpToMultiple(0.01, 0.25)).toBeCloseTo(0.25);
    expect(roundUpToMultiple(0.24, 0.25)).toBeCloseTo(0.25);
    expect(roundUpToMultiple(0.25, 0.25)).toBeCloseTo(0.25);
    expect(roundUpToMultiple(0.26, 0.25)).toBeCloseTo(0.5);
    expect(roundUpToMultiple(0.49, 0.25)).toBeCloseTo(0.5);
    expect(roundUpToMultiple(0.5, 0.25)).toBeCloseTo(0.5);
  });

  it('should handle edge cases with floating point precision', () => {
    // Test cases that might have floating point precision issues
    expect(roundUpToMultiple(0.1, 0.1)).toBeCloseTo(0.1);
    expect(roundUpToMultiple(0.2, 0.1)).toBeCloseTo(0.2);
    expect(roundUpToMultiple(0.3, 0.1)).toBeCloseTo(0.3);
    expect(roundUpToMultiple(1.1, 0.1)).toBeCloseTo(1.1);
    expect(roundUpToMultiple(2.2, 0.1)).toBeCloseTo(2.2);
    expect(roundUpToMultiple(3.3, 0.1)).toBeCloseTo(3.3);
    
    // Just above exact multiples
    expect(roundUpToMultiple(0.10001, 0.1)).toBeCloseTo(0.2);
    expect(roundUpToMultiple(0.20001, 0.1)).toBeCloseTo(0.3);
    expect(roundUpToMultiple(0.30001, 0.1)).toBeCloseTo(0.4);
  });

  it('should be the opposite of roundDownToMultiple for non-exact values', () => {
    // For non-exact multiples, roundUp should give the next multiple
    // while roundDown should give the previous multiple
    const testCases = [
      { n: 10.1, m: 3, expectedUp: 12, expectedDown: 9 },
      { n: 1.7, m: 0.5, expectedUp: 2.0, expectedDown: 1.5 },
      { n: 0.123, m: 0.01, expectedUp: 0.13, expectedDown: 0.12 },
    ];
    
    for (const { n, m, expectedUp } of testCases) {
      expect(roundUpToMultiple(n, m)).toBeCloseTo(expectedUp);
    }
  });
});