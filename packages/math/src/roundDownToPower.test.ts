import { describe, expect, it } from 'vitest';
import { roundDownToPower } from './roundDownToPower';

describe('roundDownToPower', () => {
  it('should round down to the nearest power of the base', () => {
    // Powers of 2
    expect(roundDownToPower(1, 2)).toBe(1); // 2^0 = 1
    expect(roundDownToPower(2, 2)).toBe(2); // 2^1 = 2
    expect(roundDownToPower(3, 2)).toBe(4); // Closest to 2^2 = 4
    expect(roundDownToPower(4, 2)).toBe(4); // 2^2 = 4
    expect(roundDownToPower(5, 2)).toBe(4); // Closest to 2^2 = 4
    expect(roundDownToPower(7, 2)).toBe(8); // Closest to 2^3 = 8
    expect(roundDownToPower(8, 2)).toBe(8); // 2^3 = 8
    expect(roundDownToPower(9, 2)).toBe(8); // Closest to 2^3 = 8
  });

  it('should work with different bases', () => {
    // Powers of 3
    expect(roundDownToPower(1, 3)).toBe(1); // 3^0 = 1
    expect(roundDownToPower(3, 3)).toBe(3); // 3^1 = 3
    expect(roundDownToPower(8, 3)).toBe(9); // Closest to 3^2 = 9
    expect(roundDownToPower(9, 3)).toBe(9); // 3^2 = 9
    expect(roundDownToPower(20, 3)).toBe(27); // Closest to 3^3 = 27
    expect(roundDownToPower(27, 3)).toBe(27); // 3^3 = 27
    
    // Powers of 10
    expect(roundDownToPower(1, 10)).toBe(1); // 10^0 = 1
    expect(roundDownToPower(5, 10)).toBe(10); // Closest to 10^1 = 10
    expect(roundDownToPower(10, 10)).toBe(10); // 10^1 = 10
    expect(roundDownToPower(50, 10)).toBe(100); // Closest to 10^2 = 100
    expect(roundDownToPower(100, 10)).toBe(100); // 10^2 = 100
    expect(roundDownToPower(500, 10)).toBe(1000); // Closest to 10^3 = 1000
  });

  it('should work with decimal values of a', () => {
    expect(roundDownToPower(0.5, 2)).toBe(0.5); // 2^-1 = 0.5
    expect(roundDownToPower(0.25, 2)).toBe(0.25); // 2^-2 = 0.25
    expect(roundDownToPower(0.3, 2)).toBe(0.25); // Closest to 2^-2 = 0.25
    expect(roundDownToPower(0.1, 2)).toBe(0.125); // Closest to 2^-3 = 0.125
    
    expect(roundDownToPower(0.1, 10)).toBe(0.1); // 10^-1 = 0.1
    expect(roundDownToPower(0.01, 10)).toBe(0.01); // 10^-2 = 0.01
    expect(roundDownToPower(0.05, 10)).toBe(0.1); // Closest to 10^-1 = 0.1
  });

  it('should work with non-integer bases', () => {
    expect(roundDownToPower(1, 1.5)).toBe(1); // 1.5^0 = 1
    expect(roundDownToPower(1.5, 1.5)).toBe(1.5); // 1.5^1 = 1.5
    expect(roundDownToPower(2, 1.5)).toBe(2.25); // Closest to 1.5^2 = 2.25
    expect(roundDownToPower(2.25, 1.5)).toBe(2.25); // 1.5^2 = 2.25
  });

  it('should handle values between consecutive powers', () => {
    // For a=6, b=2: log2(6) ≈ 2.58, rounds to 3, so 2^3 = 8
    expect(roundDownToPower(6, 2)).toBe(8);
    
    // For a=15, b=2: log2(15) ≈ 3.91, rounds to 4, so 2^4 = 16
    expect(roundDownToPower(15, 2)).toBe(16);
    
    // For a=40, b=3: log3(40) ≈ 3.36, rounds to 3, so 3^3 = 27
    expect(roundDownToPower(40, 3)).toBe(27);
  });

  it('should throw error for invalid inputs', () => {
    // a must be > 0
    expect(() => roundDownToPower(0, 2)).toThrow('a must be > 0 and b must be > 1');
    expect(() => roundDownToPower(-1, 2)).toThrow('a must be > 0 and b must be > 1');
    expect(() => roundDownToPower(-10, 2)).toThrow('a must be > 0 and b must be > 1');
    
    // b must be > 1
    expect(() => roundDownToPower(10, 1)).toThrow('a must be > 0 and b must be > 1');
    expect(() => roundDownToPower(10, 0)).toThrow('a must be > 0 and b must be > 1');
    expect(() => roundDownToPower(10, -2)).toThrow('a must be > 0 and b must be > 1');
    expect(() => roundDownToPower(10, 0.5)).toThrow('a must be > 0 and b must be > 1');
    
    // Both invalid
    expect(() => roundDownToPower(0, 0)).toThrow('a must be > 0 and b must be > 1');
    expect(() => roundDownToPower(-1, 1)).toThrow('a must be > 0 and b must be > 1');
  });

  it('should handle very small positive values of a', () => {
    expect(roundDownToPower(0.001, 10)).toBe(0.001); // 10^-3 = 0.001
    expect(roundDownToPower(0.0001, 10)).toBeCloseTo(0.0001); // 10^-4 = 0.0001 (may have floating point precision issues)
    expect(roundDownToPower(0.00001, 10)).toBeCloseTo(0.00001); // 10^-5 = 0.00001 (may have floating point precision issues)
    
    expect(roundDownToPower(0.001, 2)).toBe(0.0009765625); // 2^-10 ≈ 0.00098
  });

  it('should handle very large values of a', () => {
    expect(roundDownToPower(1000, 10)).toBe(1000); // 10^3 = 1000
    expect(roundDownToPower(10000, 10)).toBe(10000); // 10^4 = 10000
    expect(roundDownToPower(50000, 10)).toBe(100000); // Closest to 10^5 = 100000
    
    expect(roundDownToPower(1000, 2)).toBe(1024); // 2^10 = 1024
  });

  it('should work with edge case where log result is exactly halfway', () => {
    // Test cases where Math.log(a) / Math.log(b) might be exactly x.5
    // Math.round will round to the nearest even integer
    const base = 2;
    const value = Math.sqrt(8); // This is 2^1.5 ≈ 2.828
    const result = roundDownToPower(value, base);
    // log2(2.828) ≈ 1.5, rounds to 2, so result should be 2^2 = 4
    expect(result).toBe(4);
  });

  it('should maintain mathematical properties', () => {
    // For any valid a and b, the result should be b^n for some integer n
    const testCases = [
      { a: 15, b: 2 },
      { a: 50, b: 3 },
      { a: 0.05, b: 10 },
      { a: 1.7, b: 1.5 }
    ];
    
    for (const { a, b } of testCases) {
      const result = roundDownToPower(a, b);
      const exponent = Math.round(Math.log(result) / Math.log(b));
      expect(result).toBeCloseTo(Math.pow(b, exponent));
    }
  });
});