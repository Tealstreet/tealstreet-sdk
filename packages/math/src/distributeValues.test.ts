import { describe, expect, it, vi } from 'vitest';
import { distributeValues } from './distributeValues';

describe('distributeValues', () => {
  it('should throw error for empty distribution', () => {
    expect(() => distributeValues(0, 100, 10, [])).toThrow('Distribution cannot be empty');
  });

  it('should create evenly distributed values for uniform distribution', () => {
    const result = distributeValues(0, 100, 5, [1, 1, 1, 1]);
    expect(result).toHaveLength(5);
    expect(result[0]).toBe(0);
    expect(result[result.length - 1]).toBe(100);
    
    // Check that values are evenly spaced
    for (let i = 1; i < result.length; i++) {
      expect(result[i] - result[i - 1]).toBeCloseTo(25, 5);
    }
  });

  it('should create weighted distribution of values', () => {
    const result = distributeValues(0, 100, 5, [1, 2, 1]);
    expect(result).toHaveLength(5);
    expect(result[0]).toBe(0);
    expect(result[result.length - 1]).toBe(100);
    
    // Middle section should have more values due to weight of 2
    expect(result[1]).toBeGreaterThan(0);
    expect(result[1]).toBeLessThan(100);
  });

  it('should handle single distribution value', () => {
    const result = distributeValues(10, 20, 3, [1]);
    expect(result).toHaveLength(3);
    expect(result[0]).toBe(10);
    expect(result[result.length - 1]).toBe(20);
  });

  it('should handle large count values', () => {
    const result = distributeValues(0, 1000, 101, [1, 2, 3, 4]);
    expect(result).toHaveLength(101);
    expect(result[0]).toBe(0);
    expect(result[result.length - 1]).toBe(1000);
  });

  it('should handle negative start values', () => {
    const result = distributeValues(-50, 50, 5, [1, 1, 1, 1]);
    expect(result).toHaveLength(5);
    expect(result[0]).toBe(-50);
    expect(result[result.length - 1]).toBe(50);
  });

  it('should handle decimal values', () => {
    const result = distributeValues(0.1, 0.9, 5, [1, 1, 1, 1]);
    expect(result).toHaveLength(5);
    expect(result[0]).toBe(0.1);
    expect(result[result.length - 1]).toBe(0.9);
  });

  it('should log error when wrong number of entries are produced due to fractional count', () => {
    const consoleSpy = vi.spyOn(console, 'log').mockImplementation(() => {});

    // Using a fractional count causes the algorithm to produce a mismatched entry count
    const result = distributeValues(0, 100, 2.5, [1, 1, 1]);

    // The function should still return the end value
    expect(result[result.length - 1]).toBe(100);

    // The console.log should have been called with an error message
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('ERROR: Wrong number of entries')
    );

    consoleSpy.mockRestore();
  });
});