import { describe, expect, it, vi } from 'vitest';
import { roundToNearestMultiple, roundToNearestMultipleDown } from './roundToNearestMultiple';

// Mock handleRoundingError to test the core logic
vi.mock('./handleRoundingError', () => ({
  handleRoundingError: (value: number) => value
}));

describe('roundToNearestMultiple', () => {
  it('should round to the nearest multiple for positive numbers', () => {
    expect(roundToNearestMultiple(10, 3)).toBe(9);
    expect(roundToNearestMultiple(11, 3)).toBe(12);
    expect(roundToNearestMultiple(12, 3)).toBe(12);
    expect(roundToNearestMultiple(13, 3)).toBe(12);
    expect(roundToNearestMultiple(14, 3)).toBe(15);
    expect(roundToNearestMultiple(15, 3)).toBe(15);
  });

  it('should round halfway cases up', () => {
    expect(roundToNearestMultiple(10.5, 1)).toBe(11);
    expect(roundToNearestMultiple(11.5, 1)).toBe(12);
    expect(roundToNearestMultiple(12.5, 1)).toBe(13);
    expect(roundToNearestMultiple(1.5, 1)).toBe(2);
    expect(roundToNearestMultiple(2.5, 1)).toBe(3);
  });

  it('should work with decimal multiples', () => {
    expect(roundToNearestMultiple(1.7, 0.5)).toBeCloseTo(1.5);
    expect(roundToNearestMultiple(1.8, 0.5)).toBeCloseTo(2.0);
    expect(roundToNearestMultiple(2.0, 0.5)).toBeCloseTo(2.0);
    expect(roundToNearestMultiple(2.2, 0.5)).toBeCloseTo(2.0);
    expect(roundToNearestMultiple(2.3, 0.5)).toBeCloseTo(2.5);
    expect(roundToNearestMultiple(2.5, 0.5)).toBeCloseTo(2.5);
  });

  it('should work with small decimal multiples', () => {
    expect(roundToNearestMultiple(0.123, 0.01)).toBeCloseTo(0.12);
    expect(roundToNearestMultiple(0.125, 0.01)).toBeCloseTo(0.13);
    expect(roundToNearestMultiple(0.127, 0.01)).toBeCloseTo(0.13);
    expect(roundToNearestMultiple(0.129, 0.01)).toBeCloseTo(0.13);
    expect(roundToNearestMultiple(0.13, 0.01)).toBeCloseTo(0.13);
  });

  it('should work with large numbers', () => {
    expect(roundToNearestMultiple(1004, 10)).toBe(1000);
    expect(roundToNearestMultiple(1005, 10)).toBe(1010);
    expect(roundToNearestMultiple(1010, 10)).toBe(1010);
    expect(roundToNearestMultiple(10000, 100)).toBe(10000);
    expect(roundToNearestMultiple(10049, 100)).toBe(10000);
    expect(roundToNearestMultiple(10050, 100)).toBe(10100);
  });

  it('should handle negative numbers', () => {
    expect(roundToNearestMultiple(-10, 3)).toBe(-9);
    expect(roundToNearestMultiple(-11, 3)).toBe(-12);
    expect(roundToNearestMultiple(-12, 3)).toBe(-12);
    expect(roundToNearestMultiple(-13, 3)).toBe(-12);
    expect(roundToNearestMultiple(-14, 3)).toBe(-15);
    expect(roundToNearestMultiple(-15, 3)).toBe(-15);
  });

  it('should handle negative multiples', () => {
    // Note: Negative multiples behave differently - Math.round(n / m) * m
    expect(roundToNearestMultiple(10, -3)).toBeCloseTo(9); // Math.round(-3.333...) * -3 = -3 * -3 = 9
    expect(roundToNearestMultiple(11, -3)).toBeCloseTo(12); // Math.round(-3.666...) * -3 = -4 * -3 = 12
    expect(roundToNearestMultiple(-10, -3)).toBeCloseTo(-9); // Math.round(3.333...) * -3 = 3 * -3 = -9
    expect(roundToNearestMultiple(-11, -3)).toBeCloseTo(-12); // Math.round(3.666...) * -3 = 4 * -3 = -12
  });

  it('should handle zero input', () => {
    expect(roundToNearestMultiple(0, 5)).toBe(0);
    expect(roundToNearestMultiple(0, 0.1)).toBe(0);
    expect(roundToNearestMultiple(0, 100)).toBe(0);
  });

  it('should handle numbers that are exact multiples', () => {
    expect(roundToNearestMultiple(10, 5)).toBe(10);
    expect(roundToNearestMultiple(100, 10)).toBe(100);
    expect(roundToNearestMultiple(0.5, 0.1)).toBeCloseTo(0.5);
    expect(roundToNearestMultiple(1.5, 0.5)).toBeCloseTo(1.5);
  });

  it('should handle very small numbers', () => {
    expect(roundToNearestMultiple(0.0001, 0.001)).toBeCloseTo(0);
    expect(roundToNearestMultiple(0.0005, 0.001)).toBeCloseTo(0.001);
    expect(roundToNearestMultiple(0.0009, 0.001)).toBeCloseTo(0.001);
    expect(roundToNearestMultiple(0.001, 0.001)).toBeCloseTo(0.001);
    expect(roundToNearestMultiple(0.0015, 0.001)).toBeCloseTo(0.002);
  });

  it('should work with multiple of 1', () => {
    expect(roundToNearestMultiple(5.4, 1)).toBe(5);
    expect(roundToNearestMultiple(5.5, 1)).toBe(6);
    expect(roundToNearestMultiple(5.6, 1)).toBe(6);
    expect(roundToNearestMultiple(-5.4, 1)).toBe(-5);
    expect(roundToNearestMultiple(-5.5, 1)).toBe(-5); // Math.round(-5.5) = -5 in JavaScript
    expect(roundToNearestMultiple(-5.6, 1)).toBe(-6);
  });

  it('should handle very large multiples', () => {
    expect(roundToNearestMultiple(499, 1000)).toBe(0);
    expect(roundToNearestMultiple(500, 1000)).toBe(1000);
    expect(roundToNearestMultiple(1499, 1000)).toBe(1000);
    expect(roundToNearestMultiple(1500, 1000)).toBe(2000);
  });

  it('should handle fractional results', () => {
    expect(roundToNearestMultiple(0.62, 0.25)).toBeCloseTo(0.5);
    expect(roundToNearestMultiple(0.63, 0.25)).toBeCloseTo(0.75);
    expect(roundToNearestMultiple(0.87, 0.25)).toBeCloseTo(0.75);
    expect(roundToNearestMultiple(0.88, 0.25)).toBeCloseTo(1.0);
  });
});

describe('roundToNearestMultipleDown', () => {
  it('should always round down to the nearest multiple', () => {
    expect(roundToNearestMultipleDown(10, 3)).toBe(9);
    expect(roundToNearestMultipleDown(11, 3)).toBe(9);
    expect(roundToNearestMultipleDown(12, 3)).toBe(12);
    expect(roundToNearestMultipleDown(13, 3)).toBe(12);
    expect(roundToNearestMultipleDown(14, 3)).toBe(12);
    expect(roundToNearestMultipleDown(15, 3)).toBe(15);
  });

  it('should round down even for halfway cases', () => {
    expect(roundToNearestMultipleDown(10.5, 1)).toBe(10);
    expect(roundToNearestMultipleDown(11.5, 1)).toBe(11);
    expect(roundToNearestMultipleDown(12.5, 1)).toBe(12);
    expect(roundToNearestMultipleDown(1.5, 1)).toBe(1);
    expect(roundToNearestMultipleDown(2.5, 1)).toBe(2);
  });

  it('should work with decimal multiples', () => {
    expect(roundToNearestMultipleDown(1.7, 0.5)).toBeCloseTo(1.5);
    expect(roundToNearestMultipleDown(1.8, 0.5)).toBeCloseTo(1.5);
    expect(roundToNearestMultipleDown(2.0, 0.5)).toBeCloseTo(2.0);
    expect(roundToNearestMultipleDown(2.2, 0.5)).toBeCloseTo(2.0);
    expect(roundToNearestMultipleDown(2.3, 0.5)).toBeCloseTo(2.0);
    expect(roundToNearestMultipleDown(2.5, 0.5)).toBeCloseTo(2.5);
  });

  it('should handle negative numbers', () => {
    expect(roundToNearestMultipleDown(-10, 3)).toBe(-12);
    expect(roundToNearestMultipleDown(-11, 3)).toBe(-12);
    expect(roundToNearestMultipleDown(-12, 3)).toBe(-12);
    expect(roundToNearestMultipleDown(-13, 3)).toBe(-15);
    expect(roundToNearestMultipleDown(-14, 3)).toBe(-15);
    expect(roundToNearestMultipleDown(-15, 3)).toBe(-15);
  });

  it('should handle zero input', () => {
    expect(roundToNearestMultipleDown(0, 5)).toBe(0);
    expect(roundToNearestMultipleDown(0, 0.1)).toBe(0);
    expect(roundToNearestMultipleDown(0, 100)).toBe(0);
  });

  it('should handle exact multiples', () => {
    expect(roundToNearestMultipleDown(10, 5)).toBe(10);
    expect(roundToNearestMultipleDown(100, 10)).toBe(100);
    expect(roundToNearestMultipleDown(0.5, 0.1)).toBeCloseTo(0.5);
    expect(roundToNearestMultipleDown(1.5, 0.5)).toBeCloseTo(1.5);
  });

  it('should handle very small numbers', () => {
    expect(roundToNearestMultipleDown(0.0001, 0.001)).toBeCloseTo(0);
    expect(roundToNearestMultipleDown(0.0005, 0.001)).toBeCloseTo(0);
    expect(roundToNearestMultipleDown(0.0009, 0.001)).toBeCloseTo(0);
    expect(roundToNearestMultipleDown(0.001, 0.001)).toBeCloseTo(0.001);
    expect(roundToNearestMultipleDown(0.0015, 0.001)).toBeCloseTo(0.001);
    expect(roundToNearestMultipleDown(0.0019, 0.001)).toBeCloseTo(0.001);
  });

  it('should work differently from roundToNearestMultiple', () => {
    // Cases where they differ
    expect(roundToNearestMultiple(11, 3)).toBe(12);
    expect(roundToNearestMultipleDown(11, 3)).toBe(9);
    
    expect(roundToNearestMultiple(1.8, 0.5)).toBeCloseTo(2.0);
    expect(roundToNearestMultipleDown(1.8, 0.5)).toBeCloseTo(1.5);
    
    expect(roundToNearestMultiple(10.5, 1)).toBe(11);
    expect(roundToNearestMultipleDown(10.5, 1)).toBe(10);
    
    // Cases where they're the same
    expect(roundToNearestMultiple(12, 3)).toBe(12);
    expect(roundToNearestMultipleDown(12, 3)).toBe(12);
    
    expect(roundToNearestMultiple(2.0, 0.5)).toBeCloseTo(2.0);
    expect(roundToNearestMultipleDown(2.0, 0.5)).toBeCloseTo(2.0);
  });
});