import { describe, expect, it } from 'vitest';
import { distributeAmounts } from './distributeAmounts';

describe('distributeAmounts', () => {
  it('should throw error for empty distribution', () => {
    expect(() => distributeAmounts(100, 5, [])).toThrow('Distribution cannot be empty');
  });

  it('should distribute amount equally for flat distribution', () => {
    const result = distributeAmounts(100, 4, [1, 1, 1, 1]);
    expect(result).toHaveLength(4);
    expect(result.every(val => val === 25)).toBe(true);
    expect(result.reduce((sum, val) => sum + val, 0)).toBeCloseTo(100);
  });

  it('should distribute amount according to weighted distribution', () => {
    const result = distributeAmounts(100, 3, [1, 2, 3]);
    expect(result).toHaveLength(3);
    // The sum should be approximately 100
    expect(result.reduce((sum, val) => sum + val, 0)).toBeCloseTo(100);
    // Values should be in ascending order based on distribution weights
    expect(result[0]).toBeLessThan(result[1]);
    expect(result[1]).toBeLessThan(result[2]);
  });

  it('should handle custom scale percent', () => {
    const result = distributeAmounts(100, 2, [1, 2], 0.5);
    expect(result).toHaveLength(2);
    expect(result.reduce((sum, val) => sum + val, 0)).toBeCloseTo(100);
  });

  it('should handle large amounts', () => {
    const result = distributeAmounts(1000000, 5, [1, 2, 3, 4, 5]);
    expect(result).toHaveLength(5);
    expect(result.reduce((sum, val) => sum + val, 0)).toBeCloseTo(1000000);
  });

  it('should handle single item distribution', () => {
    const result = distributeAmounts(100, 1, [5]);
    expect(result).toHaveLength(1);
    expect(result[0]).toBe(100);
  });
});