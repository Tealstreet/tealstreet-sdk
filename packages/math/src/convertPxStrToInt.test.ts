import { describe, expect, test } from 'vitest';
import { convertPxStrToInt } from './convertPxStrToInt';

describe('convertPxStrToInt', () => {
  test('converts pixel string to integer', () => {
    expect(convertPxStrToInt('100px')).toBe(100);
    expect(convertPxStrToInt('50px')).toBe(50);
    expect(convertPxStrToInt('0px')).toBe(0);
  });

  test('handles large pixel values', () => {
    expect(convertPxStrToInt('12345px')).toBe(12345);
  });

  test('returns NaN for non-pixel strings', () => {
    expect(convertPxStrToInt('abc')).toBeNaN();
  });

  test('returns NaN for empty or invalid strings', () => {
    expect(convertPxStrToInt('')).toBeNaN();
    expect(convertPxStrToInt('px')).toBeNaN();
  });

  test('edge cases', () => {
    expect(convertPxStrToInt('100')).toBe(1);
  });
});
