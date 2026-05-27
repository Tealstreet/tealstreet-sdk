import { describe, expect, test } from 'vitest';
import { handleRoundingError } from './handleRoundingError';

describe('handleRoundingError', () => {
  test('handles numbers correctly', () => {
    expect(handleRoundingError(1.23456789012345)).toBeCloseTo(1.2345678901);
    expect(handleRoundingError(-2.3456789012345)).toBeCloseTo(-2.3456789012);
  });

  test('handles numeric strings correctly', () => {
    expect(handleRoundingError('3.456789012345')).toBeCloseTo(3.4567890123);
    expect(handleRoundingError('-4.56789012345')).toBeCloseTo(-4.5678901234);
  });

  test('returns 0 for non-numeric values', () => {
    expect(handleRoundingError('not a number')).toBe(0);
    expect(handleRoundingError(null)).toBe(0);
    expect(handleRoundingError(undefined)).toBe(0);
    expect(handleRoundingError({})).toBe(0);
    expect(handleRoundingError([])).toBe(0);
  });

  test('handles edge cases', () => {
    expect(handleRoundingError(0)).toBe(0);
    expect(handleRoundingError(NaN)).toBe(0);
    expect(handleRoundingError(Infinity)).toBe(Infinity);
    expect(handleRoundingError(-Infinity)).toBe(-Infinity);
  });
});
