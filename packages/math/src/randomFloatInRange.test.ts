import { describe, expect, it, vi } from 'vitest';
import { randomFloatInRange } from './randomFloatInRange';

describe('randomFloatInRange', () => {
  it('should return a value within the specified range', () => {
    const min = 0;
    const max = 10;
    
    for (let i = 0; i < 100; i++) {
      const result = randomFloatInRange(min, max);
      expect(result).toBeGreaterThanOrEqual(min);
      expect(result).toBeLessThan(max);
    }
  });

  it('should work with negative ranges', () => {
    const min = -20;
    const max = -10;
    
    for (let i = 0; i < 100; i++) {
      const result = randomFloatInRange(min, max);
      expect(result).toBeGreaterThanOrEqual(min);
      expect(result).toBeLessThan(max);
    }
  });

  it('should work with decimal ranges', () => {
    const min = 1.5;
    const max = 2.5;
    
    for (let i = 0; i < 100; i++) {
      const result = randomFloatInRange(min, max);
      expect(result).toBeGreaterThanOrEqual(min);
      expect(result).toBeLessThan(max);
    }
  });

  it('should work with very small ranges', () => {
    const min = 0.0001;
    const max = 0.0002;
    
    for (let i = 0; i < 100; i++) {
      const result = randomFloatInRange(min, max);
      expect(result).toBeGreaterThanOrEqual(min);
      expect(result).toBeLessThan(max);
    }
  });

  it('should work with very large ranges', () => {
    const min = 1000000;
    const max = 2000000;
    
    for (let i = 0; i < 100; i++) {
      const result = randomFloatInRange(min, max);
      expect(result).toBeGreaterThanOrEqual(min);
      expect(result).toBeLessThan(max);
    }
  });

  it('should return min when min equals max', () => {
    const value = 5;
    const result = randomFloatInRange(value, value);
    expect(result).toBe(value);
  });

  it('should work when max < min (inverted range)', () => {
    const min = 10;
    const max = 5;
    
    for (let i = 0; i < 100; i++) {
      const result = randomFloatInRange(min, max);
      // When max < min, the result will be between max and min
      expect(result).toBeGreaterThanOrEqual(max);
      expect(result).toBeLessThan(min);
    }
  });

  it('should return predictable values when Math.random is mocked', () => {
    const originalRandom = Math.random;
    
    // Test with mocked Math.random returning 0
    Math.random = vi.fn(() => 0);
    expect(randomFloatInRange(10, 20)).toBe(10);
    expect(randomFloatInRange(-5, 5)).toBe(-5);
    
    // Test with mocked Math.random returning 0.5
    Math.random = vi.fn(() => 0.5);
    expect(randomFloatInRange(10, 20)).toBe(15);
    expect(randomFloatInRange(-10, 10)).toBe(0);
    
    // Test with mocked Math.random returning 0.999999
    Math.random = vi.fn(() => 0.999999);
    expect(randomFloatInRange(10, 20)).toBeCloseTo(19.99999);
    expect(randomFloatInRange(0, 100)).toBeCloseTo(99.9999);
    
    // Restore original Math.random
    Math.random = originalRandom;
  });

  it('should handle zero ranges correctly', () => {
    expect(randomFloatInRange(0, 0)).toBe(0);
    
    const result1 = randomFloatInRange(0, 10);
    expect(result1).toBeGreaterThanOrEqual(0);
    expect(result1).toBeLessThan(10);
    
    const result2 = randomFloatInRange(-10, 0);
    expect(result2).toBeGreaterThanOrEqual(-10);
    expect(result2).toBeLessThan(0);
  });

  it('should maintain uniform distribution', () => {
    const min = 0;
    const max = 10;
    const buckets = 10;
    const samples = 10000;
    const counts = new Array(buckets).fill(0);
    
    for (let i = 0; i < samples; i++) {
      const value = randomFloatInRange(min, max);
      const bucketIndex = Math.floor((value - min) / (max - min) * buckets);
      counts[Math.min(bucketIndex, buckets - 1)]++;
    }
    
    // Each bucket should have roughly samples/buckets values (with some tolerance)
    const expectedCount = samples / buckets;
    const tolerance = expectedCount * 0.2; // 20% tolerance
    
    for (const count of counts) {
      expect(count).toBeGreaterThan(expectedCount - tolerance);
      expect(count).toBeLessThan(expectedCount + tolerance);
    }
  });
});