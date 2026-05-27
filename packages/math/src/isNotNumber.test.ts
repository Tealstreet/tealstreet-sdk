import { describe, expect, test } from 'vitest';
import { isNotNumber } from './isNotNumber';

describe('notNumber', () => {
  test('returns true for empty string', () => {
    expect(isNotNumber('')).toBe(true);
  });

  test('returns true for non-numeric strings', () => {
    expect(isNotNumber('abc')).toBe(true);
    expect(isNotNumber('123abc')).toBe(true);
  });

  test('returns true for null', () => {
    expect(isNotNumber(null)).toBe(true);
  });

  test('returns true for undefined', () => {
    expect(isNotNumber(undefined)).toBe(true);
  });

  test('returns false for numeric strings', () => {
    expect(isNotNumber('123')).toBe(false);
    expect(isNotNumber('4.56')).toBe(false);
  });

  test('returns false for numbers', () => {
    expect(isNotNumber(123)).toBe(false);
    expect(isNotNumber(0)).toBe(false);
    expect(isNotNumber(-456)).toBe(false);
    expect(isNotNumber(3.14159)).toBe(false);
  });

  test('returns true for other non-number types', () => {
    expect(isNotNumber({})).toBe(true);
    expect(isNotNumber([])).toBe(true);
    expect(isNotNumber(true)).toBe(true);
    expect(isNotNumber(() => {})).toBe(true);
  });
});
