import { describe, expect, test } from 'vitest';
import { toFixedFloat } from './toFixedFloat';

describe('toFixedFloat', () => {
  test('correctly handles numeric values', () => {
    expect(toFixedFloat(1.2345, 2)).toBe(1.23);
    expect(toFixedFloat(-2.3456, 3)).toBe(-2.346);
    expect(toFixedFloat(123.456789, 4)).toBe(123.4568);
  });

  test('handles numeric strings correctly', () => {
    expect(toFixedFloat('3.4567', 1)).toBe(3.5);
    expect(toFixedFloat('-4.5678', 0)).toBe(-5);
  });

  test('returns 0 for non-numeric values', () => {
    expect(toFixedFloat('not a number', 2)).toBe(0);
    expect(toFixedFloat(null, 2)).toBe(0);
    expect(toFixedFloat(undefined, 2)).toBe(0);
    expect(toFixedFloat({}, 2)).toBe(0);
    expect(toFixedFloat([], 2)).toBe(0);
  });

  test('handles zero and extreme values', () => {
    expect(toFixedFloat(0, 2)).toBe(0);
    expect(toFixedFloat(Infinity, 2)).toBe(Infinity);
    expect(toFixedFloat(-Infinity, 2)).toBe(-Infinity);
    // shouldn't this be NaN ideally? --rayana
    expect(toFixedFloat(NaN, 2)).toBe(0);
  });

  test('handles various precisions', () => {
    expect(toFixedFloat(1.23456789, 5)).toBe(1.23457);
    expect(toFixedFloat(1.23456789, 0)).toBe(1);
    expect(toFixedFloat(1.23456789, 8)).toBe(1.23456789);
  });
});
