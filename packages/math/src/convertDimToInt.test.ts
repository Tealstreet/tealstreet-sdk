import { describe, expect, test } from 'vitest';
import { convertDimToInt } from './convertDimToInt';

describe('convertDimToInt', () => {
  test('returns the same number for numeric inputs', () => {
    expect(convertDimToInt(100)).toBe(100);
    expect(convertDimToInt(0)).toBe(0);
    expect(convertDimToInt(-50)).toBe(-50);
  });

  test('converts pixel string to integer', () => {
    expect(convertDimToInt('100px')).toBe(100);
    expect(convertDimToInt('50px')).toBe(50);
    expect(convertDimToInt('0px')).toBe(0);
  });

  test('parses non-pixel strings as integers', () => {
    expect(convertDimToInt('123')).toBe(123);
    expect(convertDimToInt('0')).toBe(0);
    expect(convertDimToInt('-45')).toBe(-45);
  });

  test('returns null for non-string, non-numeric inputs', () => {
    expect(convertDimToInt(null)).toBeNull();
    expect(convertDimToInt(undefined)).toBeNull();
    expect(convertDimToInt({})).toBeNull();
    expect(convertDimToInt([])).toBeNull();
  });

  test('handles invalid string inputs', () => {
    expect(convertDimToInt('abc')).toBeNaN();
    // borked? --rayana
    expect(convertDimToInt('100abc')).toBe(100);
    expect(convertDimToInt('px')).toBeNaN();
  });
});
